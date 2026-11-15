use std::path::Path;
use std::sync::atomic::AtomicBool;

use super::*;
use crate::domain::disk_usage::classify::Classifier;
use crate::domain::disk_usage::model::{Access, IssueReason, OpaqueReason};
use crate::domain::junk::walk::{WalkEnd, WalkReport, walk_directory};
use crate::domain::ports::PortError;
use crate::domain::ports::fakes::{FakeFileSystem, FsOp};
use crate::domain::protection::test_support::ctx_c_ana;

const MIB: u64 = 1024 * 1024;
const GIB: u64 = 1024 * MIB;

fn analyze(fs: &FakeFileSystem, root: &str) -> (FolderTree, WalkReport) {
    let ctx = ctx_c_ana();
    let classifier = Classifier::new(&ctx);
    let cancel = AtomicBool::new(false);
    let mut aggregator = FolderAggregator::new(&classifier, &cancel);
    let report = walk_directory(fs, Path::new(root), |_| false, &mut aggregator);
    (aggregator.finish(), report)
}

/// Busca un nodo por su ruta completa.
fn node<'t>(tree: &'t FolderTree, path: &str) -> &'t TreeNode {
    let found = (0..tree.nodes.len()).find(|index| tree.path_of(*index) == path);
    match found {
        Some(index) => &tree.nodes[index],
        None => panic!("no hay nodo para {path}"),
    }
}

fn paths(tree: &FolderTree) -> Vec<String> {
    (0..tree.nodes.len()).map(|i| tree.path_of(i)).collect()
}

fn sample_tree() -> FakeFileSystem {
    FakeFileSystem::new()
        .with_dir(r"C:\scan")
        .with_dir(r"C:\scan\a")
        .with_dir(r"C:\scan\a\b")
        .with_file(r"C:\scan\a\x.bin", 4 * MIB, None)
        .with_file(r"C:\scan\a\b\y.bin", 2 * MIB, None)
        .with_file(r"C:\scan\z.bin", MIB, None)
}

#[test]
fn aggregate_should_sum_sizes_and_counts_per_folder_of_a_known_tree() {
    let (tree, report) = analyze(&sample_tree(), r"C:\scan");
    assert_eq!(report.end, WalkEnd::Completed);
    let root = node(&tree, r"C:\scan");
    assert_eq!(
        (
            root.size_bytes,
            root.direct_bytes,
            root.file_count,
            root.dir_count
        ),
        (7 * MIB, MIB, 3, 2)
    );
    let a = node(&tree, r"C:\scan\a");
    assert_eq!(
        (a.size_bytes, a.direct_bytes, a.file_count, a.dir_count),
        (6 * MIB, 4 * MIB, 2, 1)
    );
    let b = node(&tree, r"C:\scan\a\b");
    assert_eq!(
        (b.size_bytes, b.direct_bytes, b.file_count, b.dir_count),
        (2 * MIB, 2 * MIB, 1, 0)
    );
    assert_eq!(root.depth, 0);
    assert_eq!(b.depth, 2);
    assert!(tree.nodes.iter().all(|n| n.access == Access::Complete));
}

#[test]
fn aggregate_should_create_parents_before_children_with_a_valid_parent_link() {
    let (tree, _) = analyze(&sample_tree(), r"C:\scan");
    assert_eq!(tree.nodes[0].parent, None);
    for (index, node) in tree.nodes.iter().enumerate().skip(1) {
        let parent = node.parent.expect("non-root nodes have a parent");
        assert!(parent < index, "el padre se crea antes que la hija");
        assert_eq!(node.depth, tree.nodes[parent].depth + 1);
    }
    assert_eq!(paths(&tree), vec![r"C:\scan", r"C:\scan\a", r"C:\scan\a\b"]);
}

#[test]
fn aggregate_should_use_the_volume_root_name_for_a_root_of_c_drive() {
    let fs = FakeFileSystem::new()
        .with_dir(r"C:\")
        .with_dir(r"C:\x")
        .with_file(r"C:\x\f.bin", 5, None);
    let (tree, _) = analyze(&fs, r"C:\");
    assert_eq!(paths(&tree), vec![r"C:\", r"C:\x"]);
    assert_eq!(tree.nodes[0].size_bytes, 5);
}

#[test]
fn aggregate_should_not_follow_junctions_or_sum_symlinked_files() {
    let fs = sample_tree()
        .with_dir(r"C:\outside")
        .with_file(r"C:\outside\big.bin", 32 * MIB, None)
        .with_reparse_point(r"C:\scan\link-outside")
        .with_reparse_point(r"C:\scan\a\link-file.bin");
    let (tree, _) = analyze(&fs, r"C:\scan");
    assert_eq!(tree.counters.reparse_skipped, 2);
    assert_eq!(node(&tree, r"C:\scan").size_bytes, 7 * MIB);
    assert!(!paths(&tree).iter().any(|p| p.contains("outside")));
}

#[test]
fn aggregate_should_count_cloud_files_apart_with_zero_bytes() {
    let fs = FakeFileSystem::new()
        .with_dir(r"C:\scan")
        .with_cloud_reparse_point(r"C:\scan\one.docx")
        .with_cloud_reparse_point(r"C:\scan\two.docx")
        .with_cloud_placeholder(r"C:\scan\three.docx", 5 * MIB)
        .with_file(r"C:\scan\real.bin", MIB, None);
    let (tree, _) = analyze(&fs, r"C:\scan");
    assert_eq!(tree.counters.cloud_files, 3);
    let root = node(&tree, r"C:\scan");
    assert_eq!((root.size_bytes, root.file_count), (MIB, 1));
}

#[test]
fn aggregate_should_mark_an_unlistable_folder_and_keep_summing_the_rest() {
    let fs = sample_tree().with_dir(r"C:\scan\locked").fail_on_op(
        r"C:\scan\locked",
        FsOp::ReadDir,
        PortError::PermissionDenied("denied".into()),
    );
    let (tree, report) = analyze(&fs, r"C:\scan");
    assert_eq!(report.inaccessible, 1);
    let locked = node(&tree, r"C:\scan\locked");
    assert_eq!(
        (locked.access, locked.size_bytes),
        (Access::Inaccessible, 0)
    );
    assert_eq!(node(&tree, r"C:\scan").access, Access::Incomplete);
    assert_eq!(node(&tree, r"C:\scan\a").access, Access::Complete);
    assert_eq!(node(&tree, r"C:\scan").size_bytes, 7 * MIB);
    assert_eq!(tree.counters.inaccessible_dirs, 1);
    assert_eq!(
        tree.issues,
        vec![FolderIssue {
            path: r"C:\scan\locked".into(),
            reason: IssueReason::AccessDenied
        }]
    );
}

#[test]
fn aggregate_should_propagate_incomplete_to_every_ancestor_of_a_nested_failure() {
    let fs = sample_tree().with_dir(r"C:\scan\a\b\gone").fail_on_op(
        r"C:\scan\a\b\gone",
        FsOp::ReadDir,
        PortError::NotFound("vanished".into()),
    );
    let (tree, _) = analyze(&fs, r"C:\scan");
    for path in [r"C:\scan", r"C:\scan\a", r"C:\scan\a\b"] {
        assert_eq!(node(&tree, path).access, Access::Incomplete, "{path}");
    }
    assert_eq!(tree.issues[0].reason, IssueReason::NotFound);
    assert_eq!(node(&tree, r"C:\scan").dir_count, 3);
}

#[test]
fn aggregate_should_count_all_inaccessible_folders_but_sample_only_fifty_in_walk_order() {
    let mut fs = FakeFileSystem::new().with_dir(r"C:\scan");
    for index in 0..120 {
        let dir = format!(r"C:\scan\d{index:03}");
        fs = fs.with_dir(&dir).fail_on_op(
            &dir,
            FsOp::ReadDir,
            PortError::PermissionDenied("denied".into()),
        );
    }
    let (tree, _) = analyze(&fs, r"C:\scan");
    assert_eq!(tree.counters.inaccessible_dirs, 120);
    assert_eq!(tree.issues.len(), 50);
    assert_eq!(tree.issues[0].path, r"C:\scan\d000");
    assert_eq!(tree.issues[49].path, r"C:\scan\d049");
}

#[test]
fn aggregate_should_report_an_unlistable_root_as_an_inaccessible_root() {
    let fs = FakeFileSystem::new().with_dir(r"C:\scan").fail_on_op(
        r"C:\scan",
        FsOp::ReadDir,
        PortError::PermissionDenied("denied".into()),
    );
    let (tree, _) = analyze(&fs, r"C:\scan");
    assert_eq!(tree.nodes.len(), 1);
    assert_eq!(tree.nodes[0].access, Access::Inaccessible);
    assert_eq!(tree.nodes[0].size_bytes, 0);
    assert_eq!(tree.issues.len(), 1);
}

fn machine_with_other_profile() -> FakeFileSystem {
    FakeFileSystem::new()
        .with_dir(r"C:\")
        .with_dir(r"C:\Users")
        .with_dir(r"C:\Users\ana")
        .with_file(r"C:\Users\ana\mine.bin", MIB, None)
        .with_dir(r"C:\Users\bob")
        .with_file(r"C:\Users\bob\secret.bin", 10 * GIB, None)
        .with_dir(r"C:\Users\bob\Documents")
        .with_file(r"C:\Users\bob\Documents\diary.txt", GIB, None)
        .with_dir(r"C:\Users\bob\locked")
        .fail_on_op(
            r"C:\Users\bob\locked",
            FsOp::ReadDir,
            PortError::PermissionDenied("denied".into()),
        )
        .with_dir(r"C:\$Recycle.Bin")
        .with_dir(r"C:\$Recycle.Bin\S-1-5-21")
        .with_file(r"C:\$Recycle.Bin\S-1-5-21\$R123.doc", 2 * MIB, None)
}

#[test]
fn aggregate_should_sum_an_opaque_profile_without_creating_nodes_for_its_contents() {
    let (tree, _) = analyze(&machine_with_other_profile(), r"C:\");
    let bob = node(&tree, r"C:\Users\bob");
    assert_eq!(bob.opaque, Some(OpaqueReason::OtherUserProfile));
    assert_eq!(bob.size_bytes, 11 * GIB);
    assert_eq!(bob.file_count, 2);
    assert_eq!(bob.dir_count, 2);
    assert_eq!(bob.access, Access::Incomplete);
    assert!(
        !paths(&tree)
            .iter()
            .any(|p| p.contains("Documents") || p.contains("secret")),
        "{:?}",
        paths(&tree)
    );
    assert_eq!(tree.counters.opaque_dirs, 2);
    assert_eq!(tree.counters.inaccessible_dirs, 1, "solo suma al contador");
    assert!(tree.issues.iter().all(|issue| !issue.path.contains("bob")));
}

#[test]
fn aggregate_should_treat_the_recycle_bin_as_opaque_and_keep_own_files_visible() {
    let (tree, _) = analyze(&machine_with_other_profile(), r"C:\");
    let bin = node(&tree, r"C:\$Recycle.Bin");
    assert_eq!(bin.opaque, Some(OpaqueReason::RecycleBin));
    assert_eq!(bin.size_bytes, 2 * MIB);
    assert!(!paths(&tree).iter().any(|p| p.contains("S-1-5-21")));
    assert_eq!(node(&tree, r"C:\Users\ana").opaque, None);
    assert_eq!(node(&tree, r"C:\Users\ana").size_bytes, MIB);
    assert_eq!(node(&tree, r"C:\").size_bytes, 11 * GIB + 3 * MIB);
}

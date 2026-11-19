use std::path::Path;
use std::sync::atomic::AtomicBool;

use super::*;
use crate::domain::disk_usage::aggregate::{FolderAggregator, FolderTree, TreeNode};
use crate::domain::disk_usage::classify::Classifier;
use crate::domain::disk_usage::model::{Access, MAX_LARGE_FILES};
use crate::domain::junk::walk::walk_directory;
use crate::domain::model::EpochMillis;
use crate::domain::ports::fakes::FakeFileSystem;
use crate::domain::protection::test_support::ctx_c_ana;

fn offer(top: &mut TopFiles, size: u64, path: &str) {
    top.offer(size, || (path.to_string(), None, 0));
}

fn paths(files: &[TopFile]) -> Vec<&str> {
    files.iter().map(|f| f.path.as_str()).collect()
}

#[test]
fn top_files_should_keep_only_the_largest_when_over_the_limit() {
    let mut top = TopFiles::new(MAX_LARGE_FILES);
    for index in 1..=2_000u64 {
        offer(&mut top, index, &format!(r"C:\scan\f{index}.bin"));
    }
    let files = top.into_sorted();
    assert_eq!(files.len(), 500);
    assert_eq!(files[0].size_bytes, 2_000);
    assert_eq!(files[499].size_bytes, 1_501);
    assert!(files.windows(2).all(|w| w[0].size_bytes > w[1].size_bytes));
}

#[test]
fn top_files_should_resolve_size_ties_by_folded_path() {
    let mut top = TopFiles::new(2);
    offer(&mut top, 10, r"C:\scan\Zeta.bin");
    offer(&mut top, 10, r"C:\scan\beta.bin");
    offer(&mut top, 10, r"C:\scan\Alfa.bin");
    offer(&mut top, 10, r"C:\scan\gamma.bin");
    assert_eq!(
        paths(&top.into_sorted()),
        vec![r"C:\scan\Alfa.bin", r"C:\scan\beta.bin"]
    );
}

#[test]
fn top_files_should_ignore_empty_files_and_keep_a_smaller_one_while_there_is_room() {
    let mut top = TopFiles::new(3);
    offer(&mut top, 0, r"C:\scan\empty.bin");
    offer(&mut top, 1, r"C:\scan\one.bin");
    let files = top.into_sorted();
    assert_eq!(paths(&files), vec![r"C:\scan\one.bin"]);
}

#[test]
fn top_files_should_not_build_the_entry_for_a_file_that_cannot_enter() {
    let mut top = TopFiles::new(1);
    offer(&mut top, 100, r"C:\scan\big.bin");
    let mut built = false;
    top.offer(5, || {
        built = true;
        (r"C:\scan\small.bin".to_string(), None, 0)
    });
    assert!(!built);
    assert_eq!(top.into_sorted().len(), 1);
}

fn node(parent: Option<usize>, depth: u32) -> TreeNode {
    TreeNode {
        parent,
        depth,
        name: format!("n{depth}"),
        size_bytes: 1,
        direct_bytes: 0,
        file_count: 0,
        dir_count: 0,
        access: Access::Complete,
        opaque: None,
    }
}

#[test]
fn nearest_kept_should_walk_up_to_the_closest_stored_ancestor() {
    // 0 raiz -> 1 -> 2 -> 3
    let tree = FolderTree {
        nodes: vec![
            node(None, 0),
            node(Some(0), 1),
            node(Some(1), 2),
            node(Some(2), 3),
        ],
        ..FolderTree::default()
    };
    let kept = [true, true, false, false];
    assert_eq!(nearest_kept(&tree, 3, &kept), 1);
    assert_eq!(nearest_kept(&tree, 2, &kept), 1);
    assert_eq!(nearest_kept(&tree, 1, &kept), 1);
    assert_eq!(nearest_kept(&tree, 0, &kept), 0);
    let all = [true; 4];
    assert_eq!(nearest_kept(&tree, 3, &all), 3);
}

fn run(fs: &FakeFileSystem, root: &str) -> FolderTree {
    let ctx = ctx_c_ana();
    let classifier = Classifier::new(&ctx);
    let cancel = AtomicBool::new(false);
    let mut aggregator = FolderAggregator::new(&classifier, &cancel);
    walk_directory(fs, Path::new(root), |_| false, &mut aggregator);
    aggregator.finish()
}

#[test]
fn aggregator_should_collect_the_500_largest_files_of_a_big_tree() {
    let mut fs = FakeFileSystem::new()
        .with_dir(r"C:\scan")
        .with_dir(r"C:\scan\sub");
    for index in 1..=2_000u64 {
        let dir = if index % 2 == 0 {
            r"C:\scan"
        } else {
            r"C:\scan\sub"
        };
        fs = fs.with_file(
            &format!(r"{dir}\f{index:04}.bin"),
            index,
            Some(EpochMillis(1_000)),
        );
    }
    let tree = run(&fs, r"C:\scan");
    assert_eq!(tree.top.len(), 500);
    assert_eq!(tree.top[0].size_bytes, 2_000);
    assert_eq!(tree.top[0].path, r"C:\scan\f2000.bin");
    assert_eq!(tree.top[0].modified, Some(EpochMillis(1_000)));
    assert_eq!(tree.top[0].node, 0, "carpeta directa: la raiz");
    let sub = &tree.top[1];
    assert_eq!(sub.path, r"C:\scan\sub\f1999.bin");
    assert_eq!(tree.path_of(sub.node), r"C:\scan\sub");
}

#[test]
fn aggregator_should_leave_opaque_folders_and_cloud_files_out_of_the_top() {
    let fs = FakeFileSystem::new()
        .with_dir(r"C:\")
        .with_dir(r"C:\Users")
        .with_dir(r"C:\Users\ana")
        .with_file(r"C:\Users\ana\mine.bin", 5, None)
        .with_cloud_placeholder(r"C:\Users\ana\cloud.bin", 9_000)
        .with_dir(r"C:\Users\bob")
        .with_file(r"C:\Users\bob\secret.bin", 1_000_000, None)
        .with_dir(r"C:\Users\bob\sub")
        .with_file(r"C:\Users\bob\sub\secret2.bin", 2_000_000, None)
        .with_dir(r"C:\$Recycle.Bin")
        .with_file(r"C:\$Recycle.Bin\$R1.bin", 3_000_000, None);
    let tree = run(&fs, r"C:\");
    let listed: Vec<&str> = tree.top.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(listed, vec![r"C:\Users\ana\mine.bin"]);
}

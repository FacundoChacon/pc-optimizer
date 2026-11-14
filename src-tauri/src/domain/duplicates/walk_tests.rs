use std::path::PathBuf;

use super::*;
use crate::domain::junk::walk::WalkEnd;
use crate::domain::ports::fakes::{FakeFileSystem, FsOp};
use crate::domain::ports::{DirEntry, PortError};

const ROOT: &str = r"C:\Users\ana\Documents";

#[derive(Default)]
struct Collect {
    files: Vec<String>,
    excluded: Vec<String>,
    reparse: Vec<String>,
    cancel_after: Option<usize>,
}

impl WalkVisitor for Collect {
    fn is_cancelled(&self) -> bool {
        self.cancel_after.is_some_and(|n| self.files.len() >= n)
    }
    fn file_seen(&mut self) {}
    fn candidate(&mut self, entry: DirEntry) {
        self.files.push(entry.path.display().to_string());
    }
    fn reparse_point(&mut self, path: PathBuf) {
        self.reparse.push(path.display().to_string());
    }
    fn excluded_dir(&mut self, path: PathBuf) {
        self.excluded.push(path.display().to_string());
    }
}

fn run(fs: &FakeFileSystem) -> (WalkReport, Collect) {
    let mut visitor = Collect::default();
    let report = walk_content_root(fs, Path::new(ROOT), &mut visitor);
    (report, visitor)
}

#[test]
fn is_excluded_dir_should_match_the_closed_list_case_insensitively() {
    for name in [
        ".git",
        ".GIT",
        ".hg",
        ".Hg",
        ".svn",
        "Node_Modules",
        "NODE_MODULES",
    ] {
        assert!(is_excluded_dir(name), "{name}");
    }
    for name in [
        "git",
        ".github",
        "node_modules2",
        "modules",
        ".gitignore",
        "venv",
        "target",
    ] {
        assert!(!is_excluded_dir(name), "{name}");
    }
}

#[test]
fn walk_should_not_descend_into_tool_folders_and_should_report_them() {
    let fs = FakeFileSystem::new()
        .with_dir(ROOT)
        .with_file(r"C:\Users\ana\Documents\a.bin", 10, None)
        .with_dir(r"C:\Users\ana\Documents\proyecto")
        .with_dir(r"C:\Users\ana\Documents\proyecto\Node_Modules")
        .with_file(
            r"C:\Users\ana\Documents\proyecto\Node_Modules\lib.bin",
            10,
            None,
        )
        .with_dir(r"C:\Users\ana\Documents\proyecto\.GIT")
        .with_dir(r"C:\Users\ana\Documents\proyecto\.GIT\objects")
        .with_file(
            r"C:\Users\ana\Documents\proyecto\.GIT\objects\p.pack",
            10,
            None,
        )
        .with_file(r"C:\Users\ana\Documents\proyecto\src.bin", 10, None);
    let (report, visitor) = run(&fs);
    assert_eq!(report.end, WalkEnd::Completed);
    assert_eq!(
        visitor.files,
        [
            r"C:\Users\ana\Documents\a.bin",
            r"C:\Users\ana\Documents\proyecto\src.bin"
        ]
    );
    assert_eq!(visitor.excluded.len(), 2);
    // Ni siquiera se listaron.
    let listed: Vec<String> = fs
        .calls()
        .into_iter()
        .filter(|(op, _)| *op == FsOp::ReadDir)
        .map(|(_, p)| p.display().to_string())
        .collect();
    assert!(
        listed
            .iter()
            .all(|p| !p.contains("Node_Modules") && !p.contains(".GIT"))
    );
}

#[test]
fn walk_should_not_follow_a_junction_inside_the_root() {
    let fs = FakeFileSystem::new()
        .with_dir(ROOT)
        .with_reparse_point(r"C:\Users\ana\Documents\j")
        .with_dir(r"C:\Windows")
        .with_file(r"C:\Windows\secret.bin", 10, None)
        .with_file(r"C:\Users\ana\Documents\ok.bin", 10, None);
    let (_, visitor) = run(&fs);
    assert_eq!(visitor.files, [r"C:\Users\ana\Documents\ok.bin"]);
    assert!(visitor.files.iter().all(|f| !f.contains("Windows")));
}

#[test]
fn walk_should_count_an_unreadable_folder_without_aborting() {
    let fs = FakeFileSystem::new()
        .with_dir(ROOT)
        .with_dir(r"C:\Users\ana\Documents\locked")
        .with_file(r"C:\Users\ana\Documents\locked\x.bin", 10, None)
        .with_file(r"C:\Users\ana\Documents\z.bin", 10, None)
        .fail_on_op(
            r"C:\Users\ana\Documents\locked",
            FsOp::ReadDir,
            PortError::PermissionDenied("x".into()),
        );
    let (report, visitor) = run(&fs);
    assert_eq!(report.inaccessible, 1);
    assert_eq!(visitor.files, [r"C:\Users\ana\Documents\z.bin"]);
}

#[test]
fn walk_should_stop_when_cancelled() {
    let fs = FakeFileSystem::new()
        .with_dir(ROOT)
        .with_file(r"C:\Users\ana\Documents\a.bin", 10, None)
        .with_file(r"C:\Users\ana\Documents\b.bin", 10, None)
        .with_file(r"C:\Users\ana\Documents\c.bin", 10, None);
    let mut visitor = Collect {
        cancel_after: Some(1),
        ..Collect::default()
    };
    let report = walk_content_root(&fs, Path::new(ROOT), &mut visitor);
    assert_eq!(report.end, WalkEnd::Cancelled);
    assert_eq!(visitor.files.len(), 1);
}

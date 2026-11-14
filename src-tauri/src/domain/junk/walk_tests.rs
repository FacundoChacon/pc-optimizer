use std::path::{Path, PathBuf};

use super::*;
use crate::domain::junk::category::JunkCategory;
use crate::domain::junk::filter::{Filter, NameFilter, TEMP_MIN_AGE_MS};
use crate::domain::junk::sources::{Depth, Layout};
use crate::domain::model::EpochMillis;
use crate::domain::ports::PortError;
use crate::domain::ports::fakes::{FakeFileSystem, FsOp};
use crate::domain::privileges::UserScope;
use crate::domain::protection::path::normalize_lexical;

const NOW: EpochMillis = EpochMillis(1_800_000_000_000);
const DAY: i64 = 86_400_000;
const TEMP: &str = r"C:\Windows\Temp";

#[derive(Default)]
struct Collect {
    cancelled: bool,
    cancel_after_first_candidate: bool,
    seen: u64,
    candidates: Vec<PathBuf>,
    reparse: Vec<PathBuf>,
}

impl WalkVisitor for Collect {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    fn file_seen(&mut self) {
        self.seen += 1;
    }
    fn candidate(&mut self, entry: DirEntry) {
        self.candidates.push(entry.path);
        if self.cancel_after_first_candidate {
            self.cancelled = true;
        }
    }
    fn reparse_point(&mut self, path: PathBuf) {
        self.reparse.push(path);
    }
}

fn source(root: &str, filter: Filter, depth: Depth, layout: Layout) -> JunkSource {
    JunkSource {
        category: JunkCategory::TempWindows,
        root: normalize_lexical(root).expect("ruta valida"),
        scope: UserScope::Machine,
        filter,
        depth,
        layout,
    }
}

fn tree(root: &str) -> JunkSource {
    source(root, Filter::NONE, Depth::Recursive, Layout::Direct)
}

fn run(fs: &FakeFileSystem, source: &JunkSource) -> (WalkReport, Collect) {
    let mut visitor = Collect::default();
    let report = walk_source(fs, source, NOW, &mut visitor);
    (report, visitor)
}

fn names(paths: &[PathBuf]) -> Vec<String> {
    paths.iter().map(|p| p.display().to_string()).collect()
}

fn read_dirs(fs: &FakeFileSystem) -> Vec<String> {
    fs.calls()
        .into_iter()
        .filter(|(op, _)| *op == FsOp::ReadDir)
        .map(|(_, path)| path.display().to_string())
        .collect()
}

fn temp_tree() -> FakeFileSystem {
    FakeFileSystem::new()
        .with_dir(TEMP)
        .with_file(r"C:\Windows\Temp\a.tmp", 10, None)
        .with_dir(r"C:\Windows\Temp\d")
        .with_file(r"C:\Windows\Temp\d\b.tmp", 20, None)
        .with_dir(r"C:\Windows\Temp\d\e")
        .with_file(r"C:\Windows\Temp\d\e\c.tmp", 30, None)
}

#[test]
fn walk_should_list_every_file_of_the_tree_in_a_deterministic_order() {
    let fs = temp_tree();
    let (report, visitor) = run(&fs, &tree(TEMP));
    assert_eq!(report.end, WalkEnd::Completed);
    assert!(report.present);
    assert_eq!(report.inaccessible, 0);
    assert_eq!(
        names(&visitor.candidates),
        vec![
            r"C:\Windows\Temp\a.tmp",
            r"C:\Windows\Temp\d\b.tmp",
            r"C:\Windows\Temp\d\e\c.tmp",
        ]
    );
    assert_eq!(visitor.seen, 3);
    assert!(visitor.reparse.is_empty());
}

#[test]
fn walk_should_order_siblings_by_folded_name_not_by_insertion() {
    let fs = FakeFileSystem::new()
        .with_dir(TEMP)
        .with_file(r"C:\Windows\Temp\b.tmp", 1, None)
        .with_file(r"C:\Windows\Temp\B2.tmp", 1, None)
        .with_file(r"C:\Windows\Temp\a.tmp", 1, None);
    let (_, visitor) = run(&fs, &tree(TEMP));
    assert_eq!(
        names(&visitor.candidates),
        vec![
            r"C:\Windows\Temp\a.tmp",
            r"C:\Windows\Temp\b.tmp",
            r"C:\Windows\Temp\B2.tmp",
        ]
    );
}

#[test]
fn walk_should_skip_a_junction_to_windows_without_listing_its_target() {
    let fs = temp_tree()
        .with_reparse_point(r"C:\Windows\Temp\j")
        .with_dir(r"C:\Windows\System32")
        .with_file(r"C:\Windows\System32\x.dll", 5, None);
    let (_, visitor) = run(&fs, &tree(TEMP));
    assert_eq!(names(&visitor.reparse), vec![r"C:\Windows\Temp\j"]);
    assert!(
        !names(&visitor.candidates)
            .iter()
            .any(|p| p.contains("System32"))
    );
    assert!(
        read_dirs(&fs)
            .iter()
            .all(|p| !p.contains(r"Temp\j") && !p.contains("System32")),
        "{:?}",
        read_dirs(&fs)
    );
}

#[test]
fn walk_should_skip_a_symlink_to_a_file_and_not_count_it_as_candidate() {
    let fs = FakeFileSystem::new()
        .with_dir(TEMP)
        .with_reparse_point(r"C:\Windows\Temp\link.dll")
        .with_file(r"C:\Windows\Temp\ok.tmp", 4, None);
    let (_, visitor) = run(&fs, &tree(TEMP));
    assert_eq!(names(&visitor.reparse), vec![r"C:\Windows\Temp\link.dll"]);
    assert_eq!(names(&visitor.candidates), vec![r"C:\Windows\Temp\ok.tmp"]);
    assert_eq!(visitor.seen, 1);
}

#[test]
fn walk_should_treat_a_cloud_reparse_point_as_a_candidate_for_the_protection_check() {
    let fs = FakeFileSystem::new()
        .with_dir(TEMP)
        .with_cloud_reparse_point(r"C:\Windows\Temp\nube.docx");
    let (_, visitor) = run(&fs, &tree(TEMP));
    assert_eq!(
        names(&visitor.candidates),
        vec![r"C:\Windows\Temp\nube.docx"]
    );
    assert!(visitor.reparse.is_empty());
}

#[test]
fn walk_should_count_an_unlistable_folder_and_keep_going() {
    let fs = temp_tree().fail_on_op(
        r"C:\Windows\Temp\d",
        FsOp::ReadDir,
        PortError::PermissionDenied("d".into()),
    );
    let (report, visitor) = run(&fs, &tree(TEMP));
    assert_eq!(report.end, WalkEnd::Completed);
    assert_eq!(report.inaccessible, 1);
    assert_eq!(names(&visitor.candidates), vec![r"C:\Windows\Temp\a.tmp"]);
}

#[test]
fn walk_should_count_each_failing_error_kind_as_one_inaccessible_folder() {
    for error in [
        PortError::InUse("x".into()),
        PortError::Io("x".into()),
        PortError::NotFound("x".into()),
    ] {
        let fs = temp_tree().fail_on_op(r"C:\Windows\Temp\d", FsOp::ReadDir, error.clone());
        let (report, _) = run(&fs, &tree(TEMP));
        assert_eq!(report.inaccessible, 1, "{error:?}");
    }
}

#[test]
fn walk_should_report_a_missing_root_as_absent_without_error() {
    let fs = FakeFileSystem::new().with_dir(r"C:\Windows");
    let (report, visitor) = run(&fs, &tree(TEMP));
    assert!(!report.present);
    assert_eq!(report.inaccessible, 0);
    assert_eq!(report.end, WalkEnd::Completed);
    assert!(visitor.candidates.is_empty());
    assert!(read_dirs(&fs).is_empty());
}

#[test]
fn walk_should_not_enter_a_root_that_is_a_reparse_point() {
    let fs = FakeFileSystem::new().with_reparse_point(TEMP);
    let (report, visitor) = run(&fs, &tree(TEMP));
    assert!(report.present);
    assert_eq!(report.inaccessible, 1);
    assert!(visitor.candidates.is_empty() && visitor.reparse.is_empty());
    assert!(read_dirs(&fs).is_empty());
}

#[test]
fn walk_should_count_a_root_that_cannot_be_inspected_or_listed_as_inaccessible() {
    let denied = FakeFileSystem::new().with_dir(TEMP).fail_on_op(
        TEMP,
        FsOp::Metadata,
        PortError::PermissionDenied("t".into()),
    );
    assert_eq!(run(&denied, &tree(TEMP)).0.inaccessible, 1);
    let unlistable = FakeFileSystem::new().with_dir(TEMP).fail_on_op(
        TEMP,
        FsOp::ReadDir,
        PortError::PermissionDenied("t".into()),
    );
    let (report, visitor) = run(&unlistable, &tree(TEMP));
    assert_eq!(report.inaccessible, 1);
    assert!(visitor.candidates.is_empty());
}

#[test]
fn walk_should_stay_in_the_first_level_when_the_depth_says_so() {
    let explorer = r"C:\Users\ana\AppData\Local\Microsoft\Windows\Explorer";
    let fs = FakeFileSystem::new()
        .with_dir(explorer)
        .with_file(&format!(r"{explorer}\thumbcache_256.db"), 9, None)
        .with_file(&format!(r"{explorer}\THUMBCACHE_IDX.DB"), 9, None)
        .with_file(&format!(r"{explorer}\iconcache_32.db"), 9, None)
        .with_dir(&format!(r"{explorer}\sub"))
        .with_file(&format!(r"{explorer}\sub\thumbcache_x.db"), 9, None);
    let thumbs = source(
        explorer,
        Filter {
            min_age_ms: None,
            name: NameFilter::ThumbCache,
        },
        Depth::FirstLevel,
        Layout::Direct,
    );
    let (_, visitor) = run(&fs, &thumbs);
    assert_eq!(
        names(&visitor.candidates),
        vec![
            format!(r"{explorer}\thumbcache_256.db"),
            format!(r"{explorer}\THUMBCACHE_IDX.DB"),
        ]
    );
    assert_eq!(
        visitor.seen, 3,
        "iconcache cuenta como visto, no como candidato"
    );
    assert_eq!(read_dirs(&fs), vec![explorer.to_string()]);
}

#[test]
fn walk_should_apply_the_age_filter_and_ignore_recent_or_undated_files() {
    let fs = FakeFileSystem::new()
        .with_dir(TEMP)
        .with_file(
            r"C:\Windows\Temp\viejo.tmp",
            1,
            Some(EpochMillis(NOW.0 - 3 * DAY)),
        )
        .with_file(
            r"C:\Windows\Temp\nuevo.tmp",
            1,
            Some(EpochMillis(NOW.0 - 2 * 3_600_000)),
        )
        .with_file(r"C:\Windows\Temp\sin-fecha.tmp", 1, None);
    let aged = source(
        TEMP,
        Filter {
            min_age_ms: Some(TEMP_MIN_AGE_MS),
            name: NameFilter::Any,
        },
        Depth::Recursive,
        Layout::Direct,
    );
    let (_, visitor) = run(&fs, &aged);
    assert_eq!(
        names(&visitor.candidates),
        vec![r"C:\Windows\Temp\viejo.tmp"]
    );
    assert_eq!(visitor.seen, 3);
}

#[test]
fn walk_should_only_probe_the_exact_cache_folders_of_each_browser_profile() {
    let user_data = r"C:\Users\ana\AppData\Local\Google\Chrome\User Data";
    let p = |rest: &str| format!(r"{user_data}\{rest}");
    let fs = FakeFileSystem::new()
        .with_dir(user_data)
        .with_dir(&p("Default"))
        .with_dir(&p(r"Default\Cache"))
        .with_file(&p(r"Default\Cache\f1"), 5, None)
        .with_dir(&p(r"Default\Network"))
        .with_file(&p(r"Default\Network\Cookies"), 5, None)
        .with_file(&p(r"Default\Login Data"), 5, None)
        .with_dir(&p("Profile 2"))
        .with_dir(&p(r"Profile 2\Code Cache"))
        .with_file(&p(r"Profile 2\Code Cache\f2"), 5, None)
        .with_file(&p("Local State"), 5, None)
        .with_reparse_point(&p("j"));
    let chrome = source(
        user_data,
        Filter::NONE,
        Depth::Recursive,
        Layout::Profiles {
            subdirs: &["Cache", "Code Cache", "GPUCache"],
        },
    );
    let (report, visitor) = run(&fs, &chrome);
    assert_eq!(report.end, WalkEnd::Completed);
    assert!(report.present);
    assert_eq!(report.inaccessible, 0, "GPUCache ausente no es un error");
    assert_eq!(
        names(&visitor.candidates),
        vec![p(r"Default\Cache\f1"), p(r"Profile 2\Code Cache\f2")]
    );
    assert!(
        visitor.reparse.is_empty(),
        "los hijos de User Data no son hallazgos"
    );
    let touched: Vec<String> = fs
        .calls()
        .iter()
        .map(|(_, path)| path.display().to_string())
        .collect();
    for forbidden in [
        "Cookies",
        "Network",
        "Login Data",
        "Local State",
        r"User Data\j",
    ] {
        assert!(
            touched.iter().all(|path| !path.contains(forbidden)),
            "{forbidden}: {touched:?}"
        );
    }
}

#[test]
fn walk_should_report_a_browser_that_is_not_installed_as_absent() {
    let fs = FakeFileSystem::new().with_dir(r"C:\Users\ana\AppData\Local");
    let brave = source(
        r"C:\Users\ana\AppData\Local\BraveSoftware\Brave-Browser\User Data",
        Filter::NONE,
        Depth::Recursive,
        Layout::Profiles {
            subdirs: &["Cache"],
        },
    );
    let (report, visitor) = run(&fs, &brave);
    assert!(!report.present);
    assert_eq!(report.inaccessible, 0);
    assert!(visitor.candidates.is_empty());
}

#[test]
fn walk_should_stop_immediately_when_cancelled_before_starting() {
    let fs = temp_tree();
    let mut visitor = Collect {
        cancelled: true,
        ..Collect::default()
    };
    let report = walk_source(&fs, &tree(TEMP), NOW, &mut visitor);
    assert_eq!(report.end, WalkEnd::Cancelled);
    assert_eq!(fs.total_calls(), 0);
}

#[test]
fn walk_should_stop_at_the_next_entry_when_cancelled_midway() {
    let fs = temp_tree();
    let mut visitor = Collect {
        cancel_after_first_candidate: true,
        ..Collect::default()
    };
    let report = walk_source(&fs, &tree(TEMP), NOW, &mut visitor);
    assert_eq!(report.end, WalkEnd::Cancelled);
    assert_eq!(names(&visitor.candidates), vec![r"C:\Windows\Temp\a.tmp"]);
    assert_eq!(read_dirs(&fs), vec![TEMP.to_string()]);
}

#[test]
fn walk_should_only_issue_read_operations() {
    let fs = temp_tree().with_reparse_point(r"C:\Windows\Temp\j");
    let _ = run(&fs, &tree(TEMP));
    assert!(
        fs.calls()
            .iter()
            .all(|(op, _)| matches!(op, FsOp::Metadata | FsOp::ReadDir)),
        "{:?}",
        fs.calls()
    );
    assert!(fs.calls().iter().any(|(_, p)| p == Path::new(TEMP)));
}

#[test]
fn walk_should_only_report_reparse_points_whose_name_passes_the_name_filter() {
    let explorer = r"C:\Users\ana\AppData\Local\Microsoft\Windows\Explorer";
    let fs = FakeFileSystem::new()
        .with_dir(explorer)
        .with_reparse_point(&format!(r"{explorer}\thumbcache_256.db"))
        .with_reparse_point(&format!(r"{explorer}\otra-junction"));
    let thumbs = source(
        explorer,
        Filter {
            min_age_ms: None,
            name: NameFilter::ThumbCache,
        },
        Depth::FirstLevel,
        Layout::Direct,
    );
    let (_, visitor) = run(&fs, &thumbs);
    assert_eq!(
        names(&visitor.reparse),
        vec![format!(r"{explorer}\thumbcache_256.db")]
    );
}

#[test]
fn walk_should_count_a_root_that_is_a_file_as_inaccessible() {
    let fs = FakeFileSystem::new().with_file(TEMP, 3, None);
    let (report, visitor) = run(&fs, &tree(TEMP));
    assert!(report.present);
    assert_eq!(report.inaccessible, 1);
    assert!(visitor.candidates.is_empty());
    assert!(read_dirs(&fs).is_empty());
}

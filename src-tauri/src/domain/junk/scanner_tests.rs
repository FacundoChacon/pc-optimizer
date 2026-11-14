use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::*;
use crate::domain::junk::category::CategoryScope;
use crate::domain::junk::model::{
    CategoryResult, CategoryStatus, FindingState, JunkFinding, JunkScanResults,
};
use crate::domain::ports::PortError;
use crate::domain::ports::fakes::{FakeFileSystem, FakeRecycleBin, FakeTrash, FsOp};
use crate::domain::privileges::{
    MismatchReason, PrivilegeContext, UserMatch, restrict_protection_context,
};
use crate::domain::protection::context::{ContextError, ProtectionContext, RawProtectionContext};
use crate::domain::protection::test_support::raw_c_ana;

const NOW: EpochMillis = EpochMillis(1_800_000_000_000);
const DAY: i64 = 86_400_000;
const SYSTEM_DRIVE: Option<char> = Some('C');

fn old() -> Option<EpochMillis> {
    Some(EpochMillis(NOW.0 - 3 * DAY))
}

fn matched() -> PrivilegeContext {
    PrivilegeContext::new(UserMatch::Match, false)
}

fn mismatched(reason: MismatchReason) -> PrivilegeContext {
    PrivilegeContext::new(UserMatch::Mismatch(reason), false)
}

fn ctx_for(privileges: &PrivilegeContext) -> Result<ProtectionContext, ContextError> {
    ProtectionContext::from_raw(restrict_protection_context(raw_c_ana(), privileges))
}

/// Agrega un archivo y todas las carpetas que lo contienen (sin la raiz del volumen).
fn with_file_in_tree(mut fs: FakeFileSystem, path: &str, size: u64) -> FakeFileSystem {
    let mut dirs: Vec<PathBuf> = PathBuf::from(path)
        .ancestors()
        .skip(1)
        .filter(|a| a.parent().is_some())
        .map(PathBuf::from)
        .collect();
    dirs.reverse();
    for dir in dirs {
        fs = fs.with_dir(&dir.display().to_string());
    }
    fs.with_file(path, size, old())
}

/// Un archivo viejo en cada ubicacion de la whitelist para el usuario `ana`.
fn populated_fs() -> FakeFileSystem {
    let files: [(&str, u64); 12] = [
        (r"C:\Users\ana\AppData\Local\Temp\t.tmp", 2_000),
        (r"C:\Windows\Temp\w.tmp", 1_000),
        (
            r"C:\Users\ana\AppData\Local\Google\Chrome\User Data\Default\Cache\c1",
            4_000,
        ),
        (
            r"C:\Users\ana\AppData\Local\Microsoft\Edge\User Data\Default\Code Cache\e1",
            5_000,
        ),
        (
            r"C:\Users\ana\AppData\Local\Mozilla\Firefox\Profiles\p1.default\cache2\f1",
            6_000,
        ),
        (
            r"C:\Users\ana\AppData\Local\BraveSoftware\Brave-Browser\User Data\Profile 2\GPUCache\b1",
            7_000,
        ),
        (r"C:\Windows\SoftwareDistribution\Download\w.bin", 3_000),
        (
            r"C:\Users\ana\AppData\Local\Microsoft\Windows\Explorer\thumbcache_256.db",
            8_000,
        ),
        (r"C:\Users\ana\AppData\Local\CrashDumps\u.dmp", 9_000),
        (r"C:\Windows\Minidump\m.dmp", 10_000),
        (
            r"C:\ProgramData\Microsoft\Windows\WER\ReportArchive\r1",
            11_000,
        ),
        (
            r"C:\ProgramData\Microsoft\Windows\WER\ReportQueue\r2",
            12_000,
        ),
    ];
    let mut fs = FakeFileSystem::new().canonical_as_is();
    for (path, size) in files {
        fs = with_file_in_tree(fs, path, size);
    }
    // Un archivo que NO es thumbcache y datos de perfil que nunca deben aparecer.
    fs = with_file_in_tree(
        fs,
        r"C:\Users\ana\AppData\Local\Microsoft\Windows\Explorer\iconcache_32.db",
        99,
    );
    with_file_in_tree(
        fs,
        r"C:\Users\ana\AppData\Local\Google\Chrome\User Data\Default\Network\Cookies",
        77,
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Event {
    Category(JunkCategory),
    Progress {
        category: JunkCategory,
        files_seen: u64,
        eligible_bytes: u64,
    },
}

#[derive(Default)]
struct Recorder {
    events: Vec<Event>,
}

impl ScanObserver for Recorder {
    fn category(&mut self, category: JunkCategory) {
        self.events.push(Event::Category(category));
    }
    fn progress(&mut self, category: JunkCategory, files_seen: u64, eligible_bytes: u64) {
        self.events.push(Event::Progress {
            category,
            files_seen,
            eligible_bytes,
        });
    }
}

fn run_with(
    privileges: &PrivilegeContext,
    protection: Result<ProtectionContext, ContextError>,
    fs: &FakeFileSystem,
    bin: &FakeRecycleBin,
) -> JunkScanOutcome {
    // Como el comando: la unidad sale de la ancla de Windows aunque el contexto no se valide.
    let drive = protection
        .as_ref()
        .map_or(SYSTEM_DRIVE, |ctx| Some(ctx.system_drive()));
    run_with_drive(privileges, protection, drive, fs, bin)
}

fn run_with_drive(
    privileges: &PrivilegeContext,
    protection: Result<ProtectionContext, ContextError>,
    system_drive: Option<char>,
    fs: &FakeFileSystem,
    bin: &FakeRecycleBin,
) -> JunkScanOutcome {
    let cancel = AtomicBool::new(false);
    scan_junk(
        privileges,
        protection,
        system_drive,
        fs,
        bin,
        NOW,
        &cancel,
        &mut Recorder::default(),
    )
}

fn completed(outcome: JunkScanOutcome) -> JunkScanResults {
    match outcome {
        JunkScanOutcome::Completed(results) => results,
        JunkScanOutcome::Cancelled => panic!("el escaneo no debia cancelarse"),
    }
}

fn category(results: &JunkScanResults, wanted: JunkCategory) -> CategoryResult {
    match results.categories.iter().find(|c| c.category == wanted) {
        Some(found) => *found,
        None => panic!("{wanted:?} falta en el resumen"),
    }
}

fn findings_of(results: &JunkScanResults, wanted: JunkCategory) -> Vec<&JunkFinding> {
    results
        .findings
        .iter()
        .filter(|f| f.category == wanted)
        .collect()
}

fn bin_12_items() -> FakeRecycleBin {
    FakeRecycleBin::with_usage(12, 340_000_000)
}

// ---- 4.2 tabla por categoria ----

#[test]
fn scan_should_summarize_every_category_with_correct_totals_on_a_match() {
    let fs = populated_fs();
    let privileges = matched();
    let results = completed(run_with(
        &privileges,
        ctx_for(&privileges),
        &fs,
        &bin_12_items(),
    ));

    let order: Vec<JunkCategory> = results.categories.iter().map(|c| c.category).collect();
    assert_eq!(order, JunkCategory::ALL.to_vec());
    for (wanted, items, bytes) in [
        (JunkCategory::TempUser, 1, 2_000),
        (JunkCategory::TempWindows, 1, 1_000),
        (JunkCategory::BrowserCacheChrome, 1, 4_000),
        (JunkCategory::BrowserCacheEdge, 1, 5_000),
        (JunkCategory::BrowserCacheFirefox, 1, 6_000),
        (JunkCategory::BrowserCacheBrave, 1, 7_000),
        (JunkCategory::WuCache, 1, 3_000),
        (JunkCategory::ThumbnailCache, 1, 8_000),
        (JunkCategory::CrashDumps, 4, 42_000),
    ] {
        let result = category(&results, wanted);
        assert_eq!(result.status, CategoryStatus::Done, "{wanted:?}");
        assert_eq!(result.block_reason, None, "{wanted:?}");
        assert_eq!(result.item_count, items, "{wanted:?}");
        assert_eq!(result.size_bytes, bytes, "{wanted:?}");
        assert_eq!(result.skipped_count, 0, "{wanted:?}");
        assert_eq!(result.inaccessible_count, 0, "{wanted:?}");
        assert_eq!(result.scope, wanted.scope(), "{wanted:?}");
    }
    assert_eq!(results.total_items(), 12);
    assert_eq!(results.total_bytes(), 78_000);
    assert_eq!(results.findings.len(), 12);
    assert!(results.findings.iter().all(JunkFinding::is_eligible));
}

#[test]
fn scan_should_never_list_browser_profile_data_or_non_thumbcache_files() {
    let fs = populated_fs();
    let privileges = matched();
    let results = completed(run_with(
        &privileges,
        ctx_for(&privileges),
        &fs,
        &bin_12_items(),
    ));
    for finding in &results.findings {
        assert!(!finding.path.contains("Cookies"), "{}", finding.path);
        assert!(!finding.path.contains("iconcache"), "{}", finding.path);
    }
    let touched: Vec<String> = fs
        .calls()
        .iter()
        .map(|(_, p)| p.display().to_string())
        .collect();
    assert!(
        touched.iter().all(|p| !p.contains("Cookies")),
        "{touched:?}"
    );
}

#[test]
fn scan_should_report_old_logs_as_unavailable_without_locations() {
    let fs = populated_fs();
    let privileges = matched();
    let results = completed(run_with(
        &privileges,
        ctx_for(&privileges),
        &fs,
        &bin_12_items(),
    ));
    let logs = category(&results, JunkCategory::OldLogs);
    assert_eq!(logs.status, CategoryStatus::Unavailable);
    assert_eq!(logs.block_reason, None);
    assert_eq!((logs.item_count, logs.size_bytes), (0, 0));
    assert!(findings_of(&results, JunkCategory::OldLogs).is_empty());
}

#[test]
fn scan_should_measure_the_recycle_bin_of_the_system_drive_without_adding_it_to_totals() {
    let fs = populated_fs();
    let bin = bin_12_items();
    let privileges = matched();
    let results = completed(run_with(&privileges, ctx_for(&privileges), &fs, &bin));
    let measured = category(&results, JunkCategory::RecycleBin);
    assert_eq!(measured.status, CategoryStatus::Done);
    assert_eq!(
        (measured.item_count, measured.size_bytes),
        (12, 340_000_000)
    );
    assert_eq!(measured.scope, CategoryScope::PerUser);
    assert!(findings_of(&results, JunkCategory::RecycleBin).is_empty());
    assert_eq!(bin.queried(), vec![PathBuf::from(r"C:\")]);
    assert_eq!(results.total_bytes(), 78_000, "la Papelera no suma");
}

#[test]
fn scan_should_leave_the_recycle_bin_unavailable_when_the_query_fails() {
    let fs = populated_fs();
    let bin = FakeRecycleBin::failing(PortError::Io("shell".into()));
    let privileges = matched();
    let results = completed(run_with(&privileges, ctx_for(&privileges), &fs, &bin));
    let measured = category(&results, JunkCategory::RecycleBin);
    assert_eq!(measured.status, CategoryStatus::Unavailable);
    assert_eq!((measured.item_count, measured.size_bytes), (0, 0));
    assert_eq!(category(&results, JunkCategory::TempUser).item_count, 1);
}

#[test]
fn scan_should_query_the_recycle_bin_of_the_windows_drive() {
    let bin = bin_12_items();
    let privileges = matched();
    let raw = crate::domain::protection::test_support::raw_for('D', "ana");
    let ctx = ProtectionContext::from_raw(raw);
    let _ = completed(run_with(&privileges, ctx, &FakeFileSystem::new(), &bin));
    assert_eq!(bin.queried(), vec![PathBuf::from(r"D:\")]);
}

#[test]
fn scan_should_block_per_user_categories_on_a_different_user_without_touching_the_profile() {
    let fs = populated_fs();
    let bin = bin_12_items();
    let privileges = mismatched(MismatchReason::DifferentUser);
    let results = completed(run_with(&privileges, ctx_for(&privileges), &fs, &bin));

    for blocked in [
        JunkCategory::TempUser,
        JunkCategory::BrowserCacheChrome,
        JunkCategory::BrowserCacheEdge,
        JunkCategory::BrowserCacheFirefox,
        JunkCategory::BrowserCacheBrave,
        JunkCategory::ThumbnailCache,
        JunkCategory::RecycleBin,
    ] {
        let result = category(&results, blocked);
        assert_eq!(result.status, CategoryStatus::Blocked, "{blocked:?}");
        assert_eq!(
            result.block_reason,
            Some(MismatchReason::DifferentUser),
            "{blocked:?}"
        );
        assert_eq!(
            (result.item_count, result.size_bytes),
            (0, 0),
            "{blocked:?}"
        );
        assert!(findings_of(&results, blocked).is_empty(), "{blocked:?}");
    }
    let dumps = category(&results, JunkCategory::CrashDumps);
    assert_eq!(dumps.status, CategoryStatus::Partial);
    assert_eq!(dumps.block_reason, Some(MismatchReason::DifferentUser));
    assert_eq!((dumps.item_count, dumps.size_bytes), (3, 33_000));
    for machine in [JunkCategory::TempWindows, JunkCategory::WuCache] {
        let result = category(&results, machine);
        assert_eq!(result.status, CategoryStatus::Done, "{machine:?}");
        assert_eq!(result.item_count, 1, "{machine:?}");
    }
    assert!(bin.queried().is_empty(), "la Papelera es por-usuario");
    let touched: Vec<String> = fs
        .calls()
        .iter()
        .map(|(_, p)| p.display().to_string())
        .collect();
    assert!(
        touched.iter().all(|p| !p.contains(r"C:\Users")),
        "{touched:?}"
    );
}

#[test]
fn scan_should_report_the_undetermined_reason_in_the_same_pattern() {
    let fs = populated_fs();
    let privileges = mismatched(MismatchReason::Undetermined);
    let results = completed(run_with(
        &privileges,
        ctx_for(&privileges),
        &fs,
        &bin_12_items(),
    ));
    let temp = category(&results, JunkCategory::TempUser);
    assert_eq!(temp.status, CategoryStatus::Blocked);
    assert_eq!(temp.block_reason, Some(MismatchReason::Undetermined));
    let dumps = category(&results, JunkCategory::CrashDumps);
    assert_eq!(dumps.status, CategoryStatus::Partial);
    assert_eq!(dumps.block_reason, Some(MismatchReason::Undetermined));
    assert_eq!(
        category(&results, JunkCategory::TempWindows).status,
        CategoryStatus::Done
    );
}

#[test]
fn scan_should_not_walk_user_locations_in_mismatch_even_with_an_unrestricted_context() {
    // Doble barrera: aunque el contexto de proteccion trajera al usuario, el alcance lo bloquea.
    let fs = populated_fs();
    let privileges = mismatched(MismatchReason::DifferentUser);
    let unrestricted = ProtectionContext::from_raw(raw_c_ana());
    let results = completed(run_with(&privileges, unrestricted, &fs, &bin_12_items()));
    assert_eq!(
        category(&results, JunkCategory::TempUser).status,
        CategoryStatus::Blocked
    );
    assert_eq!(category(&results, JunkCategory::CrashDumps).item_count, 3);
    assert!(
        fs.calls()
            .iter()
            .all(|(_, p)| !p.display().to_string().contains(r"C:\Users"))
    );
}

#[test]
fn scan_should_leave_every_file_category_unavailable_when_the_context_is_invalid() {
    let fs = populated_fs();
    let bin = bin_12_items();
    let privileges = matched();
    let broken = Err(ContextError::MissingRequired("windows_dir"));
    let results = completed(run_with(&privileges, broken, &fs, &bin));
    for wanted in JunkCategory::ALL {
        if wanted == JunkCategory::RecycleBin {
            continue;
        }
        let result = category(&results, wanted);
        assert_eq!(result.status, CategoryStatus::Unavailable, "{wanted:?}");
        assert_eq!((result.item_count, result.size_bytes), (0, 0), "{wanted:?}");
    }
    assert!(results.findings.is_empty());
    assert_eq!(fs.total_calls(), 0);
    let measured = category(&results, JunkCategory::RecycleBin);
    assert_eq!(measured.status, CategoryStatus::Done);
    assert_eq!(measured.item_count, 12);
    assert_eq!(bin.queried(), vec![PathBuf::from(r"C:\")]);
}

#[test]
fn scan_should_leave_temp_user_unavailable_when_the_user_has_no_temp_folder() {
    let mut raw = raw_c_ana();
    if let Some(user) = raw.user.as_mut() {
        user.temp = None;
    }
    let privileges = matched();
    let ctx = ProtectionContext::from_raw(raw);
    let results = completed(run_with(&privileges, ctx, &populated_fs(), &bin_12_items()));
    assert_eq!(
        category(&results, JunkCategory::TempUser).status,
        CategoryStatus::Unavailable
    );
    assert_eq!(
        category(&results, JunkCategory::TempWindows).status,
        CategoryStatus::Done
    );
}

#[test]
fn scan_should_count_skipped_files_without_adding_them_to_the_totals() {
    let fs = populated_fs()
        .with_file(r"C:\Users\ana\AppData\Local\Temp\h.dll", 5_000, old())
        .with_hard_links(r"C:\Users\ana\AppData\Local\Temp\h.dll", 2)
        .with_reparse_point(r"C:\Users\ana\AppData\Local\Temp\j");
    let privileges = matched();
    let results = completed(run_with(
        &privileges,
        ctx_for(&privileges),
        &fs,
        &bin_12_items(),
    ));
    let temp = category(&results, JunkCategory::TempUser);
    assert_eq!(
        (temp.item_count, temp.size_bytes, temp.skipped_count),
        (1, 2_000, 2)
    );
    let skipped: Vec<(&str, &str)> = findings_of(&results, JunkCategory::TempUser)
        .into_iter()
        .filter_map(|f| match &f.state {
            FindingState::Skipped(code) => Some((f.path.as_str(), code.as_str())),
            FindingState::Eligible(_) => None,
        })
        .collect();
    assert_eq!(
        skipped,
        vec![
            (
                r"C:\Users\ana\AppData\Local\Temp\h.dll",
                "multiple_hard_links"
            ),
            (r"C:\Users\ana\AppData\Local\Temp\j", "reparse_point"),
        ]
    );
    assert_eq!(results.total_bytes(), 78_000);
}

#[test]
fn scan_should_report_unlistable_folders_and_keep_scanning() {
    let fs = populated_fs().fail_on_op(
        r"C:\Windows\Temp",
        FsOp::ReadDir,
        PortError::PermissionDenied("sin elevar".into()),
    );
    let privileges = matched();
    let results = completed(run_with(
        &privileges,
        ctx_for(&privileges),
        &fs,
        &bin_12_items(),
    ));
    let temp = category(&results, JunkCategory::TempWindows);
    assert_eq!(temp.status, CategoryStatus::Done);
    assert_eq!((temp.item_count, temp.inaccessible_count), (0, 1));
    assert_eq!(category(&results, JunkCategory::WuCache).item_count, 1);
}

#[test]
fn scan_should_record_a_file_reachable_from_two_locations_only_once() {
    // Con TEMP redirigido a `C:\Windows\Temp` ambas categorias ven el mismo archivo; la
    // persistencia exige ruta unica por escaneo.
    let mut raw: RawProtectionContext = raw_c_ana();
    if let Some(user) = raw.user.as_mut() {
        user.temp = Some(r"C:\Windows\Temp".to_string());
    }
    let privileges = matched();
    let ctx = ProtectionContext::from_raw(raw);
    let results = completed(run_with(&privileges, ctx, &populated_fs(), &bin_12_items()));
    let mut paths: Vec<String> = results
        .findings
        .iter()
        .map(|f| f.path.to_lowercase())
        .collect();
    let before = paths.len();
    paths.sort();
    paths.dedup();
    assert_eq!(paths.len(), before);
    assert_eq!(category(&results, JunkCategory::TempUser).item_count, 1);
    assert_eq!(category(&results, JunkCategory::TempWindows).item_count, 0);
}

// ---- 3.3 solo lectura ----

#[test]
fn scan_should_only_read_and_never_pin_move_or_empty_anything() {
    let fs = Arc::new(populated_fs().with_reparse_point(r"C:\Windows\Temp\j"));
    let trash = FakeTrash::new(Arc::clone(&fs));
    let bin = bin_12_items();
    let privileges = matched();
    let _ = completed(run_with(
        &privileges,
        ctx_for(&privileges),
        fs.as_ref(),
        &bin,
    ));
    let calls = fs.calls();
    assert!(!calls.is_empty());
    assert!(
        calls
            .iter()
            .all(|(op, _)| matches!(op, FsOp::ReadDir | FsOp::Metadata | FsOp::Canonicalize)),
        "{calls:?}"
    );
    assert!(fs.pinned().is_empty());
    assert!(trash.moved().is_empty());
    assert_eq!(bin.queried().len(), 1, "solo se midio la Papelera");
}

// ---- 4.3 progreso y cancelacion ----

fn fs_with_many_windows_temp_files(count: usize) -> FakeFileSystem {
    let mut fs = FakeFileSystem::new()
        .canonical_as_is()
        .with_dir(r"C:\Windows")
        .with_dir(r"C:\Windows\Temp");
    for index in 0..count {
        fs = fs.with_file(&format!(r"C:\Windows\Temp\f{index:04}.tmp"), 10, old());
    }
    fs
}

#[test]
fn scan_should_announce_each_category_in_order_and_progress_every_256_files() {
    let fs = fs_with_many_windows_temp_files(600);
    let privileges = matched();
    let mut recorder = Recorder::default();
    let cancel = AtomicBool::new(false);
    let outcome = scan_junk(
        &privileges,
        ctx_for(&privileges),
        SYSTEM_DRIVE,
        &fs,
        &bin_12_items(),
        NOW,
        &cancel,
        &mut recorder,
    );
    assert_eq!(completed(outcome).total_items(), 600);

    let announced: Vec<JunkCategory> = recorder
        .events
        .iter()
        .filter_map(|e| match e {
            Event::Category(c) => Some(*c),
            Event::Progress { .. } => None,
        })
        .collect();
    assert_eq!(announced, JunkCategory::ALL.to_vec());
    let progress: Vec<&Event> = recorder
        .events
        .iter()
        .filter(|e| matches!(e, Event::Progress { .. }))
        .collect();
    assert_eq!(
        progress,
        vec![
            &Event::Progress {
                category: JunkCategory::TempWindows,
                files_seen: 256,
                eligible_bytes: 2_550
            },
            &Event::Progress {
                category: JunkCategory::TempWindows,
                files_seen: 512,
                eligible_bytes: 5_110
            },
        ]
    );
}

#[test]
fn scan_should_not_report_progress_below_the_threshold() {
    let fs = fs_with_many_windows_temp_files(255);
    let privileges = matched();
    let mut recorder = Recorder::default();
    let cancel = AtomicBool::new(false);
    let _ = scan_junk(
        &privileges,
        ctx_for(&privileges),
        SYSTEM_DRIVE,
        &fs,
        &bin_12_items(),
        NOW,
        &cancel,
        &mut recorder,
    );
    assert!(
        recorder
            .events
            .iter()
            .all(|e| matches!(e, Event::Category(_)))
    );
}

/// Observador que levanta la bandera de cancelacion al ver un evento dado.
struct CancelOn<'a> {
    flag: &'a AtomicBool,
    on_category: Option<JunkCategory>,
    on_progress: bool,
    announced: Vec<JunkCategory>,
}

impl ScanObserver for CancelOn<'_> {
    fn category(&mut self, category: JunkCategory) {
        self.announced.push(category);
        if self.on_category == Some(category) {
            self.flag.store(true, Ordering::Relaxed);
        }
    }
    fn progress(&mut self, _: JunkCategory, _: u64, _: u64) {
        if self.on_progress {
            self.flag.store(true, Ordering::Relaxed);
        }
    }
}

#[test]
fn scan_should_stop_before_touching_the_disk_when_cancelled_at_the_first_category() {
    let fs = populated_fs();
    let privileges = matched();
    let cancel = AtomicBool::new(false);
    let mut observer = CancelOn {
        flag: &cancel,
        on_category: Some(JunkCategory::TempUser),
        on_progress: false,
        announced: Vec::new(),
    };
    let bin = bin_12_items();
    let outcome = scan_junk(
        &privileges,
        ctx_for(&privileges),
        SYSTEM_DRIVE,
        &fs,
        &bin,
        NOW,
        &cancel,
        &mut observer,
    );
    assert_eq!(outcome, JunkScanOutcome::Cancelled);
    assert_eq!(observer.announced, vec![JunkCategory::TempUser]);
    assert_eq!(fs.total_calls(), 0);
    assert!(bin.queried().is_empty());
}

#[test]
fn scan_should_stop_midway_without_visiting_later_categories() {
    let fs = fs_with_many_windows_temp_files(600);
    let privileges = matched();
    let cancel = AtomicBool::new(false);
    let mut observer = CancelOn {
        flag: &cancel,
        on_category: None,
        on_progress: true,
        announced: Vec::new(),
    };
    let bin = bin_12_items();
    let outcome = scan_junk(
        &privileges,
        ctx_for(&privileges),
        SYSTEM_DRIVE,
        &fs,
        &bin,
        NOW,
        &cancel,
        &mut observer,
    );
    assert_eq!(outcome, JunkScanOutcome::Cancelled);
    assert_eq!(
        observer.announced,
        vec![JunkCategory::TempUser, JunkCategory::TempWindows]
    );
    assert!(bin.queried().is_empty());
}

#[test]
fn scan_should_return_cancelled_without_any_call_when_the_flag_is_already_set() {
    let fs = populated_fs();
    let privileges = matched();
    let cancel = AtomicBool::new(true);
    let mut recorder = Recorder::default();
    let bin = bin_12_items();
    let outcome = scan_junk(
        &privileges,
        ctx_for(&privileges),
        SYSTEM_DRIVE,
        &fs,
        &bin,
        NOW,
        &cancel,
        &mut recorder,
    );
    assert_eq!(outcome, JunkScanOutcome::Cancelled);
    assert!(recorder.events.is_empty());
    assert_eq!(fs.total_calls(), 0);
    assert!(bin.queried().is_empty());
}

// ---- unidad del sistema inyectada (sin letra fija en el dominio) ----

#[test]
fn scan_should_measure_the_injected_drive_even_when_the_protection_context_is_invalid() {
    let bin = bin_12_items();
    let privileges = matched();
    let broken = Err(ContextError::MissingRequired("program_data"));
    let results = completed(run_with_drive(
        &privileges,
        broken,
        Some('E'),
        &FakeFileSystem::new(),
        &bin,
    ));
    assert_eq!(bin.queried(), vec![PathBuf::from(r"E:\")]);
    assert_eq!(
        category(&results, JunkCategory::RecycleBin).status,
        CategoryStatus::Done
    );
}

#[test]
fn scan_should_report_the_recycle_bin_unavailable_without_guessing_a_drive() {
    let bin = bin_12_items();
    let privileges = matched();
    for protection in [
        Err(ContextError::MissingRequired("windows_dir")),
        ctx_for(&privileges),
    ] {
        let results = completed(run_with_drive(
            &privileges,
            protection,
            None,
            &populated_fs(),
            &bin,
        ));
        let measured = category(&results, JunkCategory::RecycleBin);
        assert_eq!(measured.status, CategoryStatus::Unavailable);
        assert_eq!((measured.item_count, measured.size_bytes), (0, 0));
    }
    assert!(
        bin.queried().is_empty(),
        "nunca se consulta una unidad adivinada"
    );
}

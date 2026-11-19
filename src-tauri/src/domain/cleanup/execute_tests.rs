//! Tests de `run_cleanup` con dobles: nada toca el disco ni la Papelera reales.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use super::*;
use crate::domain::cleanup::model::{
    AbortReason, CleanupCategory, CleanupStatus, CleanupSummary, PreviewId,
};
use crate::domain::junk::category::JunkCategory as C;
use crate::domain::model::{CleanupOutcome as O, EpochMillis, NewCleanupLogEntry, ScanId};
use crate::domain::ports::fakes::{FakeFileSystem, FakeTrash, FixedClock, FsOp, InMemoryStore};
use crate::domain::ports::{CleanupLogStore, FileIdentity, PortError, StoreError, TrashError};
use crate::domain::privileges::{MismatchReason, PrivilegeContext, UserMatch};
use crate::domain::protection::context::{ContextError, ProtectionContext};
use crate::domain::protection::test_support::{ctx_c_ana, ctx_c_no_user, ctx_c_user};

const NOW: EpochMillis = EpochMillis(1_800_000_000_000);
const HOUR: i64 = 3_600_000;
const TEMP: &str = r"C:\Users\ana\AppData\Local\Temp";
const SCAN: ScanId = ScanId(7);

fn id(n: u128) -> FileIdentity {
    FileIdentity {
        volume_serial: 1,
        file_id: n,
    }
}

fn matched() -> PrivilegeContext {
    PrivilegeContext::new(UserMatch::Match, false)
}

fn ctx() -> Result<ProtectionContext, ContextError> {
    Ok(ctx_c_ana())
}

fn ago(hours: i64) -> EpochMillis {
    EpochMillis(NOW.0 - hours * HOUR)
}

/// Un archivo del escenario: existe en el `FakeFileSystem` y figura en el preview.
#[derive(Clone)]
struct Spec {
    n: u128,
    category: C,
    path: String,
    size: u64,
    age_hours: i64,
}

fn spec(n: u128, category: C, path: &str, size: u64) -> Spec {
    Spec {
        n,
        category,
        path: path.to_string(),
        size,
        age_hours: 72,
    }
}

fn temp(n: u128, name: &str, size: u64) -> Spec {
    spec(n, C::TempUser, &format!(r"{TEMP}\{name}"), size)
}

/// Arbol de carpetas + archivos de `specs`, con la identidad y la fecha de cada uno.
fn fs_of(specs: &[Spec]) -> FakeFileSystem {
    let mut fs = FakeFileSystem::new().canonical_as_is();
    let mut dirs: Vec<PathBuf> = Vec::new();
    for spec in specs {
        let mut parents: Vec<PathBuf> = PathBuf::from(&spec.path)
            .ancestors()
            .skip(1)
            .filter(|a| a.parent().is_some())
            .map(PathBuf::from)
            .collect();
        parents.reverse();
        for dir in parents {
            if !dirs.contains(&dir) {
                dirs.push(dir);
            }
        }
    }
    dirs.sort_by_key(|dir| dir.components().count());
    for dir in dirs {
        fs = fs.with_dir(&dir.display().to_string());
    }
    for spec in specs {
        fs = fs
            .with_file(&spec.path, spec.size, Some(ago(spec.age_hours)))
            .with_identity(&spec.path, id(spec.n));
    }
    fs
}

fn item_of(spec: &Spec) -> CleanupItem {
    CleanupItem {
        finding_id: i64::try_from(spec.n).unwrap_or(0),
        category: spec.category.into(),
        path: spec.path.clone(),
        size_bytes: spec.size,
        modified_at: Some(ago(spec.age_hours)),
        identity: id(spec.n),
    }
}

fn preview_of(specs: &[Spec]) -> CleanupPreview {
    CleanupPreview::for_tests(
        PreviewId(1),
        SCAN,
        ago(1),
        specs.iter().map(item_of).collect(),
    )
}

/// Receptor que anota los avisos y puede levantar la bandera de cancelacion.
#[derive(Default)]
struct Recorder {
    started: Vec<u64>,
    progress: Vec<(u64, u64, u64, u64)>,
    processed_ids: Vec<i64>,
    cancel_at: Option<(u64, Arc<AtomicBool>)>,
}

impl CleanupObserver for Recorder {
    fn started(&mut self, total: u64) {
        self.started.push(total);
    }

    fn item_processed(&mut self, finding_id: i64) {
        self.processed_ids.push(finding_id);
    }

    fn progress(&mut self, processed: u64, total: u64, moved_count: u64, moved_bytes: u64) {
        self.progress
            .push((processed, total, moved_count, moved_bytes));
        if let Some((threshold, flag)) = &self.cancel_at
            && processed >= *threshold
        {
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "espeja la firma de run_cleanup con el reloj fijo"
)]
fn run_full(
    preview: CleanupPreview,
    privileges: &PrivilegeContext,
    protection: Result<ProtectionContext, ContextError>,
    fs: &FakeFileSystem,
    trash: &FakeTrash,
    log: &impl CleanupLogStore,
    cancel: &AtomicBool,
    observer: &mut Recorder,
) -> CleanupSummary {
    run_cleanup(
        preview,
        privileges,
        protection,
        fs,
        trash,
        log,
        &FixedClock(NOW),
        cancel,
        observer,
        &mut NoGuard,
        PROGRESS_EVERY_ITEMS,
    )
}

fn run(
    preview: CleanupPreview,
    fs: &FakeFileSystem,
    trash: &FakeTrash,
    log: &impl CleanupLogStore,
) -> CleanupSummary {
    run_full(
        preview,
        &matched(),
        ctx(),
        fs,
        trash,
        log,
        &AtomicBool::new(false),
        &mut Recorder::default(),
    )
}

/// Entradas del log en el orden en que se escribieron.
fn logged(log: &InMemoryStore) -> Vec<crate::domain::model::CleanupLogEntry> {
    let mut entries = log.recent_cleanup_log(1_000).unwrap();
    entries.reverse();
    entries
}

fn moved_paths(trash: &FakeTrash) -> Vec<String> {
    trash
        .moved()
        .iter()
        .map(|p| p.display().to_string())
        .collect()
}

// ---- 5.1 camino feliz ----

#[test]
fn run_cleanup_should_move_every_eligible_item_log_it_and_release_the_pins() {
    let specs = [
        temp(1, "a.tmp", 100),
        temp(2, "b.tmp", 200),
        temp(3, "c.tmp", 300),
    ];
    let fs = Arc::new(fs_of(&specs));
    let trash = FakeTrash::new(Arc::clone(&fs));
    let log = InMemoryStore::new();

    let summary = run(preview_of(&specs), &fs, &trash, &log);

    assert_eq!(summary.status, CleanupStatus::Completed);
    assert_eq!(summary.scan_id, SCAN);
    assert_eq!(summary.preview_id, PreviewId(1));
    assert_eq!(
        (
            summary.total_items,
            summary.processed,
            summary.not_processed
        ),
        (3, 3, 0)
    );
    assert_eq!((summary.moved_count, summary.moved_bytes), (3, 600));
    assert_eq!((summary.skipped_locked, summary.skipped_protected), (0, 0));
    assert_eq!((summary.skipped_cloud, summary.errors), (0, 0));
    assert!(summary.reasons.is_empty());
    assert_eq!(
        moved_paths(&trash),
        vec![
            format!(r"{TEMP}\a.tmp"),
            format!(r"{TEMP}\b.tmp"),
            format!(r"{TEMP}\c.tmp")
        ]
    );
    let entries = logged(&log);
    assert_eq!(entries.len(), 3);
    for (entry, spec) in entries.iter().zip(&specs) {
        assert_eq!(entry.outcome, O::MovedToTrash);
        assert_eq!(entry.error_detail, None);
        assert_eq!(entry.scan_id, Some(SCAN));
        assert_eq!(entry.category, "temp_user");
        assert_eq!(entry.path, spec.path);
        assert_eq!(entry.size_bytes, spec.size);
        assert_eq!(entry.ts, NOW);
    }
    assert!(
        fs.pinned().is_empty(),
        "las fijaciones se sueltan al terminar"
    );
}

#[test]
fn run_cleanup_should_pin_the_ancestors_before_each_move_and_keep_them_until_it_returns() {
    let specs = [temp(1, "a.tmp", 5), temp(2, "b.tmp", 6)];
    let fs = Arc::new(fs_of(&specs));
    let seen: Arc<Mutex<Vec<(usize, bool)>>> = Arc::new(Mutex::new(Vec::new()));
    let mut trash = FakeTrash::new(Arc::clone(&fs));
    for spec in &specs {
        let (hook_fs, hook_seen, path) = (Arc::clone(&fs), Arc::clone(&seen), spec.path.clone());
        trash = trash.before_move(&spec.path, move || {
            let pinned_before = hook_fs
                .calls()
                .iter()
                .any(|(op, p)| *op == FsOp::Pin && p == Path::new(&path));
            if let Ok(mut slot) = hook_seen.lock() {
                slot.push((hook_fs.pinned().len(), pinned_before));
            }
        });
    }
    let log = InMemoryStore::new();

    run(preview_of(&specs), &fs, &trash, &log);

    let observed = seen.lock().unwrap().clone();
    // Durante cada movimiento hay ancestros fijados y el Pin ya se habia pedido.
    assert_eq!(observed.len(), 2);
    assert!(observed.iter().all(|(pinned, asked)| *pinned > 0 && *asked));
    assert!(fs.pinned().is_empty());
}

#[test]
fn run_cleanup_should_hand_the_real_path_and_not_the_8_3_alias_to_the_trash() {
    let short = r"C:\Users\ANAMAR~1\AppData\Local\Temp\a.tmp";
    let long = r"C:\Users\ana maria\AppData\Local\Temp\a.tmp";
    let short_spec = spec(1, C::TempUser, short, 50);
    // El archivo existe con su nombre largo (lo que mueve la Papelera) y con el alias (lo que
    // resuelve la verificacion).
    let fs = Arc::new(
        fs_of(std::slice::from_ref(&short_spec))
            .with_dir(r"C:\Users\ana maria")
            .with_dir(r"C:\Users\ana maria\AppData")
            .with_dir(r"C:\Users\ana maria\AppData\Local")
            .with_dir(TEMP.replace("ana", "ana maria").as_str())
            .with_file(long, 50, Some(ago(72)))
            .with_canonical(short, long),
    );
    let trash = FakeTrash::new(Arc::clone(&fs));
    let log = InMemoryStore::new();

    let summary = run_full(
        preview_of(&[short_spec]),
        &matched(),
        Ok(ctx_c_user("ana maria")),
        &fs,
        &trash,
        &log,
        &AtomicBool::new(false),
        &mut Recorder::default(),
    );

    assert_eq!(summary.moved_count, 1);
    assert_eq!(moved_paths(&trash), vec![long.to_string()]);
    assert_eq!(logged(&log)[0].path, long, "el log lleva la ruta real");
}

#[test]
fn run_cleanup_should_aggregate_the_summary_by_category() {
    let specs = [
        temp(1, "a.tmp", 10),
        spec(2, C::TempWindows, r"C:\Windows\Temp\w.tmp", 40),
        temp(3, "b.tmp", 20),
    ];
    let fs = Arc::new(fs_of(&specs));
    let trash = FakeTrash::new(Arc::clone(&fs));
    let summary = run(preview_of(&specs), &fs, &trash, &InMemoryStore::new());

    assert_eq!(summary.moved_count, 3);
    let by_category: Vec<(C, u64, u64)> = summary
        .categories
        .iter()
        .filter_map(|c| Some((c.category.junk()?, c.moved_count, c.moved_bytes)))
        .collect();
    assert_eq!(
        by_category,
        vec![(C::TempUser, 2, 30), (C::TempWindows, 1, 40)]
    );
    assert!(
        summary
            .categories
            .iter()
            .all(|c| c.skipped_count == 0 && c.error_count == 0)
    );
}

#[test]
fn run_cleanup_should_do_nothing_and_report_completed_for_an_empty_preview() {
    let fs = Arc::new(FakeFileSystem::new());
    let trash = FakeTrash::new(Arc::clone(&fs));
    let log = InMemoryStore::new();
    let mut observer = Recorder::default();
    let summary = run_full(
        preview_of(&[]),
        &matched(),
        ctx(),
        &fs,
        &trash,
        &log,
        &AtomicBool::new(false),
        &mut observer,
    );
    assert_eq!(summary.status, CleanupStatus::Completed);
    assert_eq!((summary.total_items, summary.processed), (0, 0));
    assert_eq!(fs.total_calls(), 0);
    assert!(logged(&log).is_empty());
    assert_eq!(observer.started, vec![0]);
}

// ---- 5.2 TOCTOU y proteccion (una fila por scenario de "Re-verificacion de cada item") ----

const CONTROL: &str = r"C:\Windows\Temp\ok.tmp";

/// Corre un preview de `a.tmp` (el item a vulnerar) y `ok.tmp` (control, otra carpeta) y verifica
/// que el primero se omite con `outcome`/`detail`, que no se mueve y que el control si.
fn assert_first_item_skipped(
    prepare: impl FnOnce(FakeFileSystem) -> FakeFileSystem,
    race: impl FnOnce(&FakeFileSystem),
    outcome: O,
    detail: &str,
) {
    let specs = [temp(1, "a.tmp", 10), spec(2, C::TempWindows, CONTROL, 20)];
    let fs = Arc::new(prepare(fs_of(&specs)));
    race(&fs);
    let trash = FakeTrash::new(Arc::clone(&fs));
    let log = InMemoryStore::new();

    let summary = run(preview_of(&specs), &fs, &trash, &log);

    let entries = logged(&log);
    assert_eq!(entries.len(), 2, "el resto continua");
    assert_eq!(
        (entries[0].outcome, entries[0].error_detail.as_deref()),
        (outcome, Some(detail))
    );
    assert_eq!(entries[1].outcome, O::MovedToTrash);
    assert_eq!(moved_paths(&trash), vec![CONTROL.to_string()]);
    assert_eq!((summary.processed, summary.moved_count), (2, 1));
    assert_eq!(summary.reasons.len(), 1);
    let reason = &summary.reasons[0];
    assert_eq!(
        (reason.outcome, reason.detail.as_str(), reason.count),
        (outcome, detail, 1)
    );
}

fn a_tmp() -> PathBuf {
    PathBuf::from(format!(r"{TEMP}\a.tmp"))
}

#[test]
fn run_cleanup_should_skip_a_file_replaced_by_another_one_with_the_same_name() {
    assert_first_item_skipped(
        |fs| fs,
        |fs| fs.recreate_file(&a_tmp(), id(999)),
        O::SkippedProtected,
        "identity_changed",
    );
}

#[test]
fn run_cleanup_should_skip_a_file_replaced_by_a_symlink_and_leave_its_target_alone() {
    let target = r"C:\Windows\System32\x.dll";
    let specs = [temp(1, "a.tmp", 10), spec(2, C::TempWindows, CONTROL, 20)];
    let fs = Arc::new(
        fs_of(&specs)
            .with_dir(r"C:\Windows\System32")
            .with_file(target, 4, None),
    );
    fs.replace_with_reparse_point(&a_tmp());
    let trash = FakeTrash::new(Arc::clone(&fs));
    let log = InMemoryStore::new();

    run(preview_of(&specs), &fs, &trash, &log);

    let entries = logged(&log);
    assert_eq!(entries[0].outcome, O::SkippedProtected);
    assert_eq!(entries[0].error_detail.as_deref(), Some("reparse_point"));
    // Ni el enlace ni su destino se movieron.
    assert!(fs.exists(&a_tmp()));
    assert!(fs.exists(Path::new(target)));
    assert_eq!(moved_paths(&trash), vec![CONTROL.to_string()]);
}

#[test]
fn run_cleanup_should_skip_a_file_whose_parent_became_a_junction() {
    let specs = [temp(1, "a.tmp", 10), spec(2, C::TempWindows, CONTROL, 20)];
    let fs = Arc::new(fs_of(&specs).with_dir(r"C:\Windows\System32"));
    fs.replace_with_reparse_point(Path::new(TEMP));
    let trash = FakeTrash::new(Arc::clone(&fs));
    let log = InMemoryStore::new();

    run(preview_of(&specs), &fs, &trash, &log);

    let entries = logged(&log);
    assert_eq!(entries[0].outcome, O::SkippedProtected);
    assert_eq!(
        entries[0].error_detail.as_deref(),
        Some("unresolvable_pin_failed")
    );
    // Nada bajo el destino de la junction se toco ni se movio.
    assert!(
        fs.calls()
            .iter()
            .all(|(_, p)| !p.starts_with(r"C:\Windows\System32"))
    );
    assert_eq!(moved_paths(&trash), vec![CONTROL.to_string()]);
}

#[test]
fn run_cleanup_should_skip_a_file_that_got_a_second_hard_link_after_the_scan() {
    assert_first_item_skipped(
        |fs| {
            fs.with_hard_link(
                r"C:\Users\ana\Documents\copy.tmp",
                &format!(r"{TEMP}\a.tmp"),
            )
        },
        |_| {},
        O::SkippedProtected,
        "multiple_hard_links",
    );
}

#[test]
fn run_cleanup_should_skip_a_file_that_became_a_cloud_placeholder() {
    assert_first_item_skipped(
        |fs| fs.with_cloud_placeholder(&format!(r"{TEMP}\a.tmp"), 10),
        |_| {},
        O::SkippedCloud,
        "skipped_cloud",
    );
}

#[test]
fn run_cleanup_should_skip_a_file_that_no_longer_exists_and_continue() {
    assert_first_item_skipped(
        |fs| fs,
        |fs| fs.remove(&a_tmp()),
        O::SkippedProtected,
        "unresolvable_not_found",
    );
}

#[test]
fn run_cleanup_should_skip_as_locked_a_file_whose_metadata_reports_in_use() {
    assert_first_item_skipped(
        |fs| {
            fs.fail_on_op(
                &format!(r"{TEMP}\a.tmp"),
                FsOp::Metadata,
                PortError::InUse("a".into()),
            )
        },
        |_| {},
        O::SkippedLocked,
        "unresolvable_in_use",
    );
}

#[test]
fn run_cleanup_should_skip_a_persisted_path_that_does_not_normalize() {
    let bad = CleanupItem {
        path: "a.tmp".to_string(),
        ..item_of(&temp(1, "a.tmp", 10))
    };
    let ok_spec = spec(2, C::TempWindows, CONTROL, 20);
    let fs = Arc::new(fs_of(std::slice::from_ref(&ok_spec)));
    let trash = FakeTrash::new(Arc::clone(&fs));
    let log = InMemoryStore::new();
    let preview =
        CleanupPreview::for_tests(PreviewId(1), SCAN, ago(1), vec![bad, item_of(&ok_spec)]);

    let summary = run(preview, &fs, &trash, &log);

    let entries = logged(&log);
    assert_eq!(entries[0].outcome, O::SkippedProtected);
    assert_eq!(entries[0].error_detail.as_deref(), Some("not_absolute"));
    assert_eq!(
        entries[0].path, "a.tmp",
        "sin verificar se registra la ruta persistida"
    );
    assert_eq!(summary.moved_count, 1);
}

// ---- 5.3 privilegios re-evaluados al confirmar ----

fn mismatched(reason: MismatchReason) -> PrivilegeContext {
    PrivilegeContext::new(UserMatch::Mismatch(reason), false)
}

#[test]
fn run_cleanup_should_block_per_user_items_when_the_confirm_context_is_a_mismatch() {
    for (reason, detail) in [
        (MismatchReason::Undetermined, "blocked_undetermined"),
        (MismatchReason::DifferentUser, "blocked_different_user"),
    ] {
        // El preview se armo en `match`; al confirmar la sesion ya no es la misma.
        let specs = [
            temp(1, "a.tmp", 10),
            temp(2, "b.tmp", 20),
            spec(3, C::TempWindows, CONTROL, 30),
        ];
        let fs = Arc::new(fs_of(&specs));
        let trash = FakeTrash::new(Arc::clone(&fs));
        let log = InMemoryStore::new();

        let summary = run_full(
            preview_of(&specs),
            &mismatched(reason),
            Ok(ctx_c_no_user()),
            &fs,
            &trash,
            &log,
            &AtomicBool::new(false),
            &mut Recorder::default(),
        );

        let entries = logged(&log);
        assert_eq!(entries[0].outcome, O::SkippedProtected);
        assert_eq!(entries[0].error_detail.as_deref(), Some(detail));
        assert_eq!(entries[1].error_detail.as_deref(), Some(detail));
        assert_eq!(entries[2].outcome, O::MovedToTrash);
        assert_eq!(moved_paths(&trash), vec![CONTROL.to_string()]);
        assert_eq!((summary.skipped_protected, summary.moved_count), (2, 1));
        // Ninguna ruta del perfil se abre ni se fija.
        assert!(
            fs.calls()
                .iter()
                .all(|(_, p)| !p.starts_with(r"C:\Users\ana")),
            "{:?}",
            fs.calls()
        );
    }
}

#[test]
fn run_cleanup_should_protect_a_profile_crash_dump_on_mismatch_and_keep_machine_dumps() {
    let profile_dump = spec(
        1,
        C::CrashDumps,
        r"C:\Users\ana\AppData\Local\CrashDumps\a.dmp",
        10,
    );
    let machine_dump = spec(2, C::CrashDumps, r"C:\Windows\Minidump\b.dmp", 20);
    let specs = [profile_dump, machine_dump];
    let fs = Arc::new(fs_of(&specs));
    let trash = FakeTrash::new(Arc::clone(&fs));
    let log = InMemoryStore::new();

    run_full(
        preview_of(&specs),
        &mismatched(MismatchReason::DifferentUser),
        Ok(ctx_c_no_user()),
        &fs,
        &trash,
        &log,
        &AtomicBool::new(false),
        &mut Recorder::default(),
    );

    let entries = logged(&log);
    assert_eq!(entries[0].outcome, O::SkippedProtected);
    assert_eq!(
        entries[0].error_detail.as_deref(),
        Some("other_user_profile")
    );
    assert_eq!(entries[1].outcome, O::MovedToTrash);
    assert_eq!(
        moved_paths(&trash),
        vec![r"C:\Windows\Minidump\b.dmp".to_string()]
    );
}

#[test]
fn run_cleanup_should_skip_everything_as_context_unavailable_without_touching_the_disk() {
    let specs = [temp(1, "a.tmp", 10), spec(2, C::TempWindows, CONTROL, 20)];
    let fs = Arc::new(fs_of(&specs));
    let trash = FakeTrash::new(Arc::clone(&fs));
    let log = InMemoryStore::new();
    let broken = Err(ContextError::MissingRequired("windows_dir"));

    let summary = run_full(
        preview_of(&specs),
        &matched(),
        broken,
        &fs,
        &trash,
        &log,
        &AtomicBool::new(false),
        &mut Recorder::default(),
    );

    let entries = logged(&log);
    assert_eq!(entries.len(), 2);
    assert!(entries.iter().all(|e| {
        e.outcome == O::SkippedProtected && e.error_detail.as_deref() == Some("context_unavailable")
    }));
    assert_eq!(fs.total_calls(), 0);
    assert!(trash.moved().is_empty());
    assert_eq!((summary.skipped_protected, summary.moved_count), (2, 0));
}

// ---- 5.4 cambios desde el escaneo y guarda reciente por handle ----

/// El preview ve `scanned`; el disco tiene `on_disk` (mismo `n`, otra cosa). Devuelve el log.
fn run_changed(
    scanned: &Spec,
    on_disk: &Spec,
) -> (
    CleanupSummary,
    Vec<crate::domain::model::CleanupLogEntry>,
    Vec<String>,
) {
    let fs = Arc::new(fs_of(std::slice::from_ref(on_disk)));
    let trash = FakeTrash::new(Arc::clone(&fs));
    let log = InMemoryStore::new();
    let summary = run(
        CleanupPreview::for_tests(PreviewId(1), SCAN, ago(1), vec![item_of(scanned)]),
        &fs,
        &trash,
        &log,
    );
    (summary, logged(&log), moved_paths(&trash))
}

#[test]
fn run_cleanup_should_skip_a_file_whose_size_changed_since_the_scan() {
    let scanned = temp(1, "a.tmp", 10);
    let grown = Spec {
        size: 11,
        ..scanned.clone()
    };
    let (summary, entries, moved) = run_changed(&scanned, &grown);
    assert_eq!(entries[0].outcome, O::SkippedLocked);
    assert_eq!(
        entries[0].error_detail.as_deref(),
        Some("changed_since_scan")
    );
    assert_eq!(
        entries[0].size_bytes, 11,
        "se registra el tamano por handle"
    );
    assert!(moved.is_empty());
    assert_eq!(summary.skipped_locked, 1);
}

#[test]
fn run_cleanup_should_skip_a_file_whose_date_by_handle_is_newer_than_the_persisted_one() {
    // Cache de un navegador abierto: NTFS dejo vieja la fecha del listado que se persistio.
    let scanned = temp(1, "a.tmp", 10);
    let newer = Spec {
        age_hours: 5,
        ..scanned.clone()
    };
    let (_, entries, moved) = run_changed(&scanned, &newer);
    assert_eq!(entries[0].outcome, O::SkippedLocked);
    assert_eq!(
        entries[0].error_detail.as_deref(),
        Some("changed_since_scan")
    );
    assert!(moved.is_empty());
}

#[test]
fn run_cleanup_should_skip_a_file_written_two_hours_ago_with_the_scanned_size_and_date() {
    let recent = Spec {
        age_hours: 2,
        ..temp(1, "a.tmp", 10)
    };
    let (summary, entries, moved) = run_changed(&recent, &recent);
    assert_eq!(entries[0].outcome, O::SkippedLocked);
    assert_eq!(
        entries[0].error_detail.as_deref(),
        Some("recently_modified")
    );
    assert!(moved.is_empty());
    assert_eq!(summary.reasons[0].detail, "recently_modified");
}

#[test]
fn run_cleanup_should_skip_a_file_without_date_as_recently_modified() {
    let path = format!(r"{TEMP}\a.tmp");
    let fs = Arc::new(
        FakeFileSystem::new()
            .canonical_as_is()
            .with_dir(r"C:\Users")
            .with_dir(r"C:\Users\ana")
            .with_dir(r"C:\Users\ana\AppData")
            .with_dir(r"C:\Users\ana\AppData\Local")
            .with_dir(TEMP)
            .with_file(&path, 10, None)
            .with_identity(&path, id(1)),
    );
    let trash = FakeTrash::new(Arc::clone(&fs));
    let log = InMemoryStore::new();
    let undated = CleanupItem {
        modified_at: None,
        ..item_of(&temp(1, "a.tmp", 10))
    };
    run(
        CleanupPreview::for_tests(PreviewId(1), SCAN, ago(1), vec![undated]),
        &fs,
        &trash,
        &log,
    );
    let entries = logged(&log);
    assert_eq!(entries[0].outcome, O::SkippedLocked);
    assert_eq!(
        entries[0].error_detail.as_deref(),
        Some("recently_modified")
    );
    assert!(trash.moved().is_empty());
}

#[test]
fn run_cleanup_should_move_a_file_written_exactly_24_hours_before_now() {
    let edge = Spec {
        age_hours: 24,
        ..temp(1, "a.tmp", 10)
    };
    let (summary, entries, moved) = run_changed(&edge, &edge);
    assert_eq!(entries[0].outcome, O::MovedToTrash);
    assert_eq!(moved, vec![edge.path.clone()]);
    assert_eq!(summary.moved_count, 1);
}

// ---- 5.5 errores de la Papelera, post-verificacion y abortos ----

fn three_temp() -> [Spec; 3] {
    [
        temp(1, "a.tmp", 10),
        temp(2, "b.tmp", 20),
        temp(3, "c.tmp", 30),
    ]
}

fn path_of(spec: &Spec) -> String {
    spec.path.clone()
}

#[test]
fn run_cleanup_should_map_each_continuable_trash_error_and_keep_going() {
    let table = [
        (TrashError::InUse, O::SkippedLocked, "in_use"),
        (TrashError::TooLarge, O::Error, "recycle_bin_too_small"),
        (TrashError::PathTooLong, O::Error, "path_too_long"),
        (TrashError::PermissionDenied, O::Error, "permission_denied"),
        (TrashError::NotFound, O::Error, "not_found"),
        (
            TrashError::Other("io_0x80270021".into()),
            O::Error,
            "io_0x80270021",
        ),
    ];
    for (error, outcome, code) in table {
        let specs = three_temp();
        let fs = Arc::new(fs_of(&specs));
        let trash = FakeTrash::new(Arc::clone(&fs)).fail_on(&specs[1].path, error.clone());
        let log = InMemoryStore::new();

        let summary = run(preview_of(&specs), &fs, &trash, &log);

        let entries = logged(&log);
        assert_eq!(entries.len(), 3, "{error:?}");
        assert_eq!(
            (entries[1].outcome, entries[1].error_detail.as_deref()),
            (outcome, Some(code)),
            "{error:?}"
        );
        assert_eq!(
            moved_paths(&trash),
            vec![path_of(&specs[0]), path_of(&specs[2])],
            "{error:?}"
        );
        assert!(fs.exists(Path::new(&specs[1].path)), "sigue en su lugar");
        assert_eq!(summary.status, CleanupStatus::Completed);
        assert_eq!(summary.moved_count, 2);
    }
}

#[test]
fn run_cleanup_should_abort_at_the_first_unavailable_recycle_bin() {
    let specs = three_temp();
    let fs = Arc::new(fs_of(&specs));
    let trash = FakeTrash::new(Arc::clone(&fs)).fail_on(&specs[0].path, TrashError::BinUnavailable);
    let log = InMemoryStore::new();

    let summary = run(preview_of(&specs), &fs, &trash, &log);

    assert_eq!(
        summary.status,
        CleanupStatus::Aborted(AbortReason::RecycleBinUnavailable)
    );
    let entries = logged(&log);
    assert_eq!(entries.len(), 1, "los no procesados no se registran");
    assert_eq!(entries[0].outcome, O::Error);
    assert_eq!(
        entries[0].error_detail.as_deref(),
        Some("recycle_bin_unavailable")
    );
    assert!(trash.moved().is_empty());
    assert!(fs.exists(Path::new(&specs[0].path)));
    assert_eq!(
        (summary.processed, summary.not_processed, summary.errors),
        (1, 2, 1)
    );
    // Ni siquiera se fijaron los ancestros de los demas.
    assert_eq!(fs.call_count(FsOp::Pin, &specs[1].path), 0);
    assert_eq!(fs.call_count(FsOp::Pin, &specs[2].path), 0);
}

#[test]
fn run_cleanup_should_abort_before_any_move_when_the_preflight_says_the_bin_is_unavailable() {
    let specs = three_temp();
    let fs = Arc::new(fs_of(&specs));
    let trash = FakeTrash::new(Arc::clone(&fs)).preflight_fails(TrashError::BinUnavailable);
    let log = InMemoryStore::new();

    let summary = run(preview_of(&specs), &fs, &trash, &log);

    assert_eq!(
        summary.status,
        CleanupStatus::Aborted(AbortReason::RecycleBinUnavailable)
    );
    assert!(logged(&log).is_empty(), "nada se registra como movido");
    assert!(trash.moved().is_empty());
    for spec in &specs {
        assert!(fs.exists(Path::new(&spec.path)));
        assert_eq!(fs.call_count(FsOp::Pin, &spec.path), 0);
    }
    assert_eq!(
        (
            summary.processed,
            summary.not_processed,
            summary.moved_count
        ),
        (0, 3, 0)
    );
}

#[test]
fn run_cleanup_should_abort_before_any_move_when_the_preflight_says_the_backend_is_unusable() {
    let specs = three_temp();
    let fs = Arc::new(fs_of(&specs));
    let trash = FakeTrash::new(Arc::clone(&fs)).preflight_fails(TrashError::BackendUnavailable);
    let log = InMemoryStore::new();

    let summary = run(preview_of(&specs), &fs, &trash, &log);

    assert_eq!(
        summary.status,
        CleanupStatus::Aborted(AbortReason::TrashUnavailable)
    );
    assert!(logged(&log).is_empty());
    assert!(trash.moved().is_empty());
}

#[test]
fn run_cleanup_should_treat_any_other_preflight_failure_as_an_unavailable_recycle_bin() {
    let specs = three_temp();
    let fs = Arc::new(fs_of(&specs));
    let trash = FakeTrash::new(Arc::clone(&fs)).preflight_fails(TrashError::Other("io_0x5".into()));
    let log = InMemoryStore::new();

    let summary = run(preview_of(&specs), &fs, &trash, &log);

    assert_eq!(
        summary.status,
        CleanupStatus::Aborted(AbortReason::RecycleBinUnavailable)
    );
    assert!(trash.moved().is_empty());
}

#[test]
fn run_cleanup_should_run_the_preflight_exactly_once_on_the_first_item_path() {
    let specs = three_temp();
    let fs = Arc::new(fs_of(&specs));
    let trash = FakeTrash::new(Arc::clone(&fs));
    let log = InMemoryStore::new();

    let summary = run(preview_of(&specs), &fs, &trash, &log);

    assert_eq!(summary.status, CleanupStatus::Completed);
    assert_eq!(trash.preflight_calls(), vec![PathBuf::from(&specs[0].path)]);
    assert_eq!(trash.moved().len(), 3);
}

#[test]
fn run_cleanup_should_abort_when_the_trash_backend_is_unusable() {
    let specs = three_temp();
    let fs = Arc::new(fs_of(&specs));
    let trash =
        FakeTrash::new(Arc::clone(&fs)).fail_on(&specs[1].path, TrashError::BackendUnavailable);
    let log = InMemoryStore::new();

    let summary = run(preview_of(&specs), &fs, &trash, &log);

    assert_eq!(
        summary.status,
        CleanupStatus::Aborted(AbortReason::TrashUnavailable)
    );
    let entries = logged(&log);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].outcome, O::MovedToTrash);
    assert_eq!(
        entries[1].error_detail.as_deref(),
        Some("trash_unavailable")
    );
    assert_eq!(moved_paths(&trash), vec![path_of(&specs[0])]);
    assert_eq!((summary.processed, summary.not_processed), (2, 1));
}

#[test]
fn run_cleanup_should_report_an_error_when_the_trash_says_ok_but_the_file_is_still_there() {
    let specs = three_temp();
    let fs = Arc::new(fs_of(&specs));
    let trash = FakeTrash::new(Arc::clone(&fs)).succeed_without_moving(&specs[1].path);
    let log = InMemoryStore::new();

    let summary = run(preview_of(&specs), &fs, &trash, &log);

    let entries = logged(&log);
    assert_eq!(entries[1].outcome, O::Error);
    assert_eq!(entries[1].error_detail.as_deref(), Some("not_moved"));
    assert_eq!(summary.moved_count, 2);
    assert_eq!(summary.status, CleanupStatus::Completed);
}

#[test]
fn run_cleanup_should_count_as_moved_when_another_file_now_occupies_the_path() {
    // La Papelera respondio exito y en la ruta hay OTRO archivo (otra identidad): se movio.
    let specs = [temp(1, "a.tmp", 10)];
    let fs = Arc::new(fs_of(&specs));
    let (hook_fs, hook_path) = (Arc::clone(&fs), PathBuf::from(&specs[0].path));
    let trash = FakeTrash::new(Arc::clone(&fs))
        .succeed_without_moving(&specs[0].path)
        .before_move(&specs[0].path, move || {
            hook_fs.recreate_file(&hook_path, id(999));
        });
    let log = InMemoryStore::new();

    let summary = run(preview_of(&specs), &fs, &trash, &log);

    assert_eq!(logged(&log)[0].outcome, O::MovedToTrash);
    assert_eq!(summary.moved_count, 1);
}

/// Store cuyo `append_cleanup_log` falla en la llamada `fail_on_call` (desde 1).
struct FailingLog {
    inner: InMemoryStore,
    fail_on_call: usize,
    calls: std::cell::Cell<usize>,
}

impl FailingLog {
    fn new(fail_on_call: usize) -> Self {
        Self {
            inner: InMemoryStore::new(),
            fail_on_call,
            calls: std::cell::Cell::new(0),
        }
    }
}

impl CleanupLogStore for FailingLog {
    fn append_cleanup_log(&self, entries: &[NewCleanupLogEntry]) -> Result<(), StoreError> {
        self.calls.set(self.calls.get() + 1);
        if self.calls.get() == self.fail_on_call {
            return Err(StoreError::Unavailable("disk full".into()));
        }
        self.inner.append_cleanup_log(entries)
    }

    fn recent_cleanup_log(
        &self,
        limit: u32,
    ) -> Result<Vec<crate::domain::model::CleanupLogEntry>, StoreError> {
        self.inner.recent_cleanup_log(limit)
    }

    fn scan_cleaned_at(
        &self,
        scan_id: ScanId,
        since: EpochMillis,
    ) -> Result<Option<EpochMillis>, StoreError> {
        self.inner.scan_cleaned_at(scan_id, since)
    }
}

#[test]
fn run_cleanup_should_abort_without_moving_more_when_the_log_cannot_be_written() {
    let specs = three_temp();
    let fs = Arc::new(fs_of(&specs));
    let trash = FakeTrash::new(Arc::clone(&fs));
    let log = FailingLog::new(2);

    let summary = run(preview_of(&specs), &fs, &trash, &log);

    assert_eq!(
        summary.status,
        CleanupStatus::Aborted(AbortReason::LogUnavailable)
    );
    // El 1.o quedo registrado; el 2.o se movio pero no pudo registrarse; el 3.o no se intento.
    assert_eq!(logged_of(&log).len(), 1);
    assert_eq!(
        moved_paths(&trash),
        vec![path_of(&specs[0]), path_of(&specs[1])]
    );
    assert!(fs.exists(Path::new(&specs[2].path)));
    assert_eq!(fs.call_count(FsOp::Pin, &specs[2].path), 0);
    assert_eq!((summary.processed, summary.not_processed), (2, 1));
    assert_eq!(
        summary.moved_count, 2,
        "se informa lo que realmente se movio"
    );
}

#[test]
fn run_cleanup_should_abort_on_the_very_first_log_failure() {
    let specs = three_temp();
    let fs = Arc::new(fs_of(&specs));
    let trash = FakeTrash::new(Arc::clone(&fs));
    let log = FailingLog::new(1);

    let summary = run(preview_of(&specs), &fs, &trash, &log);

    assert_eq!(
        summary.status,
        CleanupStatus::Aborted(AbortReason::LogUnavailable)
    );
    assert_eq!(moved_paths(&trash), vec![path_of(&specs[0])]);
    assert!(logged_of(&log).is_empty());
}

fn logged_of(log: &FailingLog) -> Vec<crate::domain::model::CleanupLogEntry> {
    logged(&log.inner)
}

// ---- 5.6 progreso y cancelacion ----

fn many_temp(count: u128) -> Vec<Spec> {
    (1..=count)
        .map(|n| temp(n, &format!("f{n:03}.tmp"), n as u64))
        .collect()
}

#[test]
fn run_cleanup_should_report_started_and_progress_every_32_items_and_at_the_end() {
    let specs = many_temp(100);
    let fs = Arc::new(fs_of(&specs));
    let trash = FakeTrash::new(Arc::clone(&fs));
    let mut observer = Recorder::default();

    let summary = run_full(
        preview_of(&specs),
        &matched(),
        ctx(),
        &fs,
        &trash,
        &InMemoryStore::new(),
        &AtomicBool::new(false),
        &mut observer,
    );

    assert_eq!(observer.started, vec![100]);
    let processed: Vec<u64> = observer.progress.iter().map(|p| p.0).collect();
    assert_eq!(processed, vec![32, 64, 96, 100]);
    assert!(observer.progress.iter().all(|p| p.1 == 100));
    // Los bytes movidos acumulados: 1 + 2 + ... + 32 = 528 al primer aviso.
    assert_eq!((observer.progress[0].2, observer.progress[0].3), (32, 528));
    let last = observer.progress[3];
    assert_eq!((last.2, last.3), (100, summary.moved_bytes));
    assert_eq!(summary.moved_count, 100);
}

#[test]
fn run_cleanup_should_not_repeat_the_final_progress_when_the_total_is_a_multiple_of_32() {
    let specs = many_temp(64);
    let fs = Arc::new(fs_of(&specs));
    let trash = FakeTrash::new(Arc::clone(&fs));
    let mut observer = Recorder::default();
    run_full(
        preview_of(&specs),
        &matched(),
        ctx(),
        &fs,
        &trash,
        &InMemoryStore::new(),
        &AtomicBool::new(false),
        &mut observer,
    );
    let processed: Vec<u64> = observer.progress.iter().map(|p| p.0).collect();
    assert_eq!(processed, vec![32, 64]);
}

#[test]
fn run_cleanup_should_stop_before_the_next_item_when_the_observer_raises_the_flag() {
    let specs = many_temp(100);
    let fs = Arc::new(fs_of(&specs));
    let trash = FakeTrash::new(Arc::clone(&fs));
    let log = InMemoryStore::new();
    let cancel = Arc::new(AtomicBool::new(false));
    let mut observer = Recorder {
        cancel_at: Some((32, Arc::clone(&cancel))),
        ..Recorder::default()
    };

    let summary = run_full(
        preview_of(&specs),
        &matched(),
        ctx(),
        &fs,
        &trash,
        &log,
        &cancel,
        &mut observer,
    );

    assert_eq!(summary.status, CleanupStatus::Cancelled);
    assert_eq!(logged(&log).len(), 32);
    assert_eq!(
        (
            summary.processed,
            summary.not_processed,
            summary.moved_count
        ),
        (32, 68, 32)
    );
    // Los 68 restantes siguen en su lugar.
    assert!(specs[32..].iter().all(|s| fs.exists(Path::new(&s.path))));
    assert_eq!(trash.moved().len(), 32);
    // El ciclo cortado igual informa el avance final.
    assert_eq!(observer.progress.last().map(|p| p.0), Some(32));
}

#[test]
fn run_cleanup_should_finish_and_log_the_item_in_progress_when_cancelled_during_it() {
    let specs = many_temp(100);
    let fs = Arc::new(fs_of(&specs));
    let cancel = Arc::new(AtomicBool::new(false));
    let hook_flag = Arc::clone(&cancel);
    // Se cancela mientras se mueve el item 41.
    let trash = FakeTrash::new(Arc::clone(&fs)).before_move(&specs[40].path, move || {
        hook_flag.store(true, std::sync::atomic::Ordering::SeqCst);
    });
    let log = InMemoryStore::new();

    let summary = run_full(
        preview_of(&specs),
        &matched(),
        ctx(),
        &fs,
        &trash,
        &log,
        &cancel,
        &mut Recorder::default(),
    );

    assert_eq!(summary.status, CleanupStatus::Cancelled);
    assert_eq!(
        logged(&log).len(),
        41,
        "el item en curso termina y se registra"
    );
    assert_eq!(summary.not_processed, 59);
    assert!(!fs.exists(Path::new(&specs[40].path)));
    assert!(fs.exists(Path::new(&specs[41].path)));
}

#[test]
fn run_cleanup_should_do_nothing_when_cancelled_before_starting() {
    let specs = many_temp(5);
    let fs = Arc::new(fs_of(&specs));
    let trash = FakeTrash::new(Arc::clone(&fs));
    let log = InMemoryStore::new();
    let cancel = AtomicBool::new(true);
    let mut observer = Recorder::default();

    let summary = run_full(
        preview_of(&specs),
        &matched(),
        ctx(),
        &fs,
        &trash,
        &log,
        &cancel,
        &mut observer,
    );

    assert_eq!(summary.status, CleanupStatus::Cancelled);
    assert_eq!((summary.processed, summary.not_processed), (0, 5));
    assert_eq!(fs.total_calls(), 0);
    assert!(trash.moved().is_empty());
    assert!(logged(&log).is_empty());
    assert_eq!(observer.started, vec![5]);
    assert!(observer.progress.is_empty());
}

#[test]
fn run_cleanup_should_report_completed_when_the_flag_rises_during_the_last_item() {
    let specs = many_temp(3);
    let fs = Arc::new(fs_of(&specs));
    let cancel = Arc::new(AtomicBool::new(false));
    let hook_flag = Arc::clone(&cancel);
    let trash = FakeTrash::new(Arc::clone(&fs)).before_move(&specs[2].path, move || {
        hook_flag.store(true, std::sync::atomic::Ordering::SeqCst);
    });
    let summary = run_full(
        preview_of(&specs),
        &matched(),
        ctx(),
        &fs,
        &trash,
        &InMemoryStore::new(),
        &cancel,
        &mut Recorder::default(),
    );
    assert_eq!(summary.status, CleanupStatus::Completed);
    assert_eq!(summary.not_processed, 0);
}

// ---- 5.7 resumen agregado por motivo y por categoria ----

#[test]
fn run_cleanup_should_aggregate_reasons_by_count_then_outcome_and_totals_by_category() {
    let specs = [
        temp(1, "a.tmp", 10),
        temp(2, "b.tmp", 20),
        temp(3, "c.tmp", 30),
        temp(4, "d.tmp", 40),
        temp(5, "e.tmp", 50),
        temp(6, "f.tmp", 60),
        spec(7, C::TempWindows, CONTROL, 70),
        spec(8, C::TempWindows, r"C:\Windows\Temp\w2.tmp", 80),
    ];
    let fs = Arc::new(fs_of(&specs));
    // 3 reemplazados (identity_changed), 2 en uso al mover, 1 error de permisos, 2 movidos.
    for spec in &specs[0..3] {
        fs.recreate_file(Path::new(&spec.path), id(900 + spec.n));
    }
    let trash = FakeTrash::new(Arc::clone(&fs))
        .fail_on(&specs[3].path, TrashError::InUse)
        .fail_on(&specs[4].path, TrashError::InUse)
        .fail_on(&specs[5].path, TrashError::PermissionDenied);

    let summary = run(preview_of(&specs), &fs, &trash, &InMemoryStore::new());

    let reasons: Vec<(O, &str, u64)> = summary
        .reasons
        .iter()
        .map(|r| (r.outcome, r.detail.as_str(), r.count))
        .collect();
    assert_eq!(
        reasons,
        vec![
            (O::SkippedProtected, "identity_changed", 3),
            (O::SkippedLocked, "in_use", 2),
            (O::Error, "permission_denied", 1),
        ]
    );
    assert_eq!(
        (
            summary.skipped_protected,
            summary.skipped_locked,
            summary.errors,
            summary.moved_count
        ),
        (3, 2, 1, 2)
    );
    let categories: Vec<(C, u64, u64, u64, u64)> = summary
        .categories
        .iter()
        .map(|c| {
            (
                c.category.junk().unwrap(),
                c.moved_count,
                c.moved_bytes,
                c.skipped_count,
                c.error_count,
            )
        })
        .collect();
    assert_eq!(
        categories,
        vec![(C::TempUser, 0, 0, 5, 1), (C::TempWindows, 2, 150, 0, 0)]
    );
}

#[test]
fn run_cleanup_should_order_reasons_with_the_same_count_by_outcome_and_detail() {
    let specs = [
        temp(1, "a.tmp", 10),
        temp(2, "b.tmp", 20),
        temp(3, "c.tmp", 30),
    ];
    let fs = Arc::new(fs_of(&specs));
    let trash = FakeTrash::new(Arc::clone(&fs))
        .fail_on(&specs[0].path, TrashError::PermissionDenied)
        .fail_on(&specs[1].path, TrashError::InUse)
        .fail_on(&specs[2].path, TrashError::NotFound);
    let summary = run(preview_of(&specs), &fs, &trash, &InMemoryStore::new());
    let reasons: Vec<(O, &str)> = summary
        .reasons
        .iter()
        .map(|r| (r.outcome, r.detail.as_str()))
        .collect();
    assert_eq!(
        reasons,
        vec![
            (O::Error, "not_found"),
            (O::Error, "permission_denied"),
            (O::SkippedLocked, "in_use"),
        ]
    );
}

// ---- C-11: fuentes distintas de la basura y verificacion adicional por item ----

fn duplicate_item(spec: &Spec) -> CleanupItem {
    CleanupItem {
        category: CleanupCategory::Duplicates,
        ..item_of(spec)
    }
}

fn dup_spec(n: u128, name: &str, size: u64) -> Spec {
    spec(
        n,
        C::TempUser,
        &format!(r"C:\Users\ana\Downloads\{name}"),
        size,
    )
}

#[test]
fn run_cleanup_should_block_duplicate_items_in_a_mismatch_without_touching_the_disk() {
    for (reason, detail) in [
        (MismatchReason::Undetermined, "blocked_undetermined"),
        (MismatchReason::DifferentUser, "blocked_different_user"),
    ] {
        let specs = [dup_spec(1, "a.iso", 100), dup_spec(2, "b.iso", 200)];
        let fs = Arc::new(fs_of(&specs));
        let trash = FakeTrash::new(Arc::clone(&fs));
        let log = InMemoryStore::new();
        let preview = CleanupPreview::for_tests(
            PreviewId(1),
            SCAN,
            ago(1),
            specs.iter().map(duplicate_item).collect(),
        );
        let calls_before = fs.total_calls();

        let summary = run_full(
            preview,
            &mismatched(reason),
            ctx(),
            &fs,
            &trash,
            &log,
            &AtomicBool::new(false),
            &mut Recorder::default(),
        );

        assert_eq!(summary.skipped_protected, 2, "{reason:?}");
        assert_eq!(summary.moved_count, 0);
        assert!(trash.moved().is_empty());
        assert_eq!(
            fs.total_calls(),
            calls_before,
            "sin fijar ni consultar el perfil"
        );
        let entries = logged(&log);
        assert_eq!(entries.len(), 2);
        for entry in &entries {
            assert_eq!(entry.outcome, O::SkippedProtected);
            assert_eq!(entry.error_detail.as_deref(), Some(detail));
            assert_eq!(entry.category, "duplicates");
        }
        assert_eq!(
            summary
                .categories
                .iter()
                .map(|c| c.category)
                .collect::<Vec<_>>(),
            [CleanupCategory::Duplicates]
        );
    }
}

#[test]
fn run_cleanup_should_log_a_moved_duplicate_with_its_category_and_scan() {
    let specs = [dup_spec(1, "a.iso", 100)];
    let fs = Arc::new(fs_of(&specs));
    let trash = FakeTrash::new(Arc::clone(&fs));
    let log = InMemoryStore::new();
    let preview = CleanupPreview::for_tests(
        PreviewId(1),
        SCAN,
        ago(1),
        specs.iter().map(duplicate_item).collect(),
    );

    let summary = run(preview, &fs, &trash, &log);

    assert_eq!(summary.moved_count, 1);
    let entries = logged(&log);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].category, "duplicates");
    assert_eq!(entries[0].scan_id, Some(SCAN));
    assert_eq!(entries[0].outcome, O::MovedToTrash);
    assert_eq!(summary.categories[0].category, CleanupCategory::Duplicates);
}

/// Guardia guionada: veredicto por `finding_id` (`Proceed` si no figura) y registro de lo visto
/// mientras la fijacion esta viva.
struct Scripted {
    verdicts: Vec<(i64, GuardVerdict)>,
    fs: Arc<FakeFileSystem>,
    trash: Rc<FakeTrash>,
    calls: Vec<i64>,
    pinned_during: Vec<usize>,
    moved_before: Vec<usize>,
}

impl Scripted {
    fn new(
        fs: &Arc<FakeFileSystem>,
        trash: &Rc<FakeTrash>,
        verdicts: Vec<(i64, GuardVerdict)>,
    ) -> Self {
        Self {
            verdicts,
            fs: Arc::clone(fs),
            trash: Rc::clone(trash),
            calls: Vec::new(),
            pinned_during: Vec::new(),
            moved_before: Vec::new(),
        }
    }
}

impl ItemGuard for Scripted {
    fn verify(
        &mut self,
        item: &CleanupItem,
        target: &DeleteTarget,
        _cancel: &AtomicBool,
    ) -> GuardVerdict {
        self.calls.push(item.finding_id);
        self.pinned_during.push(self.fs.pinned().len());
        self.moved_before.push(self.trash.moved().len());
        assert_eq!(target.path(), Path::new(&item.path));
        self.verdicts
            .iter()
            .find(|(id, _)| *id == item.finding_id)
            .map_or(GuardVerdict::Proceed, |(_, verdict)| *verdict)
    }
}

fn run_guarded(
    preview: CleanupPreview,
    fs: &Arc<FakeFileSystem>,
    trash: &FakeTrash,
    log: &InMemoryStore,
    observer: &mut Recorder,
    guard: &mut Scripted,
    progress_every: u64,
) -> CleanupSummary {
    run_cleanup(
        preview,
        &matched(),
        ctx(),
        fs.as_ref(),
        trash,
        log,
        &FixedClock(NOW),
        &AtomicBool::new(false),
        observer,
        guard,
        progress_every,
    )
}

fn five() -> Vec<Spec> {
    (1..=5)
        .map(|n| temp(n, &format!("f{n}.tmp"), 10 * n as u64))
        .collect()
}

#[test]
fn run_cleanup_should_skip_an_item_the_guard_rejects_and_keep_going() {
    let specs = five();
    let fs = Arc::new(fs_of(&specs));
    let trash = Rc::new(FakeTrash::new(Arc::clone(&fs)));
    let log = InMemoryStore::new();
    let mut guard = Scripted::new(
        &fs,
        &trash,
        vec![(
            2,
            GuardVerdict::Skip(O::SkippedLocked, detail::CONTENT_CHANGED),
        )],
    );

    let summary = run_guarded(
        preview_of(&specs),
        &fs,
        &trash,
        &log,
        &mut Recorder::default(),
        &mut guard,
        PROGRESS_EVERY_ITEMS,
    );

    assert_eq!(summary.status, CleanupStatus::Completed);
    assert_eq!((summary.moved_count, summary.skipped_locked), (4, 1));
    let entries = logged(&log);
    assert_eq!(entries.len(), 5);
    assert_eq!(entries[1].outcome, O::SkippedLocked);
    assert_eq!(entries[1].error_detail.as_deref(), Some("content_changed"));
    assert_eq!(entries[1].path, specs[1].path);
    assert!(!moved_paths(&trash).contains(&specs[1].path));
    assert_eq!(guard.calls, [1, 2, 3, 4, 5]);
}

#[test]
fn run_cleanup_should_register_a_guard_skip_as_protected_when_the_guard_says_so() {
    let specs = five();
    let fs = Arc::new(fs_of(&specs[..2]));
    let trash = Rc::new(FakeTrash::new(Arc::clone(&fs)));
    let log = InMemoryStore::new();
    let mut guard = Scripted::new(
        &fs,
        &trash,
        vec![
            (
                1,
                GuardVerdict::Skip(O::SkippedProtected, detail::KEPT_COPY_UNAVAILABLE),
            ),
            (
                2,
                GuardVerdict::Skip(O::SkippedLocked, detail::CONTENT_UNREADABLE),
            ),
        ],
    );

    let summary = run_guarded(
        preview_of(&specs[..2]),
        &fs,
        &trash,
        &log,
        &mut Recorder::default(),
        &mut guard,
        1,
    );

    assert_eq!(summary.moved_count, 0);
    assert_eq!((summary.skipped_protected, summary.skipped_locked), (1, 1));
    assert!(trash.moved().is_empty());
    let details: Vec<_> = logged(&log)
        .iter()
        .map(|e| e.error_detail.clone().unwrap_or_default())
        .collect();
    assert_eq!(details, ["kept_copy_unavailable", "content_unreadable"]);
}

#[test]
fn run_cleanup_should_stop_without_logging_the_item_the_guard_interrupts() {
    let specs = five();
    let fs = Arc::new(fs_of(&specs));
    let trash = Rc::new(FakeTrash::new(Arc::clone(&fs)));
    let log = InMemoryStore::new();
    let mut guard = Scripted::new(&fs, &trash, vec![(3, GuardVerdict::Interrupted)]);
    let mut observer = Recorder::default();

    let summary = run_guarded(
        preview_of(&specs),
        &fs,
        &trash,
        &log,
        &mut observer,
        &mut guard,
        1,
    );

    assert_eq!(summary.status, CleanupStatus::Cancelled);
    assert_eq!(
        (
            summary.total_items,
            summary.processed,
            summary.not_processed
        ),
        (5, 2, 3)
    );
    assert_eq!(summary.moved_count, 2);
    assert_eq!(
        moved_paths(&trash),
        [specs[0].path.clone(), specs[1].path.clone()]
    );
    assert_eq!(logged(&log).len(), 2, "el item interrumpido no se registra");
    assert_eq!(guard.calls, [1, 2, 3], "los items 4 y 5 no se procesan");
    assert_eq!(observer.processed_ids, [1, 2]);
    assert!(fs.exists(Path::new(&specs[2].path)));
    assert!(fs.pinned().is_empty());
}

#[test]
fn run_cleanup_should_run_the_guard_with_the_ancestors_pinned_and_before_moving() {
    let specs = [temp(1, "a.tmp", 10), temp(2, "b.tmp", 20)];
    let fs = Arc::new(fs_of(&specs));
    let trash = Rc::new(FakeTrash::new(Arc::clone(&fs)));
    let log = InMemoryStore::new();
    let mut guard = Scripted::new(&fs, &trash, vec![]);

    let summary = run_guarded(
        preview_of(&specs),
        &fs,
        &trash,
        &log,
        &mut Recorder::default(),
        &mut guard,
        PROGRESS_EVERY_ITEMS,
    );

    assert_eq!(summary.moved_count, 2);
    assert!(
        guard.pinned_during.iter().all(|pinned| *pinned > 0),
        "{:?}",
        guard.pinned_during
    );
    // Cada verificacion ocurre antes de mover ese item (0 movidos antes del 1.o, 1 antes del 2.o).
    assert_eq!(guard.moved_before, [0, 1]);
    assert!(fs.pinned().is_empty(), "se sueltan despues de mover");
}

#[test]
fn run_cleanup_should_not_consult_the_guard_for_an_item_the_standard_checks_reject() {
    let mut recent = temp(1, "recent.tmp", 10);
    recent.age_hours = 2;
    let changed = temp(2, "changed.tmp", 20);
    let ok = temp(3, "ok.tmp", 30);
    let specs = [recent, changed, ok];
    // El archivo `changed.tmp` cambio de tamano despues del escaneo.
    let fs = Arc::new(fs_of(&[
        specs[0].clone(),
        temp(2, "changed.tmp", 21),
        specs[2].clone(),
    ]));
    let trash = Rc::new(FakeTrash::new(Arc::clone(&fs)));
    let log = InMemoryStore::new();
    let mut guard = Scripted::new(&fs, &trash, vec![]);

    let summary = run_guarded(
        preview_of(&specs),
        &fs,
        &trash,
        &log,
        &mut Recorder::default(),
        &mut guard,
        PROGRESS_EVERY_ITEMS,
    );

    assert_eq!(summary.moved_count, 1);
    assert_eq!(summary.skipped_locked, 2);
    assert_eq!(
        guard.calls,
        [3],
        "solo el item que paso las guardas estandar"
    );
}

#[test]
fn run_cleanup_should_report_progress_by_the_requested_cadence() {
    let specs = five();
    let run_with = |every: u64| {
        let fs = Arc::new(fs_of(&specs));
        let trash = Rc::new(FakeTrash::new(Arc::clone(&fs)));
        let mut guard = Scripted::new(&fs, &trash, vec![]);
        let mut observer = Recorder::default();
        run_guarded(
            preview_of(&specs),
            &fs,
            &trash,
            &InMemoryStore::new(),
            &mut observer,
            &mut guard,
            every,
        );
        observer.progress.iter().map(|p| p.0).collect::<Vec<_>>()
    };
    assert_eq!(run_with(1), [1, 2, 3, 4, 5]);
    assert_eq!(run_with(2), [2, 4, 5]);
    assert_eq!(run_with(PROGRESS_EVERY_ITEMS), [5]);
}

#[test]
fn run_cleanup_should_tell_the_observer_which_items_were_processed() {
    let mut recent = temp(2, "recent.tmp", 10);
    recent.age_hours = 1;
    let specs = [temp(1, "a.tmp", 10), recent, temp(3, "c.tmp", 30)];
    let fs = Arc::new(fs_of(&specs));
    let trash = FakeTrash::new(Arc::clone(&fs));
    let mut observer = Recorder::default();

    run_full(
        preview_of(&specs),
        &matched(),
        ctx(),
        &fs,
        &trash,
        &InMemoryStore::new(),
        &AtomicBool::new(false),
        &mut observer,
    );

    // Tambien el omitido por escritura reciente cuenta como procesado.
    assert_eq!(observer.processed_ids, [1, 2, 3]);
}

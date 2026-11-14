//! Tests del guard de contenido ejecutados a traves de `run_cleanup` con dobles.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::*;
use crate::domain::cleanup::execute::{CleanupObserver, run_cleanup};
use crate::domain::cleanup::model::{CleanupPreview, CleanupStatus, CleanupSummary, PreviewId};
use crate::domain::duplicates::decision::Decision;
use crate::domain::duplicates::model::{
    DuplicateFile, DuplicateScanSummary, MinSize, ScanCounters, StoredDuplicateFile,
    StoredDuplicateGroup,
};
use crate::domain::duplicates::preview::{
    DuplicatePreviewInput, KeptCopy, build_duplicate_preview,
};
use crate::domain::model::{
    CleanupLogEntry, CleanupOutcome as O, EpochMillis, Scan, ScanId, ScanKind, ScanStatus,
    ScanTrigger,
};
use crate::domain::ports::fakes::{FakeFileSystem, FakeTrash, FixedClock, InMemoryStore};
use crate::domain::ports::fakes_content::FakeContentReader;
use crate::domain::ports::{CleanupLogStore, FileIdentity, PortError};
use crate::domain::privileges::{PrivilegeContext, UserMatch};
use crate::domain::protection::context::ContentFolder;
use crate::domain::protection::test_support::ctx_c_ana;

const NOW: EpochMillis = EpochMillis(1_800_000_000_000);
const HOUR: i64 = 3_600_000;
const SCAN: ScanId = ScanId(7);
const SIZE: usize = 10_000;

fn old() -> Option<EpochMillis> {
    Some(EpochMillis(NOW.0 - 72 * HOUR))
}

fn bytes(tag: u8, len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| u8::try_from((i * 7 + usize::from(tag) * 13) % 251).unwrap_or(0))
        .collect()
}

fn path(name: &str) -> String {
    format!(r"C:\Users\ana\Downloads\{name}")
}

#[derive(Default)]
struct Obs {
    processed: Vec<i64>,
}

impl CleanupObserver for Obs {
    fn started(&mut self, _: u64) {}
    fn progress(&mut self, _: u64, _: u64, _: u64, _: u64) {}
    fn item_processed(&mut self, finding_id: i64) {
        self.processed.push(finding_id);
    }
}

/// Un archivo del escenario: contenido y fecha.
struct Entry {
    id: i64,
    name: &'static str,
    content: Vec<u8>,
}

fn entry(id: i64, name: &'static str, tag: u8) -> Entry {
    Entry {
        id,
        name,
        content: bytes(tag, SIZE),
    }
}

struct World {
    fs: Arc<FakeFileSystem>,
    reader: FakeContentReader,
    trash: FakeTrash,
    log: InMemoryStore,
    groups: Vec<StoredDuplicateGroup>,
}

/// Construye el mundo: cada `(id de grupo, entradas)` es un grupo cuyo hash es el del contenido
/// de su primera entrada.
fn world(groups: Vec<(i64, Vec<Entry>)>) -> World {
    let mut fs = FakeFileSystem::new()
        .canonical_as_is()
        .with_dir(r"C:\Users")
        .with_dir(r"C:\Users\ana")
        .with_dir(r"C:\Users\ana\Downloads");
    for (_, entries) in &groups {
        for e in entries {
            fs = fs.with_file(&path(e.name), e.content.len() as u64, old());
        }
    }
    let fs = Arc::new(fs);
    let mut reader = FakeContentReader::new(Arc::clone(&fs));
    for (_, entries) in &groups {
        for e in entries {
            reader = reader.with_content(&path(e.name), e.content.clone());
        }
    }
    let mut stored = Vec::new();
    for (group_id, entries) in groups {
        let blake3 = to_hex(&blake3::hash(&entries[0].content));
        let files = entries
            .iter()
            .map(|e| StoredDuplicateFile {
                id: e.id,
                file: DuplicateFile {
                    root: ContentFolder::Downloads,
                    path: path(e.name),
                    modified: old(),
                    identity: fs
                        .peek(Path::new(&path(e.name)))
                        .and_then(|m| m.identity)
                        .unwrap(),
                },
                kept: false,
            })
            .collect();
        stored.push(StoredDuplicateGroup {
            id: group_id,
            size_bytes: SIZE as u64,
            blake3,
            resolved_at: None,
            files,
        });
    }
    World {
        trash: FakeTrash::new(Arc::clone(&fs)),
        fs,
        reader,
        log: InMemoryStore::new(),
        groups: stored,
    }
}

fn decide(group_id: i64, keep: &[i64]) -> Decision {
    Decision {
        group_id,
        keep_file_ids: keep.to_vec(),
    }
}

fn summary_of(scan: ScanId) -> DuplicateScanSummary {
    DuplicateScanSummary {
        scan: Scan {
            id: scan,
            kind: ScanKind::Duplicates,
            trigger: ScanTrigger::Manual,
            status: ScanStatus::Done,
            started_at: EpochMillis(1),
            finished_at: Some(EpochMillis(2)),
        },
        min_size: MinSize::MIB_1,
        roots: Vec::new(),
        counters: ScanCounters::default(),
        group_count: 1,
        duplicate_file_count: 2,
        reclaimable_bytes: 0,
        open_group_count: 1,
    }
}

fn run_world(w: &World, decisions: &[Decision], cancel: &AtomicBool) -> (CleanupSummary, Obs) {
    let preview = build_duplicate_preview(DuplicatePreviewInput {
        requested_scan: SCAN,
        last_done: Some(&summary_of(SCAN)),
        decisions,
        groups: &w.groups,
        now: NOW,
        id: PreviewId(1),
    })
    .unwrap();
    let (core, plan) = preview.into_parts();
    run_plan(w, core, &plan, cancel)
}

fn run_plan(
    w: &World,
    core: CleanupPreview,
    plan: &DuplicatePlan,
    cancel: &AtomicBool,
) -> (CleanupSummary, Obs) {
    let mut observer = Obs::default();
    let mut guard = DuplicateContentGuard::new(&w.reader, plan);
    let summary = run_cleanup(
        core,
        &PrivilegeContext::new(UserMatch::Match, false),
        Ok(ctx_c_ana()),
        w.fs.as_ref(),
        &w.trash,
        &w.log,
        &FixedClock(NOW),
        cancel,
        &mut observer,
        &mut guard,
        1,
    );
    (summary, observer)
}

fn entries_of(w: &World) -> Vec<CleanupLogEntry> {
    let mut entries = w.log.recent_cleanup_log(1_000).unwrap();
    entries.reverse();
    entries
}

fn moved_names(w: &World) -> Vec<String> {
    w.trash
        .moved()
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect()
}

fn still_there(w: &World, name: &str) -> bool {
    w.fs.exists(Path::new(&path(name)))
}

fn no_cancel() -> AtomicBool {
    AtomicBool::new(false)
}

fn standard() -> World {
    world(vec![(
        4,
        vec![entry(41, "keep.bin", 1), entry(42, "copy.bin", 1)],
    )])
}

#[test]
fn guard_should_move_the_copy_and_leave_the_kept_one_untouched_when_everything_matches() {
    let w = standard();
    let (summary, observer) = run_world(&w, &[decide(4, &[41])], &no_cancel());
    assert_eq!(summary.status, CleanupStatus::Completed);
    assert_eq!(summary.moved_count, 1);
    assert_eq!(moved_names(&w), ["copy.bin"]);
    assert!(still_there(&w, "keep.bin"));
    assert!(!still_there(&w, "copy.bin"));
    let entries = entries_of(&w);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].category, "duplicates");
    assert_eq!(entries[0].outcome, O::MovedToTrash);
    assert_eq!(observer.processed, [42]);
    assert!(w.fs.pinned().is_empty());
}

#[test]
fn guard_should_skip_a_copy_whose_content_changed_even_with_the_same_size_and_date() {
    let w = standard();
    // Mismo tamano y fecha, otro contenido.
    w.reader
        .replace_content(Path::new(&path("copy.bin")), bytes(9, SIZE));
    let (summary, _) = run_world(&w, &[decide(4, &[41])], &no_cancel());
    assert_eq!(summary.moved_count, 0);
    assert_eq!(summary.skipped_locked, 1);
    let entries = entries_of(&w);
    assert_eq!(entries[0].outcome, O::SkippedLocked);
    assert_eq!(entries[0].error_detail.as_deref(), Some("content_changed"));
    assert!(still_there(&w, "copy.bin"));
    assert!(w.trash.moved().is_empty());
}

#[test]
fn guard_should_skip_a_copy_open_without_sharing_read_as_in_use() {
    let mut w = standard();
    w.reader = std::mem::replace(&mut w.reader, FakeContentReader::new(Arc::clone(&w.fs)))
        .fail_open(&path("copy.bin"), PortError::InUse("x".into()));
    let (summary, _) = run_world(&w, &[decide(4, &[41])], &no_cancel());
    assert_eq!(summary.skipped_locked, 1);
    let entries = entries_of(&w);
    assert_eq!(entries[0].outcome, O::SkippedLocked);
    assert_eq!(entries[0].error_detail.as_deref(), Some("in_use"));
    assert!(still_there(&w, "copy.bin"));
}

#[test]
fn guard_should_skip_an_unreadable_copy_as_content_unreadable() {
    let mut w = standard();
    w.reader = std::mem::replace(&mut w.reader, FakeContentReader::new(Arc::clone(&w.fs)))
        .fail_read(&path("copy.bin"), PortError::Io("sector".into()));
    let (_, _) = run_world(&w, &[decide(4, &[41])], &no_cancel());
    let entries = entries_of(&w);
    assert_eq!(entries[0].outcome, O::SkippedLocked);
    assert_eq!(
        entries[0].error_detail.as_deref(),
        Some("content_unreadable")
    );
    assert!(still_there(&w, "copy.bin"));
}

#[test]
fn guard_should_skip_a_copy_replaced_by_another_file_as_identity_changed() {
    let mut w = standard();
    let fs = Arc::clone(&w.fs);
    w.reader = std::mem::replace(&mut w.reader, FakeContentReader::new(Arc::clone(&w.fs))).on_open(
        &path("copy.bin"),
        move || {
            fs.recreate_file(
                Path::new(&path("copy.bin")),
                FileIdentity {
                    volume_serial: 1,
                    file_id: 31_337,
                },
            );
        },
    );
    let (summary, _) = run_world(&w, &[decide(4, &[41])], &no_cancel());
    assert_eq!(summary.skipped_protected, 1);
    let entries = entries_of(&w);
    assert_eq!(entries[0].outcome, O::SkippedProtected);
    assert_eq!(entries[0].error_detail.as_deref(), Some("identity_changed"));
    assert!(w.trash.moved().is_empty());
}

#[test]
fn guard_should_protect_every_copy_of_a_group_whose_kept_copy_is_gone_and_keep_the_other_groups() {
    let w = world(vec![
        (
            4,
            vec![
                entry(41, "keep.bin", 1),
                entry(42, "copy_a.bin", 1),
                entry(43, "copy_b.bin", 1),
            ],
        ),
        (
            5,
            vec![entry(51, "keep2.bin", 2), entry(52, "copy2.bin", 2)],
        ),
    ]);
    // El usuario borro la unica copia que eligio conservar en el grupo 4.
    w.fs.remove(Path::new(&path("keep.bin")));
    let (summary, observer) = run_world(&w, &[decide(4, &[41]), decide(5, &[51])], &no_cancel());
    assert_eq!(summary.moved_count, 1);
    assert_eq!(summary.skipped_protected, 2);
    assert_eq!(moved_names(&w), ["copy2.bin"]);
    let details: Vec<(String, Option<String>)> = entries_of(&w)
        .into_iter()
        .map(|e| {
            (
                e.path.rsplit('\\').next().unwrap().to_string(),
                e.error_detail,
            )
        })
        .collect();
    assert_eq!(
        details,
        [
            (
                "copy_a.bin".to_string(),
                Some("kept_copy_unavailable".into())
            ),
            (
                "copy_b.bin".to_string(),
                Some("kept_copy_unavailable".into())
            ),
            ("copy2.bin".to_string(), None),
        ]
    );
    assert!(still_there(&w, "copy_a.bin") && still_there(&w, "copy_b.bin"));
    // Se verifico una sola vez la conservada del grupo 4, aunque habia 2 copias a eliminar.
    assert_eq!(w.reader.open_count(&path("keep.bin")), 1);
    assert_eq!(observer.processed, [42, 43, 52]);
}

#[test]
fn guard_should_protect_the_group_when_the_kept_copy_changed_its_content() {
    let w = standard();
    w.reader
        .replace_content(Path::new(&path("keep.bin")), bytes(8, SIZE));
    let (summary, _) = run_world(&w, &[decide(4, &[41])], &no_cancel());
    assert_eq!(summary.skipped_protected, 1);
    assert_eq!(
        entries_of(&w)[0].error_detail.as_deref(),
        Some("kept_copy_unavailable")
    );
    assert!(still_there(&w, "copy.bin"));
}

#[test]
fn guard_should_protect_the_group_when_the_kept_copy_changed_its_date() {
    let w = standard();
    w.fs.set_size_and_time(
        Path::new(&path("keep.bin")),
        SIZE as u64,
        Some(EpochMillis(5)),
    );
    let (summary, _) = run_world(&w, &[decide(4, &[41])], &no_cancel());
    assert_eq!(summary.skipped_protected, 1);
    assert!(still_there(&w, "copy.bin"));
}

#[test]
fn guard_should_proceed_when_one_of_two_kept_copies_is_valid() {
    let w = world(vec![(
        4,
        vec![
            entry(41, "keep1.bin", 1),
            entry(42, "keep2.bin", 1),
            entry(43, "copy.bin", 1),
        ],
    )]);
    // La primera conservada ya no esta; la segunda sigue igual.
    w.fs.remove(Path::new(&path("keep1.bin")));
    let (summary, _) = run_world(&w, &[decide(4, &[41, 42])], &no_cancel());
    assert_eq!(summary.moved_count, 1);
    assert_eq!(moved_names(&w), ["copy.bin"]);
    assert!(still_there(&w, "keep2.bin"));
}

#[test]
fn guard_should_interrupt_without_logging_when_cancelled_while_rehashing_a_large_copy() {
    // La copia a eliminar es de 4 GiB (virtual); la conservada, chica y con el hash del grupo.
    let huge = 4u64 << 30;
    let kept_content = bytes(1, SIZE);
    let fs = Arc::new(
        FakeFileSystem::new()
            .canonical_as_is()
            .with_dir(r"C:\Users")
            .with_dir(r"C:\Users\ana")
            .with_dir(r"C:\Users\ana\Downloads")
            .with_file(&path("keep.bin"), SIZE as u64, old())
            .with_file(&path("huge.bin"), huge, old()),
    );
    let identity = |name: &str| {
        fs.peek(Path::new(&path(name)))
            .and_then(|m| m.identity)
            .unwrap()
    };
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancel);
    let reader = FakeContentReader::new(Arc::clone(&fs))
        .with_content(&path("keep.bin"), kept_content.clone())
        .with_filled(&path("huge.bin"), huge, 3)
        .after_read(&path("huge.bin"), 1, move || {
            flag.store(true, Ordering::Relaxed);
        });
    let plan = DuplicatePlan {
        groups: vec![GroupPlan {
            group_id: 4,
            blake3: to_hex(&blake3::hash(&kept_content)),
            size_bytes: huge,
            kept: vec![KeptCopy {
                path: path("keep.bin"),
                identity: identity("keep.bin"),
                size_bytes: SIZE as u64,
                modified: old(),
            }],
            kept_file_ids: vec![41],
            item_file_ids: vec![42],
        }],
    };
    let core = CleanupPreview::for_tests(
        PreviewId(1),
        SCAN,
        EpochMillis(NOW.0 - HOUR),
        vec![CleanupItem {
            finding_id: 42,
            category: crate::domain::cleanup::model::CleanupCategory::Duplicates,
            path: path("huge.bin"),
            size_bytes: huge,
            modified_at: old(),
            identity: identity("huge.bin"),
        }],
    );
    let w = World {
        trash: FakeTrash::new(Arc::clone(&fs)),
        fs: Arc::clone(&fs),
        reader,
        log: InMemoryStore::new(),
        groups: Vec::new(),
    };

    let (summary, observer) = run_plan(&w, core, &plan, &cancel);

    assert_eq!(summary.status, CleanupStatus::Cancelled);
    assert_eq!((summary.processed, summary.not_processed), (0, 1));
    assert!(
        entries_of(&w).is_empty(),
        "el item interrumpido no se registra"
    );
    assert!(observer.processed.is_empty());
    assert!(w.trash.moved().is_empty());
    assert!(w.fs.exists(Path::new(&path("huge.bin"))));
    // Se leyo solo el primer bloque de 1 MiB de los 4 GiB.
    assert_eq!(w.reader.bytes_read_of(&path("huge.bin")), 1 << 20);
}

#[test]
fn guard_should_interrupt_when_cancelled_while_verifying_the_kept_copy() {
    let w = standard();
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancel);
    let mut w = w;
    w.reader = std::mem::replace(&mut w.reader, FakeContentReader::new(Arc::clone(&w.fs)))
        .after_read(&path("keep.bin"), 1, move || {
            flag.store(true, Ordering::Relaxed);
        });
    let (summary, _) = run_world(&w, &[decide(4, &[41])], &cancel);
    assert_eq!(summary.status, CleanupStatus::Cancelled);
    assert!(entries_of(&w).is_empty());
    assert!(still_there(&w, "copy.bin"));
}

#[test]
fn guard_should_refuse_an_item_that_is_not_in_the_plan() {
    let w = standard();
    let preview = build_duplicate_preview(DuplicatePreviewInput {
        requested_scan: SCAN,
        last_done: Some(&summary_of(SCAN)),
        decisions: &[decide(4, &[41])],
        groups: &w.groups,
        now: NOW,
        id: PreviewId(1),
    })
    .unwrap();
    let (core, _) = preview.into_parts();
    let empty_plan = DuplicatePlan::default();
    let (summary, _) = run_plan(&w, core, &empty_plan, &no_cancel());
    assert_eq!(summary.skipped_protected, 1);
    assert!(still_there(&w, "copy.bin"));
}

#[test]
fn guard_should_keep_the_ancestors_pinned_while_it_reads() {
    let w = standard();
    let fs = Arc::clone(&w.fs);
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen_hook = Arc::clone(&seen);
    let mut w = w;
    w.reader = std::mem::replace(&mut w.reader, FakeContentReader::new(Arc::clone(&w.fs)))
        .after_read(&path("copy.bin"), 1, move || {
            if let Ok(mut seen) = seen_hook.lock() {
                seen.push(fs.pinned().len());
            }
        });
    let _ = run_world(&w, &[decide(4, &[41])], &no_cancel());
    let seen = seen.lock().unwrap().clone();
    assert!(!seen.is_empty() && seen.iter().all(|n| *n > 0), "{seen:?}");
    assert!(w.fs.pinned().is_empty());
}

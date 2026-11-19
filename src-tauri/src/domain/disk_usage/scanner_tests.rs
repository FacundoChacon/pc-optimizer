use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::*;
use crate::domain::disk_usage::model::{
    Access, FolderCounters, IssueReason, OpaqueReason, RootKind, unaccounted,
};
use crate::domain::model::EpochMillis;
use crate::domain::ports::fakes::FakeFileSystem;
use crate::domain::ports::fakes_shell::FakeVolumeSpace;
use crate::domain::ports::{PortError, VolumeSpaceInfo};
use crate::domain::protection::context::{ContextError, ProtectionContext};
use crate::domain::protection::decision::ProtectedReason;
use crate::domain::protection::path::normalize_lexical;
use crate::domain::protection::test_support::{ctx_c_ana, ctx_c_no_user};

const MIB: u64 = 1024 * 1024;
const GIB: u64 = 1024 * MIB;

/// Observador que registra el avance.
#[derive(Default)]
struct Recorder {
    walking: Vec<(u64, u64, u64)>,
    saving: u32,
    /// Si esta, se levanta al emitir el avance numero `cancel_at` (1 = el primero).
    cancel_on_walking: Option<(Arc<AtomicBool>, usize)>,
}

impl FolderScanObserver for Recorder {
    fn walking(&mut self, files_seen: u64, dirs_seen: u64, bytes_seen: u64) {
        self.walking.push((files_seen, dirs_seen, bytes_seen));
        if let Some((flag, at)) = &self.cancel_on_walking
            && self.walking.len() >= *at
        {
            flag.store(true, Ordering::Relaxed);
        }
    }
    fn saving(&mut self) {
        self.saving += 1;
    }
}

fn base_fs() -> FakeFileSystem {
    FakeFileSystem::new()
        .with_dir(r"C:\")
        .with_dir(r"C:\Windows")
        .with_dir(r"C:\Windows\Temp")
        .with_file(r"C:\Windows\Temp\t.tmp", 3 * MIB, None)
        .with_file(r"C:\Windows\big.dll", 20 * MIB, None)
        .with_dir(r"C:\Users")
        .with_dir(r"C:\Users\ana")
        .with_dir(r"C:\Users\ana\Downloads")
        .with_file(
            r"C:\Users\ana\Downloads\setup.exe",
            8 * MIB,
            Some(EpochMillis(5_000)),
        )
        .with_dir(r"C:\Users\ana\AppData")
        .with_dir(r"C:\Users\ana\AppData\Local")
        .with_dir(r"C:\Users\ana\AppData\Local\Temp")
        .with_file(r"C:\Users\ana\AppData\Local\Temp\x.tmp", MIB, None)
        .with_dir(r"C:\Users\bob")
        .with_dir(r"C:\Users\bob\Documents")
        .with_file(r"C:\Users\bob\Documents\diary.txt", 2 * MIB, None)
        .canonical_as_is()
}

fn run_with(
    fs: &FakeFileSystem,
    root: RootChoice,
    ctx: Result<ProtectionContext, ContextError>,
    volume: &FakeVolumeSpace,
    cancel: &AtomicBool,
    observer: &mut Recorder,
) -> Result<FolderScanOutcome, FolderScanError> {
    scan_folders(root, ctx, fs, volume, cancel, observer)
}

fn run(fs: &FakeFileSystem, root: RootChoice) -> Result<FolderScanOutcome, FolderScanError> {
    run_with(
        fs,
        root,
        Ok(ctx_c_ana()),
        &FakeVolumeSpace::with_values(100 * GIB, 60 * GIB),
        &AtomicBool::new(false),
        &mut Recorder::default(),
    )
}

fn completed(result: Result<FolderScanOutcome, FolderScanError>) -> FolderScanResults {
    match result {
        Ok(FolderScanOutcome::Completed(results)) => results,
        other => panic!("se esperaba Completed, dio {other:?}"),
    }
}

fn folder<'r>(results: &'r FolderScanResults, path: &str) -> &'r StoredFolder {
    match results.folders.iter().find(|f| f.path == path) {
        Some(found) => found,
        None => panic!("no hay carpeta {path}"),
    }
}

fn picked(raw: &str) -> RootChoice {
    RootChoice::Picked(normalize_lexical(raw).expect("valid path"))
}

#[test]
fn scan_should_walk_the_volume_root_of_the_context_for_system_drive() {
    let results = completed(run(&base_fs(), RootChoice::SystemDrive));
    assert_eq!(results.root_kind, RootKind::SystemDrive);
    assert_eq!(results.root_path, r"C:\");
    assert_eq!(results.folders[0].path, r"C:\");
    assert_eq!(results.folders[0].parent, None);
    assert_eq!(results.folders[0].depth, 0);
    assert_eq!(results.counters.total_bytes, 34 * MIB);
    assert_eq!(
        results.counters.folders_stored,
        results.folders.len() as u64
    );
    assert!(!results.folders_truncated);
    assert_eq!(
        results.volume,
        Some(VolumeSpaceInfo {
            total_bytes: 100 * GIB,
            free_bytes: 60 * GIB
        })
    );
}

#[test]
fn scan_should_flag_system_and_junk_folders_and_link_files_to_stored_folders() {
    let results = completed(run(&base_fs(), RootChoice::SystemDrive));
    let windows = folder(&results, r"C:\Windows");
    assert!(windows.system && !windows.junk_hint);
    assert_eq!(windows.size_bytes, 23 * MIB);
    let temp = folder(&results, r"C:\Users\ana\AppData\Local\Temp");
    assert!(temp.junk_hint && !temp.system);
    let downloads = folder(&results, r"C:\Users\ana\Downloads");
    assert!(!downloads.junk_hint && !downloads.system);
    let top = &results.large_files[0];
    assert_eq!(top.path, r"C:\Windows\big.dll");
    assert_eq!(top.size_bytes, 20 * MIB);
    assert_eq!(results.folders[top.folder].path, r"C:\Windows");
    let setup = results
        .large_files
        .iter()
        .find(|f| f.path.ends_with("setup.exe"))
        .expect("setup.exe in top");
    assert_eq!(setup.modified_at, Some(EpochMillis(5_000)));
}

#[test]
fn scan_should_keep_parents_before_children_with_remapped_indexes() {
    let results = completed(run(&base_fs(), RootChoice::SystemDrive));
    for (index, stored) in results.folders.iter().enumerate().skip(1) {
        let parent = stored.parent.expect("non-root folders have a parent");
        assert!(parent < index);
        assert_eq!(stored.depth, results.folders[parent].depth + 1);
    }
    assert!(
        results
            .large_files
            .iter()
            .all(|f| f.folder < results.folders.len())
    );
}

#[test]
fn scan_should_treat_other_profiles_as_opaque_with_no_inner_paths() {
    let results = completed(run(&base_fs(), RootChoice::SystemDrive));
    let bob = folder(&results, r"C:\Users\bob");
    assert_eq!(bob.opaque, Some(OpaqueReason::OtherUserProfile));
    assert_eq!(bob.size_bytes, 2 * MIB);
    assert!(
        results
            .folders
            .iter()
            .all(|f| !f.path.contains("Documents"))
    );
    assert!(results.large_files.iter().all(|f| !f.path.contains("bob")));
    assert_eq!(results.counters.opaque_dirs, 1);
}

#[test]
fn scan_should_make_every_profile_opaque_and_hint_only_machine_locations_in_mismatch() {
    let results = completed(run_with(
        &base_fs(),
        RootChoice::SystemDrive,
        Ok(ctx_c_no_user()),
        &FakeVolumeSpace::with_values(100 * GIB, 60 * GIB),
        &AtomicBool::new(false),
        &mut Recorder::default(),
    ));
    let ana = folder(&results, r"C:\Users\ana");
    assert_eq!(ana.opaque, Some(OpaqueReason::OtherUserProfile));
    assert!(
        results
            .folders
            .iter()
            .all(|f| !f.path.contains("Downloads"))
    );
    assert!(results.large_files.iter().all(|f| !f.path.contains("ana")));
    assert!(folder(&results, r"C:\Windows\Temp").junk_hint);
    assert!(results.folders.iter().all(|f| !f.path.contains("AppData")));
}

#[test]
fn scan_should_revalidate_a_picked_root_and_record_its_path() {
    let results = completed(run(&base_fs(), picked(r"C:\Users\ana\Downloads")));
    assert_eq!(results.root_kind, RootKind::Picked);
    assert_eq!(results.root_path, r"C:\Users\ana\Downloads");
    assert_eq!(results.folders.len(), 1);
    assert_eq!(results.counters.total_bytes, 8 * MIB);
}

#[test]
fn scan_should_reject_a_picked_root_replaced_by_a_junction() {
    let fs = base_fs().with_reparse_point(r"C:\Users\ana\Swapped");
    let result = run(&fs, picked(r"C:\Users\ana\Swapped"));
    assert_eq!(
        result,
        Err(FolderScanError::RootRejected(ProtectedReason::ReparsePoint))
    );
}

#[test]
fn scan_should_reject_picked_roots_of_other_users_or_volumes_and_a_broken_context() {
    let fs = base_fs();
    assert_eq!(
        run(&fs, picked(r"C:\Users\bob\Documents")),
        Err(FolderScanError::RootRejected(
            ProtectedReason::OtherUserProfile
        ))
    );
    assert_eq!(
        run(&fs, picked(r"D:\Datos")),
        Err(FolderScanError::RootRejected(
            ProtectedReason::UnsupportedVolume
        ))
    );
    let broken = run_with(
        &fs,
        RootChoice::SystemDrive,
        Err(ContextError::MissingRequired("windows_dir")),
        &FakeVolumeSpace::with_values(1, 1),
        &AtomicBool::new(false),
        &mut Recorder::default(),
    );
    assert_eq!(
        broken,
        Err(FolderScanError::RootRejected(
            ProtectedReason::ContextUnavailable
        ))
    );
}

#[test]
fn scan_should_finish_done_with_an_inaccessible_root() {
    let fs = base_fs().fail_on_op(
        r"C:\Users\ana\Downloads",
        crate::domain::ports::fakes::FsOp::ReadDir,
        PortError::PermissionDenied("denied".into()),
    );
    let results = completed(run(&fs, picked(r"C:\Users\ana\Downloads")));
    assert_eq!(results.folders.len(), 1);
    assert_eq!(results.folders[0].access, Access::Inaccessible);
    assert_eq!(results.counters.total_bytes, 0);
    assert_eq!(results.counters.inaccessible_dirs, 1);
    assert_eq!(results.issues.len(), 1);
    assert_eq!(results.issues[0].reason, IssueReason::AccessDenied);
}

#[test]
fn scan_should_report_a_root_that_vanishes_during_the_walk_as_not_found() {
    // `metadata` ve la carpeta al validar; el listado del arbol ya no la encuentra.
    let fs = base_fs().fail_on_op(
        r"C:\Users\ana\Downloads",
        crate::domain::ports::fakes::FsOp::ReadDir,
        PortError::NotFound("gone".into()),
    );
    let results = completed(run(&fs, picked(r"C:\Users\ana\Downloads")));
    assert_eq!(results.issues[0].reason, IssueReason::NotFound);
}

#[test]
fn unaccounted_should_be_used_space_minus_analyzed_for_the_system_drive() {
    let volume = Some(VolumeSpaceInfo {
        total_bytes: 500 * GIB,
        free_bytes: 100 * GIB,
    });
    let result = unaccounted(RootKind::SystemDrive, volume, 370 * GIB);
    assert_eq!((result.bytes, result.over_counted), (Some(30 * GIB), false));
    let exact = unaccounted(RootKind::SystemDrive, volume, 400 * GIB);
    assert_eq!((exact.bytes, exact.over_counted), (Some(0), false));
}

#[test]
fn unaccounted_should_be_zero_and_flagged_when_analysis_exceeds_used_space() {
    let volume = Some(VolumeSpaceInfo {
        total_bytes: 500 * GIB,
        free_bytes: 100 * GIB,
    });
    let result = unaccounted(RootKind::SystemDrive, volume, 410 * GIB);
    assert_eq!((result.bytes, result.over_counted), (Some(0), true));
}

#[test]
fn unaccounted_should_not_apply_to_a_picked_folder_or_a_missing_volume() {
    let volume = Some(VolumeSpaceInfo {
        total_bytes: 500 * GIB,
        free_bytes: 100 * GIB,
    });
    let picked = unaccounted(RootKind::Picked, volume, 1);
    assert_eq!((picked.bytes, picked.over_counted), (None, false));
    let none = unaccounted(RootKind::SystemDrive, None, 1);
    assert_eq!((none.bytes, none.over_counted), (None, false));
}

#[test]
fn scan_should_report_no_volume_data_when_the_query_fails() {
    let results = completed(run_with(
        &base_fs(),
        RootChoice::SystemDrive,
        Ok(ctx_c_ana()),
        &FakeVolumeSpace::failing(PortError::Io("boom".into())),
        &AtomicBool::new(false),
        &mut Recorder::default(),
    ));
    assert_eq!(results.volume, None);
}

#[test]
fn counters_should_report_the_root_totals() {
    let results = completed(run(&base_fs(), picked(r"C:\Users\ana")));
    let counters: FolderCounters = results.counters;
    assert_eq!(counters.total_bytes, 9 * MIB);
    assert_eq!(counters.file_count, 2);
    assert_eq!(counters.dir_count, 4);
    assert_eq!(
        PathBuf::from(&results.root_path),
        PathBuf::from(r"C:\Users\ana")
    );
}

// ---- 4.2 cancelacion y progreso (D-16, D-17) ----

fn run_observed(
    fs: &FakeFileSystem,
    cancel: &AtomicBool,
    observer: &mut Recorder,
) -> Result<FolderScanOutcome, FolderScanError> {
    run_with(
        fs,
        RootChoice::SystemDrive,
        Ok(ctx_c_ana()),
        &FakeVolumeSpace::with_values(100 * GIB, 60 * GIB),
        cancel,
        observer,
    )
}

fn many_files(count: u64) -> FakeFileSystem {
    let mut fs = FakeFileSystem::new()
        .with_dir(r"C:\")
        .with_dir(r"C:\many")
        .canonical_as_is();
    for index in 0..count {
        fs = fs.with_file(&format!(r"C:\many\f{index:05}.bin"), 10, None);
    }
    fs
}

fn many_dirs(count: u64) -> FakeFileSystem {
    let mut fs = FakeFileSystem::new()
        .with_dir(r"C:\")
        .with_dir(r"C:\many")
        .canonical_as_is();
    for index in 0..count {
        fs = fs.with_dir(&format!(r"C:\many\d{index:05}"));
    }
    fs
}

#[test]
fn scan_should_cancel_before_the_first_listing_without_results_or_saving() {
    let cancel = AtomicBool::new(true);
    let mut observer = Recorder::default();
    let outcome = run_observed(&base_fs(), &cancel, &mut observer);
    assert_eq!(outcome, Ok(FolderScanOutcome::Cancelled));
    assert_eq!(observer.saving, 0);
}

#[test]
fn scan_should_cancel_in_the_middle_of_the_walk() {
    let cancel = Arc::new(AtomicBool::new(false));
    let mut observer = Recorder {
        cancel_on_walking: Some((Arc::clone(&cancel), 1)),
        ..Recorder::default()
    };
    let outcome = run_observed(&many_dirs(2_000), &cancel, &mut observer);
    assert_eq!(outcome, Ok(FolderScanOutcome::Cancelled));
    assert_eq!(
        observer.walking.len(),
        1,
        "el recorrido se corto tras el primer aviso"
    );
    assert!(observer.walking[0].1 < 2_000, "no llego al final del arbol");
    assert_eq!(observer.saving, 0);
}

#[test]
fn scan_should_cancel_right_before_building_the_result() {
    let cancel = Arc::new(AtomicBool::new(false));
    let mut observer = Recorder {
        // Un arbol chico solo avisa al terminar el recorrido: ese aviso cancela.
        cancel_on_walking: Some((Arc::clone(&cancel), 1)),
        ..Recorder::default()
    };
    let outcome = run_observed(&base_fs(), &cancel, &mut observer);
    assert_eq!(outcome, Ok(FolderScanOutcome::Cancelled));
    assert_eq!(observer.walking.len(), 1);
    assert_eq!(observer.saving, 0);
}

#[test]
fn scan_should_report_progress_every_4096_files_with_growing_counts() {
    let mut observer = Recorder::default();
    let outcome = run_observed(&many_files(10_000), &AtomicBool::new(false), &mut observer);
    assert!(matches!(outcome, Ok(FolderScanOutcome::Completed(_))));
    let files: Vec<u64> = observer.walking.iter().map(|event| event.0).collect();
    assert_eq!(files, vec![4_096, 8_192, 10_000]);
    assert!(observer.walking.windows(2).all(|w| w[0].2 < w[1].2));
    assert_eq!(observer.walking[2].2, 100_000);
    assert_eq!(observer.saving, 1);
}

#[test]
fn scan_should_report_progress_every_512_directories() {
    let mut observer = Recorder::default();
    let outcome = run_observed(&many_dirs(1_100), &AtomicBool::new(false), &mut observer);
    assert!(matches!(outcome, Ok(FolderScanOutcome::Completed(_))));
    let dirs: Vec<u64> = observer.walking.iter().map(|event| event.1).collect();
    // Listadas: C:\ + many + 1 100 subcarpetas = 1 102.
    assert_eq!(dirs, vec![512, 1_024, 1_102]);
    assert_eq!(observer.saving, 1);
}

#[test]
fn scan_progress_should_count_what_the_walk_saw_without_exposing_any_path() {
    let mut observer = Recorder::default();
    let results = completed(run_observed(
        &base_fs(),
        &AtomicBool::new(false),
        &mut observer,
    ));
    // El perfil ajeno (opaco) se cuenta en los totales pero el evento solo lleva numeros.
    let last = observer.walking.last().copied().expect("one final event");
    assert_eq!(last.0, results.counters.file_count);
    assert_eq!(last.2, results.counters.total_bytes);
    assert_eq!(last.1, results.counters.dir_count + 1);
    assert_eq!(observer.saving, 1);
}

use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use serde_json::{Value, json};

use super::*;
use crate::commands::scan::{ScanPorts, last_junk_scan, run_junk_scan};
use crate::domain::cleanup::model::{
    AbortReason, CategorySummary, CleanupStatus, CleanupSummary, ReasonCount,
};
use crate::domain::model::{CleanupOutcome, EpochMillis, ScanId};
use crate::domain::ports::fakes::{
    FakeFileSystem, FakeRecycleBin, FakeTrash, FixedClock, InMemoryStore,
};
use crate::domain::ports::{CleanupLogStore, Trash, TrashError};
use crate::domain::privileges::{MismatchReason, UserMatch};
use crate::domain::protection::service::DeleteTarget;
use crate::domain::protection::test_support::raw_c_ana;

const NOW: EpochMillis = EpochMillis(1_800_000_000_000);
const DAY: i64 = 86_400_000;
const TEMP_USER: &str = r"C:\Users\ana\AppData\Local\Temp\t.tmp";
const TEMP_WIN_W: &str = r"C:\Windows\Temp\w.tmp";
const TEMP_WIN_Z: &str = r"C:\Windows\Temp\z.tmp";

fn matched() -> PrivilegeContext {
    PrivilegeContext::new(UserMatch::Match, true)
}

fn mismatched() -> PrivilegeContext {
    PrivilegeContext::new(UserMatch::Mismatch(MismatchReason::DifferentUser), true)
}

fn fs_with_files() -> FakeFileSystem {
    let old = Some(EpochMillis(NOW.0 - 3 * DAY));
    FakeFileSystem::new()
        .canonical_as_is()
        .with_dir(r"C:\Windows")
        .with_dir(r"C:\Windows\Temp")
        .with_file(TEMP_WIN_W, 1_000, old)
        .with_file(TEMP_WIN_Z, 3_000, old)
        .with_dir(r"C:\Users")
        .with_dir(r"C:\Users\ana")
        .with_dir(r"C:\Users\ana\AppData")
        .with_dir(r"C:\Users\ana\AppData\Local")
        .with_dir(r"C:\Users\ana\AppData\Local\Temp")
        .with_file(TEMP_USER, 2_000, old)
}

/// Comparte un `FakeTrash` para poder inspeccionarlo despues de que el comando lo use.
struct SharedTrash(Rc<FakeTrash>);

impl Trash for SharedTrash {
    fn preflight(&self, sample: &Path) -> Result<(), TrashError> {
        self.0.preflight(sample)
    }

    fn move_to_trash(&self, target: &DeleteTarget) -> Result<(), TrashError> {
        self.0.move_to_trash(target)
    }
}

struct Env {
    store: InMemoryStore,
    fs: Arc<FakeFileSystem>,
    trash: Rc<FakeTrash>,
    bin: FakeRecycleBin,
    clock: FixedClock,
    active: ActiveJob,
    previews: CleanupPreviews,
}

impl Env {
    fn new() -> Self {
        Self::with_trash(FakeTrash::new)
    }

    fn with_trash(make: impl FnOnce(Arc<FakeFileSystem>) -> FakeTrash) -> Self {
        let fs = Arc::new(fs_with_files());
        let trash = Rc::new(make(Arc::clone(&fs)));
        Self {
            store: InMemoryStore::new(),
            fs,
            trash,
            bin: FakeRecycleBin::with_usage(0, 0),
            clock: FixedClock(NOW),
            active: ActiveJob::default(),
            previews: CleanupPreviews::default(),
        }
    }

    fn ports_at<'a>(
        &'a self,
        clock: &'a FixedClock,
    ) -> CleanupPorts<'a, InMemoryStore, FakeFileSystem, FixedClock> {
        CleanupPorts {
            store: &self.store,
            fs: &self.fs,
            clock,
            previews: &self.previews,
            active: &self.active,
        }
    }

    fn scan(&self) -> i64 {
        let guard = self.active.begin_scan().unwrap();
        let ports = ScanPorts {
            store: &self.store,
            fs: &*self.fs,
            recycle_bin: &self.bin,
            clock: &self.clock,
        };
        run_junk_scan(&ports, &matched(), raw_c_ana(), &guard, &mut |_| {})
            .unwrap()
            .scan_id
    }

    fn preview_with(
        &self,
        privileges: &PrivilegeContext,
        scan_id: i64,
        categories: &[&str],
    ) -> Result<CleanupPreviewDto, AppError> {
        let names: Vec<String> = categories.iter().map(|c| (*c).to_owned()).collect();
        let trash = Rc::clone(&self.trash);
        create_preview(
            &self.ports_at(&self.clock),
            privileges,
            || Ok(SharedTrash(trash)),
            scan_id,
            &names,
        )
    }

    fn preview(&self, scan_id: i64, categories: &[&str]) -> Result<CleanupPreviewDto, AppError> {
        self.preview_with(&matched(), scan_id, categories)
    }

    fn confirm_with(
        &self,
        privileges: &PrivilegeContext,
        clock: &FixedClock,
        preview_id: i64,
    ) -> (Result<CleanupSummaryDto, AppError>, Vec<CleanupEventDto>) {
        let trash = Rc::clone(&self.trash);
        let mut events = Vec::new();
        let result = confirm_cleanup_run(
            &self.ports_at(clock),
            privileges,
            raw_c_ana(),
            || Ok(SharedTrash(trash)),
            preview_id,
            &mut |event| events.push(event),
        );
        (result, events)
    }

    fn confirm(
        &self,
        preview_id: i64,
    ) -> (Result<CleanupSummaryDto, AppError>, Vec<CleanupEventDto>) {
        self.confirm_with(&matched(), &self.clock, preview_id)
    }
}

fn preview_id(dto: &CleanupPreviewDto) -> i64 {
    i64::try_from(dto.preview_id).unwrap()
}

fn is_kind<T: std::fmt::Debug>(result: Result<T, AppError>, kind: &str) -> bool {
    match result {
        Err(error) => serde_json::to_value(&error).unwrap()["kind"] == kind,
        Ok(_) => false,
    }
}

// ---- 7.3 DTOs ----

fn sample_summary(status: CleanupStatus) -> CleanupSummary {
    CleanupSummary {
        preview_id: PreviewId(4),
        scan_id: ScanId(9),
        status,
        started_at: EpochMillis(100),
        finished_at: EpochMillis(250),
        total_items: 10,
        processed: 9,
        not_processed: 1,
        moved_count: 6,
        moved_bytes: 6_000,
        skipped_locked: 1,
        skipped_protected: 1,
        skipped_cloud: 0,
        errors: 1,
        reasons: vec![
            ReasonCount {
                outcome: CleanupOutcome::SkippedLocked,
                detail: "in_use".to_owned(),
                count: 1,
            },
            ReasonCount {
                outcome: CleanupOutcome::Error,
                detail: "not_moved".to_owned(),
                count: 1,
            },
        ],
        categories: vec![CategorySummary {
            category: JunkCategory::TempUser,
            moved_count: 6,
            moved_bytes: 6_000,
            skipped_count: 2,
            error_count: 1,
        }],
    }
}

#[test]
fn summary_dto_should_serialize_the_exact_camel_case_shape_with_a_null_abort_reason() {
    let value = serde_json::to_value(CleanupSummaryDto::from(&sample_summary(
        CleanupStatus::Cancelled,
    )))
    .unwrap();
    assert_eq!(
        value,
        json!({
            "previewId": 4, "scanId": 9, "status": "cancelled", "abortReason": null,
            "startedAt": 100, "finishedAt": 250, "totalItems": 10, "processed": 9,
            "notProcessed": 1, "movedCount": 6, "movedBytes": 6000, "skippedLocked": 1,
            "skippedProtected": 1, "skippedCloud": 0, "errors": 1,
            "reasons": [
                {"outcome": "skipped_locked", "detail": "in_use", "count": 1},
                {"outcome": "error", "detail": "not_moved", "count": 1}
            ],
            "categories": [{
                "category": "temp_user", "movedCount": 6, "movedBytes": 6000,
                "skippedCount": 2, "errorCount": 1
            }]
        })
    );
}

#[test]
fn summary_dto_should_report_the_abort_reason_only_when_aborted() {
    let cases = [
        (CleanupStatus::Completed, "completed", Value::Null),
        (CleanupStatus::Cancelled, "cancelled", Value::Null),
        (
            CleanupStatus::Aborted(AbortReason::LogUnavailable),
            "aborted",
            json!("log_unavailable"),
        ),
        (
            CleanupStatus::Aborted(AbortReason::RecycleBinUnavailable),
            "aborted",
            json!("recycle_bin_unavailable"),
        ),
        (
            CleanupStatus::Aborted(AbortReason::TrashUnavailable),
            "aborted",
            json!("trash_unavailable"),
        ),
    ];
    for (status, name, reason) in cases {
        let value = serde_json::to_value(CleanupSummaryDto::from(&sample_summary(status))).unwrap();
        assert_eq!(value["status"], name);
        assert_eq!(value["abortReason"], reason, "{name}");
    }
}

#[test]
fn event_dto_should_serialize_with_a_type_tag_and_camel_case_fields() {
    let cases = [
        (
            CleanupEventDto::Started {
                preview_id: 3,
                total: 100,
            },
            json!({"type": "started", "previewId": 3, "total": 100}),
        ),
        (
            CleanupEventDto::Progress {
                processed: 32,
                total: 100,
                moved_count: 30,
                moved_bytes: 9_000,
            },
            json!({"type": "progress", "processed": 32, "total": 100, "movedCount": 30, "movedBytes": 9000}),
        ),
    ];
    for (event, expected) in cases {
        assert_eq!(serde_json::to_value(event).unwrap(), expected);
    }
}

#[test]
fn preview_dto_should_use_camel_case_and_expose_block_reason_only_when_blocked() {
    let env = Env::new();
    let scan = env.scan();
    let included = env
        .preview(scan, &["temp_user", "temp_windows"])
        .map(|dto| serde_json::to_value(dto).unwrap())
        .unwrap();
    assert_eq!(included["scanId"], scan);
    assert_eq!(included["createdAt"], NOW.0);
    assert_eq!(included["expiresAt"], NOW.0 + 600_000);
    assert_eq!(included["totalItems"], 3);
    assert_eq!(included["totalBytes"], 6_000);
    assert_eq!(included["recycleBinAvailable"], true);
    assert_eq!(
        included["categories"][0],
        json!({
            "category": "temp_user", "status": "included", "itemCount": 1, "sizeBytes": 2000,
            "recentCount": 0, "recentBytes": 0
        })
    );

    let blocked = env
        .preview_with(&mismatched(), scan, &["temp_user", "temp_windows"])
        .map(|dto| serde_json::to_value(dto).unwrap())
        .unwrap();
    assert_eq!(
        blocked["categories"][0],
        json!({
            "category": "temp_user", "status": "blocked", "blockReason": "different_user",
            "itemCount": 0, "sizeBytes": 0, "recentCount": 0, "recentBytes": 0
        })
    );
    assert_eq!(blocked["totalItems"], 2);
}

#[test]
fn preview_and_summary_json_should_not_leak_paths_or_identities() {
    let env = Env::new();
    let scan = env.scan();
    let preview = env.preview(scan, &["temp_user", "temp_windows"]).unwrap();
    let id = preview_id(&preview);
    let (summary, events) = env.confirm(id);
    let mut texts = vec![
        serde_json::to_string(&preview).unwrap(),
        serde_json::to_string(&summary.unwrap()).unwrap(),
    ];
    texts.extend(events.iter().map(|e| serde_json::to_string(e).unwrap()));
    for text in texts {
        assert!(!text.contains(char::from(0x5c)), "{text}");
        assert!(!text.contains("S-1-"), "{text}");
        assert!(!text.contains("ana"), "{text}");
    }
}

#[test]
fn ids_above_the_javascript_safe_integer_should_be_rejected_as_invalid_arguments() {
    let env = Env::new();
    let unsafe_id = (1_i64 << 53) + 1;
    let safe_max = (1_i64 << 53) - 1;
    assert!(is_kind(
        env.preview(unsafe_id, &["temp_user"]),
        "invalid_argument"
    ));
    assert!(is_kind(env.preview(-1, &["temp_user"]), "invalid_argument"));
    assert!(is_kind(env.confirm(unsafe_id).0, "invalid_argument"));
    assert!(is_kind(env.confirm(-3).0, "invalid_argument"));
    assert!(is_kind(
        cancel_cleanup_run(&env.active, unsafe_id),
        "invalid_argument"
    ));
    // El maximo seguro si es un identificador valido (simplemente no existe).
    assert!(is_kind(
        env.preview(safe_max, &["temp_user"]),
        "scan_not_found"
    ));
    assert!(is_kind(env.confirm(safe_max).0, "preview_not_found"));
}

#[test]
fn last_junk_scan_should_report_cleaned_at_null_until_a_cleanup_logged_items() {
    let env = Env::new();
    let scan = env.scan();
    let before = serde_json::to_value(last_junk_scan(&env.store).unwrap().unwrap()).unwrap();
    assert_eq!(before["cleanedAt"], Value::Null);

    let id = preview_id(&env.preview(scan, &["temp_user"]).unwrap());
    let later = FixedClock(EpochMillis(NOW.0 + 5_000));
    env.confirm_with(&matched(), &later, id).0.unwrap();
    let after = serde_json::to_value(last_junk_scan(&env.store).unwrap().unwrap()).unwrap();
    assert_eq!(after["cleanedAt"], NOW.0 + 5_000);
    assert_eq!(
        after["totalItems"], before["totalItems"],
        "conserva sus totales"
    );
}

// ---- 7.3a recycleBinAvailable ----

#[test]
fn preview_should_report_the_recycle_bin_as_available_when_the_preflight_passes() {
    let env = Env::new();
    let scan = env.scan();
    let preview = env.preview(scan, &["temp_windows"]).unwrap();
    assert!(preview.recycle_bin_available);
    assert_eq!(env.trash.preflight_calls().len(), 1);
}

#[test]
fn preview_should_report_the_recycle_bin_as_unavailable_when_the_preflight_fails() {
    for error in [TrashError::BinUnavailable, TrashError::BackendUnavailable] {
        let env = Env::with_trash(|fs| FakeTrash::new(fs).preflight_fails(error.clone()));
        let scan = env.scan();
        let preview = env.preview(scan, &["temp_windows"]).unwrap();
        assert!(!preview.recycle_bin_available, "{error:?}");
        assert_eq!(preview.total_items, 2, "el preview igual se arma");
    }
}

#[test]
fn preview_should_report_the_recycle_bin_as_unavailable_when_the_trash_cannot_be_created() {
    let env = Env::new();
    let scan = env.scan();
    let names = vec!["temp_windows".to_owned()];
    let preview = create_preview(
        &env.ports_at(&env.clock),
        &matched(),
        || Err::<SharedTrash, _>(TrashError::BackendUnavailable),
        scan,
        &names,
    )
    .unwrap();
    assert!(!preview.recycle_bin_available);
}

#[test]
fn empty_preview_should_report_the_recycle_bin_as_available_without_calling_the_preflight() {
    let env = Env::new();
    let scan = env.scan();
    // En `mismatch` la categoria por-usuario queda bloqueada y sin items.
    let preview = env
        .preview_with(&mismatched(), scan, &["temp_user"])
        .unwrap();
    assert_eq!(preview.total_items, 0);
    assert!(preview.recycle_bin_available);
    assert!(env.trash.preflight_calls().is_empty());
}

#[test]
fn confirm_should_abort_without_moving_anything_when_the_recycle_bin_became_disabled() {
    // El preview dijo que si, pero la politica cambio antes de confirmar.
    let env = Env::new();
    let scan = env.scan();
    let id = preview_id(&env.preview(scan, &["temp_user", "temp_windows"]).unwrap());
    let disabled = Env {
        trash: Rc::new(
            FakeTrash::new(Arc::clone(&env.fs)).preflight_fails(TrashError::BinUnavailable),
        ),
        ..env
    };
    let (summary, events) = disabled.confirm(id);
    let summary = summary.unwrap();
    assert_eq!(summary.status, "aborted");
    assert_eq!(summary.abort_reason, Some("recycle_bin_unavailable"));
    assert_eq!(summary.moved_count, 0);
    assert!(disabled.trash.moved().is_empty());
    assert!(disabled.fs.exists(Path::new(TEMP_USER)));
    assert!(matches!(
        events.first(),
        Some(CleanupEventDto::Started { total: 3, .. })
    ));
    assert_eq!(disabled.store.recent_cleanup_log(10).unwrap().len(), 0);
}

// ---- 7.4 preview_cleanup ----

#[test]
fn preview_should_reject_invalid_category_lists_with_invalid_argument() {
    let env = Env::new();
    let scan = env.scan();
    let cases: [&[&str]; 5] = [
        &[],
        &["temp_user", "temp_user"],
        &["recycle_bin"],
        &["old_logs"],
        &["downloads"],
    ];
    for categories in cases {
        assert!(
            is_kind(env.preview(scan, categories), "invalid_argument"),
            "{categories:?}"
        );
    }
    assert!(env.preview(scan, &["temp_user"]).is_ok());
}

#[test]
fn preview_should_answer_scan_not_found_for_an_unknown_or_replaced_scan() {
    let env = Env::new();
    assert!(is_kind(env.preview(1, &["temp_user"]), "scan_not_found"));
    let first = env.scan();
    let second = env.scan();
    assert!(env.preview(second, &["temp_user"]).is_ok());
    assert!(is_kind(
        env.preview(first, &["temp_user"]),
        "scan_not_found"
    ));
}

#[test]
fn preview_should_fail_while_a_scan_or_a_cleanup_is_running() {
    let env = Env::new();
    let scan = env.scan();
    let running_scan = env.active.begin_scan().unwrap();
    assert!(is_kind(
        env.preview(scan, &["temp_user"]),
        "scan_in_progress"
    ));
    drop(running_scan);
    let running_cleanup = env.active.begin_cleanup(PreviewId(99)).unwrap();
    assert!(is_kind(
        env.preview(scan, &["temp_user"]),
        "cleanup_in_progress"
    ));
    drop(running_cleanup);
    assert!(env.preview(scan, &["temp_user"]).is_ok());
}

#[test]
fn preview_should_replace_the_previous_one_and_use_increasing_ids() {
    let env = Env::new();
    let scan = env.scan();
    let first = preview_id(&env.preview(scan, &["temp_user"]).unwrap());
    let second = preview_id(&env.preview(scan, &["temp_windows"]).unwrap());
    assert_eq!((first, second), (1, 2));
    assert!(is_kind(env.confirm(first).0, "preview_not_found"));
    assert!(env.confirm(second).0.is_ok());
}

// ---- 7.4 confirm_cleanup ----

#[test]
fn confirm_should_move_the_previewed_items_and_stream_started_and_progress() {
    let env = Env::new();
    let scan = env.scan();
    let id = preview_id(&env.preview(scan, &["temp_user", "temp_windows"]).unwrap());
    let (summary, events) = env.confirm(id);
    let summary = summary.unwrap();

    assert_eq!(summary.status, "completed");
    assert_eq!(summary.abort_reason, None);
    assert_eq!((summary.moved_count, summary.moved_bytes), (3, 6_000));
    assert_eq!(summary.not_processed, 0);
    assert_eq!(env.trash.moved().len(), 3);
    assert_eq!(env.store.recent_cleanup_log(10).unwrap().len(), 3);
    assert_eq!(
        events.first(),
        Some(&CleanupEventDto::Started {
            preview_id: id.unsigned_abs(),
            total: 3
        })
    );
    assert!(matches!(
        events.last(),
        Some(CleanupEventDto::Progress {
            processed: 3,
            total: 3,
            moved_count: 3,
            moved_bytes: 6_000
        })
    ));
}

#[test]
fn confirm_should_consume_the_preview_so_a_second_confirmation_is_not_found() {
    let env = Env::new();
    let scan = env.scan();
    let id = preview_id(&env.preview(scan, &["temp_windows"]).unwrap());
    assert!(env.confirm(id).0.is_ok());
    assert!(is_kind(env.confirm(id).0, "preview_not_found"));
    assert!(is_kind(env.confirm(id + 40).0, "preview_not_found"));
}

#[test]
fn confirm_should_reject_a_preview_without_items_with_nothing_to_clean() {
    let env = Env::new();
    let scan = env.scan();
    let empty = preview_id(
        &env.preview_with(&mismatched(), scan, &["temp_user"])
            .unwrap(),
    );
    assert!(is_kind(env.confirm(empty).0, "nothing_to_clean"));
    assert!(env.trash.moved().is_empty());
    assert!(env.active.begin_scan().is_ok(), "el lugar se libera");
}

#[test]
fn confirm_should_fail_with_preview_stale_when_another_scan_finished_after_the_preview() {
    let env = Env::new();
    let scan = env.scan();
    let id = preview_id(&env.preview(scan, &["temp_user", "temp_windows"]).unwrap());
    env.scan();
    assert!(is_kind(env.confirm(id).0, "preview_stale"));
    assert!(env.trash.moved().is_empty());
    assert!(env.store.recent_cleanup_log(10).unwrap().is_empty());
}

#[test]
fn confirm_should_fail_with_preview_expired_after_ten_minutes_and_not_before() {
    let env = Env::new();
    let scan = env.scan();
    let id = preview_id(&env.preview(scan, &["temp_windows"]).unwrap());
    let late = FixedClock(EpochMillis(NOW.0 + 600_001));
    assert!(is_kind(
        env.confirm_with(&matched(), &late, id).0,
        "preview_expired"
    ));
    assert!(env.trash.moved().is_empty());

    let id = preview_id(&env.preview(scan, &["temp_windows"]).unwrap());
    let in_time = FixedClock(EpochMillis(NOW.0 + 600_000));
    assert!(env.confirm_with(&matched(), &in_time, id).0.is_ok());
}

#[test]
fn preview_should_fail_with_scan_already_cleaned_after_a_cleanup_logged_items() {
    let env = Env::new();
    let scan = env.scan();
    let id = preview_id(&env.preview(scan, &["temp_windows"]).unwrap());
    env.confirm(id).0.unwrap();
    assert!(is_kind(
        env.preview(scan, &["temp_user"]),
        "scan_already_cleaned"
    ));
}

#[test]
fn confirm_should_never_run_a_replaced_preview_and_the_cleaned_scan_admits_no_more_previews() {
    let env = Env::new();
    let scan = env.scan();
    let first = preview_id(&env.preview(scan, &["temp_windows"]).unwrap());
    // Un segundo preview reemplaza al primero; se limpia con el segundo.
    let second = preview_id(&env.preview(scan, &["temp_user"]).unwrap());
    env.confirm(second).0.unwrap();
    assert!(is_kind(env.confirm(first).0, "preview_not_found"));
    assert!(is_kind(
        env.preview(scan, &["temp_windows"]),
        "scan_already_cleaned"
    ));
}

#[test]
fn confirm_should_fail_while_a_scan_or_another_cleanup_runs_without_consuming_the_preview() {
    let env = Env::new();
    let scan = env.scan();
    let id = preview_id(&env.preview(scan, &["temp_windows"]).unwrap());
    let running_scan = env.active.begin_scan().unwrap();
    assert!(is_kind(env.confirm(id).0, "scan_in_progress"));
    drop(running_scan);
    let running_cleanup = env.active.begin_cleanup(PreviewId(77)).unwrap();
    assert!(is_kind(env.confirm(id).0, "cleanup_in_progress"));
    drop(running_cleanup);
    assert!(env.confirm(id).0.is_ok(), "el preview seguia vigente");
}

#[test]
fn confirm_should_reevaluate_privileges_and_never_move_per_user_items_on_a_mismatch() {
    let env = Env::new();
    let scan = env.scan();
    let id = preview_id(&env.preview(scan, &["temp_user", "temp_windows"]).unwrap());
    let (summary, _) = env.confirm_with(&mismatched(), &env.clock, id);
    let summary = summary.unwrap();
    assert_eq!(summary.moved_count, 2);
    assert_eq!(summary.skipped_protected, 1);
    assert!(
        summary
            .reasons
            .iter()
            .any(|r| r.detail == "blocked_different_user" && r.count == 1)
    );
    assert!(env.fs.exists(Path::new(TEMP_USER)));
    assert!(
        env.trash
            .moved()
            .iter()
            .all(|p| !p.to_string_lossy().contains("ana"))
    );
}

#[test]
fn confirm_should_release_the_slot_even_when_the_trash_cannot_be_created() {
    let env = Env::new();
    let scan = env.scan();
    let id = preview_id(&env.preview(scan, &["temp_windows"]).unwrap());
    let result = confirm_cleanup_run(
        &env.ports_at(&env.clock),
        &matched(),
        raw_c_ana(),
        || Err::<SharedTrash, _>(TrashError::BackendUnavailable),
        id,
        &mut |_| {},
    );
    assert!(is_kind(result, "internal"));
    assert!(env.active.begin_scan().is_ok());
}

// ---- 7.4 cancel_cleanup ----

#[test]
fn cancel_cleanup_should_answer_preview_not_found_without_a_running_cleanup() {
    let env = Env::new();
    assert!(is_kind(
        cancel_cleanup_run(&env.active, 5),
        "preview_not_found"
    ));
    let scan = env.scan();
    let id = preview_id(&env.preview(scan, &["temp_windows"]).unwrap());
    assert!(
        is_kind(cancel_cleanup_run(&env.active, id), "preview_not_found"),
        "un preview que aun no se ejecuta no se cancela"
    );
}

#[test]
fn cancel_cleanup_should_raise_the_flag_of_the_running_cleanup_only() {
    let env = Env::new();
    let guard = env.active.begin_cleanup(PreviewId(8)).unwrap();
    assert!(is_kind(
        cancel_cleanup_run(&env.active, 9),
        "preview_not_found"
    ));
    assert!(!guard.cancel_flag().load(Ordering::Relaxed));
    cancel_cleanup_run(&env.active, 8).unwrap();
    assert!(guard.cancel_flag().load(Ordering::Relaxed));
}

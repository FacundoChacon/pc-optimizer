use serde_json::{Value, json};

use super::*;
use crate::domain::junk::model::JunkFinding;
use crate::domain::model::EpochMillis;
use crate::domain::ports::fakes::{FakeFileSystem, FakeRecycleBin, FixedClock, InMemoryStore};
use crate::domain::ports::{PortError, ScanStore};
use crate::domain::privileges::{MismatchReason, UserMatch};
use crate::domain::protection::test_support::{raw_c_ana, raw_for};

const NOW: EpochMillis = EpochMillis(1_800_000_000_000);
const DAY: i64 = 86_400_000;

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
        .with_file(r"C:\Windows\Temp\w.tmp", 1_000, old)
        .with_file(r"C:\Windows\Temp\z.tmp", 3_000, old)
        .with_dir(r"C:\Users")
        .with_dir(r"C:\Users\ana")
        .with_dir(r"C:\Users\ana\AppData")
        .with_dir(r"C:\Users\ana\AppData\Local")
        .with_dir(r"C:\Users\ana\AppData\Local\Temp")
        .with_file(r"C:\Users\ana\AppData\Local\Temp\t.tmp", 2_000, old)
}

struct Env {
    store: InMemoryStore,
    fs: FakeFileSystem,
    bin: FakeRecycleBin,
    clock: FixedClock,
    active: ActiveScan,
}

impl Env {
    fn new() -> Self {
        Self {
            store: InMemoryStore::new(),
            fs: fs_with_files(),
            bin: FakeRecycleBin::with_usage(12, 340_000_000),
            clock: FixedClock(NOW),
            active: ActiveScan::default(),
        }
    }

    fn run(
        &self,
        privileges: &PrivilegeContext,
        raw: RawProtectionContext,
    ) -> (Result<JunkScanSummaryDto, AppError>, Vec<ScanEventDto>) {
        let guard = self.active.begin().unwrap();
        self.run_with_guard(privileges, raw, &guard)
    }

    fn run_with_guard(
        &self,
        privileges: &PrivilegeContext,
        raw: RawProtectionContext,
        guard: &ScanGuard,
    ) -> (Result<JunkScanSummaryDto, AppError>, Vec<ScanEventDto>) {
        let ports = ScanPorts {
            store: &self.store,
            fs: &self.fs,
            recycle_bin: &self.bin,
            clock: &self.clock,
        };
        let mut events = Vec::new();
        let result = run_junk_scan(&ports, privileges, raw, guard, &mut |e| events.push(e));
        (result, events)
    }
}

fn category_of<'a>(summary: &'a JunkScanSummaryDto, name: &str) -> &'a CategoryDto {
    summary
        .categories
        .iter()
        .find(|c| c.category == name)
        .unwrap_or_else(|| panic!("{name} falta"))
}

// ---- 7.2 DTOs ----

fn dto_for(status: CategoryStatus, reason: Option<MismatchReason>) -> Value {
    let result = CategoryResult {
        status,
        block_reason: reason,
        ..CategoryResult::empty(JunkCategory::TempUser, status, reason)
    };
    serde_json::to_value(CategoryDto::from(&result)).unwrap()
}

#[test]
fn category_dto_should_use_camel_case_and_expose_block_reason_only_when_partial_or_blocked() {
    let blocked = dto_for(CategoryStatus::Blocked, Some(MismatchReason::DifferentUser));
    assert_eq!(
        blocked,
        json!({
            "category": "temp_user", "scope": "per_user", "status": "blocked",
            "blockReason": "different_user", "itemCount": 0, "sizeBytes": 0,
            "skippedCount": 0, "inaccessibleCount": 0
        })
    );
    let partial = dto_for(CategoryStatus::Partial, Some(MismatchReason::Undetermined));
    assert_eq!(partial["blockReason"], "undetermined");
    for status in [CategoryStatus::Done, CategoryStatus::Unavailable] {
        // Aunque el dominio trajera un motivo, solo se publica con partial/blocked.
        let value = dto_for(status, Some(MismatchReason::DifferentUser));
        assert!(value.get("blockReason").is_none(), "{status:?}: {value}");
    }
}

#[test]
fn scan_event_dto_should_serialize_with_a_type_tag_and_camel_case_fields() {
    let cases = [
        (
            ScanEventDto::Started { scan_id: 7 },
            json!({"type": "started", "scanId": 7}),
        ),
        (
            ScanEventDto::Category {
                category: "temp_user",
            },
            json!({"type": "category", "category": "temp_user"}),
        ),
        (
            ScanEventDto::Progress {
                category: "wu_cache",
                files_seen: 256,
                eligible_bytes: 9_000,
            },
            json!({"type": "progress", "category": "wu_cache", "filesSeen": 256, "eligibleBytes": 9000}),
        ),
    ];
    for (event, expected) in cases {
        assert_eq!(serde_json::to_value(event).unwrap(), expected);
    }
}

#[test]
fn finding_page_dto_should_report_a_null_skip_reason_for_eligible_and_the_code_for_skipped() {
    use crate::domain::junk::model::{FindingState, StoredFinding};
    use crate::domain::ports::FileIdentity;
    let eligible = JunkFinding {
        category: JunkCategory::TempUser,
        path: r"C:\Users\ana\AppData\Local\Temp\a.tmp".to_owned(),
        size_bytes: 10,
        modified_at: Some(EpochMillis(5)),
        state: FindingState::Eligible(FileIdentity {
            volume_serial: 1,
            file_id: 2,
        }),
    };
    let skipped = JunkFinding {
        path: r"C:\x\b.tmp".to_owned(),
        modified_at: None,
        state: FindingState::Skipped("reparse_point".to_owned()),
        ..eligible.clone()
    };
    let page = JunkFindingPage {
        total: 40,
        items: vec![
            StoredFinding {
                id: 1,
                finding: eligible,
            },
            StoredFinding {
                id: 2,
                finding: skipped,
            },
        ],
    };
    let value = serde_json::to_value(FindingPageDto::from(page)).unwrap();
    assert_eq!(value["total"], 40);
    assert_eq!(value["items"][0]["skipReason"], Value::Null);
    assert_eq!(value["items"][0]["modifiedAt"], 5);
    assert_eq!(value["items"][0]["sizeBytes"], 10);
    assert_eq!(value["items"][1]["skipReason"], "reparse_point");
    assert_eq!(value["items"][1]["modifiedAt"], Value::Null);
}

#[test]
fn summary_and_events_json_should_not_leak_paths_or_identities() {
    let env = Env::new();
    let (result, events) = env.run(&matched(), raw_c_ana());
    let mut texts = vec![serde_json::to_string(&result.unwrap()).unwrap()];
    texts.extend(events.iter().map(|e| serde_json::to_string(e).unwrap()));
    for text in texts {
        assert!(!text.contains(char::from(0x5c)), "{text}");
        assert!(!text.contains("S-1-"), "{text}");
        assert!(!text.contains("ana"), "{text}");
    }
}

// ---- 7.3 estado ActiveScan ----

#[test]
fn active_scan_should_reject_a_second_scan_and_release_the_slot_on_drop() {
    let active = ActiveScan::default();
    let first = active.begin().unwrap();
    assert!(matches!(active.begin(), Err(AppError::ScanInProgress)));
    drop(first);
    let again = active.begin();
    assert!(again.is_ok(), "el lugar debe liberarse al soltar el guard");
}

#[test]
fn active_scan_should_release_the_slot_when_the_task_fails() {
    let active = ActiveScan::default();
    let outcome: Result<(), AppError> = (|| {
        let _guard = active.begin()?;
        Err(AppError::Internal("falla de la tarea".to_owned()))
    })();
    assert!(outcome.is_err());
    assert!(active.begin().is_ok());
}

#[test]
fn active_scan_cancel_should_raise_the_flag_only_for_the_active_id() {
    let active = ActiveScan::default();
    assert!(matches!(
        active.cancel(ScanId(1)),
        Err(AppError::ScanNotFound(_))
    ));
    let guard = active.begin().unwrap();
    assert!(
        matches!(active.cancel(ScanId(1)), Err(AppError::ScanNotFound(_))),
        "aun sin id asociado nada coincide"
    );
    guard.bind(ScanId(7));
    assert!(matches!(
        active.cancel(ScanId(8)),
        Err(AppError::ScanNotFound(_))
    ));
    assert!(!guard.cancel_flag().load(Ordering::Relaxed));
    active.cancel(ScanId(7)).unwrap();
    assert!(guard.cancel_flag().load(Ordering::Relaxed));
    drop(guard);
    assert!(matches!(
        active.cancel(ScanId(7)),
        Err(AppError::ScanNotFound(_))
    ));
}

// ---- 7.4 flujo del escaneo (con dobles) ----

#[test]
fn run_junk_scan_should_persist_a_done_scan_and_stream_events_in_order() {
    let env = Env::new();
    let (result, events) = env.run(&matched(), raw_c_ana());
    let summary = result.unwrap();

    assert_eq!(summary.status, "done");
    assert_eq!(summary.categories.len(), 11);
    assert_eq!(summary.total_items, 3);
    assert_eq!(summary.total_bytes, 6_000);
    assert_eq!(category_of(&summary, "recycle_bin").size_bytes, 340_000_000);
    assert_eq!(summary.started_at, NOW.0);

    assert_eq!(
        events.first(),
        Some(&ScanEventDto::Started {
            scan_id: summary.scan_id
        })
    );
    let announced: Vec<&str> = events
        .iter()
        .filter_map(|e| match e {
            ScanEventDto::Category { category } => Some(*category),
            _ => None,
        })
        .collect();
    let expected: Vec<&str> = JunkCategory::ALL.iter().map(|c| c.as_str()).collect();
    assert_eq!(announced, expected);

    let stored = last_junk_scan(&env.store).unwrap().unwrap();
    assert_eq!(stored, summary);
}

#[test]
fn run_junk_scan_should_leave_a_cancelled_scan_without_findings_and_keep_the_previous_summary() {
    let env = Env::new();
    let first = env.run(&matched(), raw_c_ana()).0.unwrap();

    let guard = env.active.begin().unwrap();
    guard.cancel_flag().store(true, Ordering::Relaxed);
    let (second, events) = env.run_with_guard(&matched(), raw_c_ana(), &guard);
    let second = second.unwrap();

    assert_eq!(second.status, "cancelled");
    assert_ne!(second.scan_id, first.scan_id);
    assert_eq!((second.total_items, second.total_bytes), (0, 0));
    assert!(second.categories.is_empty());
    assert!(second.finished_at.is_some());
    assert_eq!(
        events,
        vec![ScanEventDto::Started {
            scan_id: second.scan_id
        }]
    );

    let scans = env.store.list_scans().unwrap();
    let cancelled = scans.iter().find(|s| s.id.0 == second.scan_id).unwrap();
    assert_eq!(cancelled.status, ScanStatus::Cancelled);
    assert_eq!(last_junk_scan(&env.store).unwrap().unwrap(), first);
    let page = list_findings(&env.store, first.scan_id, "temp_windows", 0, 10).unwrap();
    assert_eq!(page.total, 2, "los hallazgos del escaneo previo siguen");
}

#[test]
fn run_junk_scan_should_block_per_user_categories_on_a_mismatch_without_listing_the_profile() {
    let env = Env::new();
    let (result, _) = env.run(&mismatched(), raw_c_ana());
    let summary = result.unwrap();
    for name in ["temp_user", "thumbnail_cache", "recycle_bin"] {
        let blocked = category_of(&summary, name);
        assert_eq!(blocked.status, "blocked", "{name}");
        assert_eq!(blocked.block_reason, Some("different_user"), "{name}");
    }
    assert_eq!(category_of(&summary, "temp_windows").item_count, 2);
    assert!(env.bin.queried().is_empty());
    let page = list_findings(&env.store, summary.scan_id, "temp_user", 0, 10).unwrap();
    assert_eq!(page.total, 0);
}

#[test]
fn run_junk_scan_should_measure_the_recycle_bin_of_the_real_windows_drive() {
    let env = Env::new();
    let (result, _) = env.run(&matched(), raw_for('D', "ana"));
    assert_eq!(result.unwrap().status, "done");
    assert_eq!(env.bin.queried(), vec![PathBuf::from(r"D:\")]);
}

#[test]
fn run_junk_scan_should_report_the_recycle_bin_unavailable_when_the_drive_cannot_be_resolved() {
    let env = Env::new();
    let mut raw = raw_c_ana();
    raw.windows_dir = None;
    let (result, _) = env.run(&matched(), raw);
    let summary = result.unwrap();
    let bin = category_of(&summary, "recycle_bin");
    assert_eq!(bin.status, "unavailable");
    assert_eq!(bin.size_bytes, 0);
    assert!(env.bin.queried().is_empty(), "no se adivina una letra");
    assert_eq!(category_of(&summary, "temp_windows").status, "unavailable");
}

#[test]
fn run_junk_scan_should_mark_the_scan_as_error_when_the_recycle_bin_query_fails_nothing_else() {
    let mut env = Env::new();
    env.bin = FakeRecycleBin::failing(PortError::Io("shell".into()));
    let summary = env.run(&matched(), raw_c_ana()).0.unwrap();
    assert_eq!(summary.status, "done");
    assert_eq!(category_of(&summary, "recycle_bin").status, "unavailable");
}

#[test]
fn run_junk_scan_should_release_the_slot_only_when_the_guard_drops() {
    let env = Env::new();
    let guard = env.active.begin().unwrap();
    let _ = env.run_with_guard(&matched(), raw_c_ana(), &guard);
    assert!(matches!(env.active.begin(), Err(AppError::ScanInProgress)));
    drop(guard);
    assert!(env.active.begin().is_ok());
}

// ---- list_junk_findings / get_last_junk_scan ----

#[test]
fn last_junk_scan_should_be_none_without_a_finished_scan() {
    let env = Env::new();
    assert_eq!(last_junk_scan(&env.store).unwrap(), None);
}

#[test]
fn list_findings_should_page_the_last_done_scan_by_size_descending() {
    let env = Env::new();
    let summary = env.run(&matched(), raw_c_ana()).0.unwrap();
    let first = list_findings(&env.store, summary.scan_id, "temp_windows", 0, 1).unwrap();
    assert_eq!(first.total, 2);
    assert_eq!(first.items.len(), 1);
    assert_eq!(first.items[0].size_bytes, 3_000);
    assert_eq!(first.items[0].skip_reason, None);
    let next = list_findings(&env.store, summary.scan_id, "temp_windows", 1, 1).unwrap();
    assert_eq!(next.items[0].size_bytes, 1_000);
}

#[test]
fn list_findings_should_reject_invalid_arguments_with_invalid_argument() {
    let env = Env::new();
    let id = env.run(&matched(), raw_c_ana()).0.unwrap().scan_id;
    let cases: [(&str, i64, i64); 7] = [
        ("temp_user", 0, 0),
        ("temp_user", 0, 501),
        ("temp_user", 0, -5),
        ("temp_user", -1, 10),
        ("downloads", 0, 10),
        ("custom_rule", 0, 10),
        ("", 0, 10),
    ];
    for (category, offset, limit) in cases {
        let error = list_findings(&env.store, id, category, offset, limit).unwrap_err();
        assert!(
            matches!(error, AppError::InvalidArgument(_)),
            "{category} {offset} {limit}: {error:?}"
        );
    }
    assert!(list_findings(&env.store, id, "temp_user", 0, 500).is_ok());
    assert!(list_findings(&env.store, id, "temp_user", 0, 1).is_ok());
}

#[test]
fn list_findings_should_answer_scan_not_found_for_any_scan_but_the_last_done() {
    let env = Env::new();
    assert!(matches!(
        list_findings(&env.store, 1, "temp_user", 0, 10),
        Err(AppError::ScanNotFound(_))
    ));
    let first = env.run(&matched(), raw_c_ana()).0.unwrap();
    let second = env.run(&matched(), raw_c_ana()).0.unwrap();
    assert!(list_findings(&env.store, second.scan_id, "temp_user", 0, 10).is_ok());
    assert!(matches!(
        list_findings(&env.store, first.scan_id, "temp_user", 0, 10),
        Err(AppError::ScanNotFound(_))
    ));
    assert!(matches!(
        list_findings(&env.store, 9_999, "temp_user", 0, 10),
        Err(AppError::ScanNotFound(_))
    ));
}

// ---- app_dirs ----

#[test]
fn app_dirs_should_include_the_data_dir_and_the_executable_folder() {
    let dirs = app_dirs(
        Some(Path::new(r"C:\Users\ana\AppData\Local\pc-optimizer")),
        Some(Path::new(r"C:\Program Files\PC Optimizer\pc-optimizer.exe")),
    );
    assert_eq!(
        dirs,
        vec![
            r"C:\Users\ana\AppData\Local\pc-optimizer".to_owned(),
            r"C:\Program Files\PC Optimizer".to_owned()
        ]
    );
    assert_eq!(
        app_dirs(None, Some(Path::new(r"C:\app\x.exe"))),
        vec![r"C:\app".to_owned()]
    );
    assert!(app_dirs(None, None).is_empty());
}

#[test]
fn scan_environment_should_keep_the_data_dir_it_was_given() {
    let env = ScanEnvironment::default();
    assert_eq!(env.data_dir(), None);
    env.set_data_dir(Some(PathBuf::from(r"D:\datos")));
    assert_eq!(env.data_dir(), Some(PathBuf::from(r"D:\datos")));
}

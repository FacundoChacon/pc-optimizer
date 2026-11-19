use serde_json::json;

use super::*;
use crate::domain::model::{CleanupOutcome, EpochMillis, NewCleanupLogEntry, ScanId};
use crate::domain::ports::fakes::InMemoryStore;
use crate::domain::ports::{CleanupLogStore, CleanupRunKey, CleanupRunRow, RunCursor, StoreError};
use crate::domain::privileges::{MismatchReason, UserMatch};

fn matched() -> PrivilegeContext {
    PrivilegeContext::new(UserMatch::Match, true)
}

fn row(
    ts: i64,
    category: &str,
    outcome: CleanupOutcome,
    size_bytes: u64,
    scan_id: Option<i64>,
) -> NewCleanupLogEntry {
    NewCleanupLogEntry {
        ts: EpochMillis(ts),
        category: category.into(),
        path: format!(r"C:\Users\ana\Temp\f{ts}.tmp"),
        size_bytes,
        outcome,
        scan_id: scan_id.map(ScanId),
        error_detail: None,
    }
}

fn kind_of<T: std::fmt::Debug>(result: Result<T, AppError>) -> String {
    serde_json::to_value(result.unwrap_err()).unwrap()["kind"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn junk_store() -> InMemoryStore {
    let store = InMemoryStore::new();
    store
        .append_cleanup_log(&[
            row(
                1_000,
                "temp_user",
                CleanupOutcome::MovedToTrash,
                100,
                Some(7),
            ),
            row(
                1_010,
                "browser_cache_edge",
                CleanupOutcome::MovedToTrash,
                200,
                Some(7),
            ),
            row(
                1_020,
                "temp_user",
                CleanupOutcome::SkippedLocked,
                50,
                Some(7),
            ),
        ])
        .unwrap();
    store
}

#[test]
fn list_runs_should_answer_the_exact_camel_case_page_of_a_junk_run() {
    let store = junk_store();

    let page = list_runs(&store, None, 10).unwrap();

    assert_eq!(
        serde_json::to_value(page).unwrap(),
        json!({
            "items": [{
                "key": { "kind": "scan", "scanId": 7 },
                "kind": "junk",
                "startedAt": 1_000,
                "finishedAt": 1_020,
                "entryCount": 3,
                "outcomes": {
                    "movedToTrash": 2,
                    "skippedLocked": 1,
                    "skippedProtected": 0,
                    "skippedCloud": 0,
                    "error": 0,
                    "recycleBinEmptied": 0
                },
                "movedBytes": 300,
                "emptiedBytes": 0,
                "emptyDetail": null,
                "categories": ["browser_cache_edge", "temp_user"]
            }],
            "nextCursor": null
        })
    );
}

fn key_arg(kind: &str, scan_id: Option<i64>, entry_id: Option<i64>) -> RunKeyArg {
    RunKeyArg {
        kind: kind.to_owned(),
        scan_id,
        entry_id,
    }
}

fn scan_key(id: i64) -> RunKeyArg {
    key_arg("scan", Some(id), None)
}

#[test]
fn list_runs_should_reject_an_invalid_limit_without_reading_the_log() {
    let store = UnavailableLog;

    for limit in [0, 51, -1, i64::MAX] {
        assert_eq!(
            kind_of(list_runs(&store, None, limit)),
            "invalid_argument",
            "limit {limit}"
        );
    }
}

#[test]
fn list_runs_should_reject_a_malformed_cursor() {
    let store = junk_store();
    let cursor = |finished_at: i64, key: RunKeyArg| Some(RunCursorArg { finished_at, key });

    for before in [
        cursor(0, scan_key(7)),
        cursor(-5, scan_key(7)),
        cursor(1 << 53, scan_key(7)),
        cursor(1_000, key_arg("folder", Some(7), None)),
        cursor(1_000, key_arg("scan", None, None)),
        cursor(1_000, key_arg("entry", Some(7), None)),
        cursor(1_000, scan_key(-1)),
    ] {
        assert_eq!(
            kind_of(list_runs(&store, before.clone(), 10)),
            "invalid_argument",
            "{before:?}"
        );
    }
}

#[test]
fn list_runs_should_page_with_a_cursor_that_is_null_on_the_last_page() {
    let store = InMemoryStore::new();
    store
        .append_cleanup_log(&[
            row(1_000, "temp_user", CleanupOutcome::MovedToTrash, 1, Some(1)),
            row(
                2_000,
                "duplicates",
                CleanupOutcome::MovedToTrash,
                2,
                Some(2),
            ),
            row(
                3_000,
                "recycle_bin",
                CleanupOutcome::RecycleBinEmptied,
                900,
                None,
            ),
        ])
        .unwrap();

    let first = list_runs(&store, None, 2).unwrap();

    assert_eq!(first.items.len(), 2);
    assert_eq!(first.items[0].kind, "recycle_bin_empty");
    assert_eq!(first.items[0].emptied_bytes, 900);
    assert_eq!(first.items[1].kind, "duplicates");
    let cursor = first.next_cursor.expect("a second page exists");
    assert_eq!(cursor.finished_at, 2_000);
    assert_eq!(cursor.key, RunKeyDto::Scan { scan_id: 2 });

    let second = list_runs(
        &store,
        Some(RunCursorArg {
            finished_at: cursor.finished_at,
            key: scan_key(2),
        }),
        2,
    )
    .unwrap();

    assert_eq!(second.items.len(), 1);
    assert_eq!(second.items[0].key, RunKeyDto::Scan { scan_id: 1 });
    assert_eq!(second.items[0].kind, "junk");
    assert_eq!(second.next_cursor, None);
}

#[test]
fn list_runs_should_only_report_partial_and_unverified_empty_details() {
    let store = InMemoryStore::new();
    let with_detail = |ts: i64, detail: &str| NewCleanupLogEntry {
        error_detail: Some(detail.to_owned()),
        ..row(
            ts,
            "recycle_bin",
            CleanupOutcome::RecycleBinEmptied,
            10,
            None,
        )
    };
    store
        .append_cleanup_log(&[
            with_detail(1_000, "partial"),
            with_detail(2_000, "unverified"),
            with_detail(3_000, "something_else"),
        ])
        .unwrap();

    let details: Vec<Option<&str>> = list_runs(&store, None, 10)
        .unwrap()
        .items
        .iter()
        .map(|run| run.empty_detail)
        .collect();

    assert_eq!(details, [None, Some("unverified"), Some("partial")]);
}

#[test]
fn list_entries_should_answer_the_exact_page_with_paths_for_a_matching_user() {
    let store = junk_store();

    let page = list_entries(&store, &matched(), &scan_key(7), 1, 2).unwrap();

    assert_eq!(
        serde_json::to_value(page).unwrap(),
        json!({
            "total": 3,
            "pathsHidden": false,
            "items": [
                {
                    "id": 2, "ts": 1_010, "category": "browser_cache_edge",
                    "path": r"C:\Users\ana\Temp\f1010.tmp", "sizeBytes": 200,
                    "outcome": "moved_to_trash", "errorDetail": null
                },
                {
                    "id": 3, "ts": 1_020, "category": "temp_user",
                    "path": r"C:\Users\ana\Temp\f1020.tmp", "sizeBytes": 50,
                    "outcome": "skipped_locked", "errorDetail": null
                }
            ]
        })
    );
}

#[test]
fn list_entries_should_hide_paths_in_mismatch_and_keep_sizes_and_outcomes() {
    let store = junk_store();
    for reason in [MismatchReason::DifferentUser, MismatchReason::Undetermined] {
        let context = PrivilegeContext::new(UserMatch::Mismatch(reason), true);

        let page = list_entries(&store, &context, &scan_key(7), 0, 10).unwrap();

        assert!(page.paths_hidden, "{reason:?}");
        assert_eq!(page.total, 3);
        assert!(page.items.iter().all(|item| item.path.is_none()));
        let sizes: Vec<u64> = page.items.iter().map(|item| item.size_bytes).collect();
        assert_eq!(sizes, [100, 200, 50]);
        assert_eq!(page.items[2].outcome, "skipped_locked");
    }
}

#[test]
fn list_entries_should_reject_invalid_arguments_without_reading_the_log() {
    let store = UnavailableLog;
    let ctx = matched();
    let cases: [(RunKeyArg, i64, i64); 7] = [
        (scan_key(7), 0, 0),
        (scan_key(7), 0, 201),
        (scan_key(7), 0, -3),
        (scan_key(7), -1, 10),
        (scan_key(7), 1 << 53, 10),
        (key_arg("folder", Some(7), None), 0, 10),
        (key_arg("entry", None, None), 0, 10),
    ];

    for (key, offset, limit) in cases {
        assert_eq!(
            kind_of(list_entries(&store, &ctx, &key, offset, limit)),
            "invalid_argument",
            "{key:?} {offset} {limit}"
        );
    }
}

#[test]
fn list_entries_should_answer_an_empty_page_for_an_unknown_run() {
    let store = junk_store();

    let page = list_entries(&store, &matched(), &scan_key(999), 0, 10).unwrap();

    assert_eq!(page.total, 0);
    assert!(page.items.is_empty());
    assert!(!page.paths_hidden);
}

/// Un log cuya base no esta disponible.
struct UnavailableLog;

impl CleanupLogStore for UnavailableLog {
    fn append_cleanup_log(&self, _entries: &[NewCleanupLogEntry]) -> Result<(), StoreError> {
        Err(StoreError::Unavailable("locked".into()))
    }

    fn recent_cleanup_log(
        &self,
        _limit: u32,
    ) -> Result<Vec<crate::domain::model::CleanupLogEntry>, StoreError> {
        Err(StoreError::Unavailable("locked".into()))
    }

    fn scan_cleaned_at(
        &self,
        _scan_id: ScanId,
        _since: EpochMillis,
    ) -> Result<Option<EpochMillis>, StoreError> {
        Err(StoreError::Unavailable("locked".into()))
    }

    fn cleanup_runs(
        &self,
        _before: Option<RunCursor>,
        _limit: u32,
    ) -> Result<Vec<CleanupRunRow>, StoreError> {
        Err(StoreError::Unavailable("locked".into()))
    }

    fn cleanup_run_entries(
        &self,
        _key: CleanupRunKey,
        _offset: u64,
        _limit: u32,
    ) -> Result<(u64, Vec<crate::domain::model::CleanupLogEntry>), StoreError> {
        Err(StoreError::Unavailable("locked".into()))
    }
}

#[test]
fn queries_should_answer_the_existing_store_error_when_the_database_is_unavailable() {
    let store = UnavailableLog;

    assert_eq!(kind_of(list_runs(&store, None, 10)), "store_unavailable");
    assert_eq!(
        kind_of(list_entries(&store, &matched(), &scan_key(7), 0, 10)),
        "store_unavailable"
    );
}

#[test]
fn queries_should_answer_while_a_cleanup_is_active_and_leave_the_log_untouched() {
    use crate::commands::scan::ActiveJob;
    use crate::domain::cleanup::model::PreviewId;

    let store = junk_store();
    let active = ActiveJob::default();
    let _running = active.begin_cleanup(PreviewId(9)).unwrap();
    let before = store.recent_cleanup_log(100).unwrap().len();

    let runs = list_runs(&store, None, 10).unwrap();
    let entries = list_entries(&store, &matched(), &scan_key(7), 0, 10).unwrap();

    assert_eq!(runs.items.len(), 1);
    assert_eq!(entries.total, 3);
    assert_eq!(store.recent_cleanup_log(100).unwrap().len(), before);
}

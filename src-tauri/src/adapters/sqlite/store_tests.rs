//! `SqliteStore` contra la bateria de contrato y pruebas propias del adaptador.

use std::cell::RefCell;
use std::sync::Arc;

use serde_json::json;
use tempfile::TempDir;

use super::SqliteStore;
use super::rows::{RawLogRow, log_entry_from_parts, scan_from_parts};
use crate::domain::model::{
    CleanupOutcome, EpochMillis, NewCleanupLogEntry, Scan, ScanId, ScanKind, ScanStatus,
    ScanTrigger,
};
use crate::domain::ports::store_contract as contract;
use crate::domain::ports::{CleanupLogStore, SettingsStore, StoreError};

/// Ejecuta `check` con una fabrica de stores sobre directorios temporales (viven hasta el final).
fn with_temp_stores(check: impl FnOnce(&dyn Fn() -> SqliteStore)) {
    let dirs: RefCell<Vec<TempDir>> = RefCell::new(Vec::new());
    let make = || {
        let dir = TempDir::new().unwrap();
        let store = SqliteStore::open(&dir.path().join("data.db")).unwrap();
        dirs.borrow_mut().push(dir);
        store
    };
    check(&make);
}

// --- 6.1 bateria de contrato ---
#[test]
fn sqlite_store_should_seed_defaults() {
    with_temp_stores(|make| contract::contract_seed_defaults(make));
}

#[test]
fn sqlite_store_should_satisfy_settings_contract() {
    with_temp_stores(|make| contract::contract_settings(make));
}

#[test]
fn sqlite_store_should_satisfy_scan_contracts() {
    with_temp_stores(|make| {
        contract::contract_scan_lifecycle(make);
        contract::contract_scan_listing_order(make);
    });
}

#[test]
fn sqlite_store_should_satisfy_retention_contracts() {
    with_temp_stores(|make| {
        contract::contract_retention(make);
        contract::contract_log_survives_scan_pruning(make);
    });
}

#[test]
fn sqlite_store_should_satisfy_log_contracts() {
    with_temp_stores(|make| {
        contract::contract_log(make);
        contract::contract_log_batch_is_atomic(make);
    });
}

// --- C-08: hallazgos de basura ---
#[test]
fn sqlite_store_should_satisfy_contract_junk_scan_round_trip() {
    with_temp_stores(|make| contract::contract_junk_scan_round_trip(make));
}

#[test]
fn sqlite_store_should_satisfy_contract_junk_second_scan_purges_previous_findings_keeping_summaries()
 {
    with_temp_stores(|make| {
        contract::contract_junk_second_scan_purges_previous_findings_keeping_summaries(make)
    });
}

#[test]
fn sqlite_store_should_satisfy_contract_junk_failed_save_is_atomic() {
    with_temp_stores(|make| contract::contract_junk_failed_save_is_atomic(make));
}

#[test]
fn sqlite_store_should_satisfy_contract_junk_complete_requires_a_running_junk_scan() {
    with_temp_stores(|make| contract::contract_junk_complete_requires_a_running_junk_scan(make));
}

#[test]
fn sqlite_store_should_satisfy_contract_junk_findings_pagination() {
    with_temp_stores(|make| contract::contract_junk_findings_pagination(make));
}

#[test]
fn sqlite_store_should_satisfy_contract_last_done_junk_scan_ignores_unfinished_and_other_kinds() {
    with_temp_stores(|make| {
        contract::contract_last_done_junk_scan_ignores_unfinished_and_other_kinds(make)
    });
}

#[test]
fn sqlite_store_should_satisfy_contract_junk_findings_follow_scan_retention() {
    with_temp_stores(|make| contract::contract_junk_findings_follow_scan_retention(make));
}

// --- 6.2 persistencia real y conversiones ---
#[test]
fn sqlite_store_should_persist_changes_across_reopen_without_reseeding() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("data.db");
    {
        let store = SqliteStore::open(&path).unwrap();
        store.set_setting("ui_mode", &json!("advanced")).unwrap();
    }
    let reopened = SqliteStore::open(&path).unwrap();
    assert_eq!(
        reopened.get_setting("ui_mode").unwrap(),
        Some(json!("advanced"))
    );
    assert_eq!(reopened.list_settings().unwrap().len(), 3);
    let conn = rusqlite::Connection::open(&path).unwrap();
    let version: i64 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(version, 2);
}

#[test]
fn scan_from_parts_should_convert_valid_values_and_flag_corrupt_ones() {
    let ok = scan_from_parts(7, "junk", "scheduled", "done", 100, Some(200)).unwrap();
    assert_eq!(
        ok,
        Scan {
            id: ScanId(7),
            kind: ScanKind::Junk,
            trigger: ScanTrigger::Scheduled,
            status: ScanStatus::Done,
            started_at: EpochMillis(100),
            finished_at: Some(EpochMillis(200)),
        }
    );
    for (kind, trigger, status) in [
        ("quick", "manual", "done"),
        ("full", "auto", "done"),
        ("full", "manual", "DONE"),
    ] {
        let err = scan_from_parts(1, kind, trigger, status, 100, None).unwrap_err();
        assert!(
            matches!(err, StoreError::Corrupt(_)),
            "{kind}/{trigger}/{status}: {err:?}"
        );
    }
}

#[test]
fn log_entry_from_parts_should_reject_unknown_outcome_and_negative_size() {
    let row = |size_bytes: i64, outcome: &str, scan_id: Option<i64>| RawLogRow {
        id: 1,
        ts: 100,
        category: "temp".into(),
        path: "C:/a".into(),
        size_bytes,
        outcome: outcome.to_string(),
        scan_id,
        error_detail: None,
    };
    let ok = log_entry_from_parts(row(5, "skipped_cloud", Some(3))).unwrap();
    assert_eq!(ok.outcome, CleanupOutcome::SkippedCloud);
    assert_eq!(ok.scan_id, Some(ScanId(3)));
    assert!(matches!(
        log_entry_from_parts(row(5, "deleted", None)),
        Err(StoreError::Corrupt(_))
    ));
    assert!(matches!(
        log_entry_from_parts(row(-5, "error", None)),
        Err(StoreError::Corrupt(_))
    ));
}

// --- 6.3 concurrencia ---
fn assert_send_sync<T: Send + Sync>() {}

#[test]
fn sqlite_store_should_be_send_and_sync() {
    assert_send_sync::<SqliteStore>();
}

#[test]
fn sqlite_store_should_accept_concurrent_appends_from_four_threads() {
    let dir = TempDir::new().unwrap();
    let store = Arc::new(SqliteStore::open(&dir.path().join("data.db")).unwrap());
    let handles: Vec<_> = (0..4)
        .map(|t| {
            let store = Arc::clone(&store);
            std::thread::spawn(move || {
                for i in 0..25 {
                    let entry = NewCleanupLogEntry {
                        ts: EpochMillis(1_000 + i64::from(t) * 100 + i),
                        category: "temp".into(),
                        path: format!("C:/t{t}/{i}"),
                        size_bytes: 1,
                        outcome: CleanupOutcome::MovedToTrash,
                        scan_id: None,
                        error_detail: None,
                    };
                    store.append_cleanup_log(&[entry])?;
                }
                Ok::<(), StoreError>(())
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap().unwrap();
    }
    assert_eq!(store.recent_cleanup_log(1_000).unwrap().len(), 100);
}

// --- C-08: SQL parametrizado y filas corruptas ---
mod junk_rows {
    use super::*;
    use crate::adapters::sqlite::rows::{
        RawCategoryRow, RawFindingRow, category_result_from_parts, finding_from_parts,
    };
    use crate::domain::junk::category::JunkCategory;
    use crate::domain::junk::model::{
        CategoryResult, CategoryStatus, FindingState, JunkFinding, JunkScanResults,
    };
    use crate::domain::model::NewScan;
    use crate::domain::ports::{FileIdentity, JunkFindingStore, ScanStore};

    #[test]
    fn hostile_paths_should_be_stored_and_read_back_as_plain_data() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("data.db");
        let store = SqliteStore::open(&path).unwrap();
        let scan = store
            .create_scan(&NewScan {
                kind: ScanKind::Junk,
                trigger: ScanTrigger::Manual,
                started_at: EpochMillis(1_000),
            })
            .unwrap();
        let hostile = r"C:\t\'); DROP TABLE junk_finding; --";
        let results = JunkScanResults {
            findings: vec![JunkFinding {
                category: JunkCategory::TempUser,
                path: hostile.to_string(),
                size_bytes: 1,
                modified_at: None,
                state: FindingState::Eligible(FileIdentity {
                    volume_serial: 1,
                    file_id: 1,
                }),
            }],
            categories: JunkCategory::ALL
                .into_iter()
                .map(|c| CategoryResult::empty(c, CategoryStatus::Done, None))
                .collect(),
        };
        store
            .complete_junk_scan(scan, &results, EpochMillis(1_100))
            .unwrap();
        let page = store
            .list_junk_findings(scan, JunkCategory::TempUser, 0, 10)
            .unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].finding.path, hostile);
    }

    fn finding_row(
        skip_reason: Option<&str>,
        volume_serial: Option<i64>,
        file_id: Option<&str>,
    ) -> RawFindingRow {
        RawFindingRow {
            id: 1,
            category: JunkCategory::TempUser,
            path: r"C:\t\a".into(),
            size_bytes: 5,
            modified_at: None,
            skip_reason: skip_reason.map(str::to_string),
            volume_serial,
            file_id: file_id.map(str::to_string),
        }
    }

    #[test]
    fn finding_from_parts_should_convert_both_states_and_flag_inconsistent_rows() {
        let skipped = finding_from_parts(finding_row(Some("reparse_point"), None, None)).unwrap();
        assert_eq!(
            skipped.finding.state,
            FindingState::Skipped("reparse_point".into())
        );
        let eligible = finding_from_parts(finding_row(
            None,
            Some(-1),
            Some("000000000000000000000000000000ff"),
        ))
        .unwrap();
        assert_eq!(
            eligible.finding.state,
            FindingState::Eligible(FileIdentity {
                volume_serial: u64::MAX,
                file_id: 255
            })
        );
        for bad in [
            finding_row(None, None, None),
            finding_row(None, Some(1), None),
            finding_row(None, Some(1), Some("no-es-hexadecimal-no-es-hexadecimal")),
        ] {
            assert!(matches!(
                finding_from_parts(bad),
                Err(StoreError::Corrupt(_))
            ));
        }
        let negative = RawFindingRow {
            size_bytes: -1,
            ..finding_row(Some("x"), None, None)
        };
        assert!(matches!(
            finding_from_parts(negative),
            Err(StoreError::Corrupt(_))
        ));
    }

    fn category_row(category: &str, status: &str, reason: Option<&str>) -> RawCategoryRow {
        RawCategoryRow {
            category: category.into(),
            scope: "per_user".into(),
            status: status.into(),
            block_reason: reason.map(str::to_string),
            item_count: 1,
            size_bytes: 2,
            skipped_count: 3,
            inaccessible_count: 4,
        }
    }

    #[test]
    fn category_result_from_parts_should_convert_valid_rows_and_flag_corrupt_ones() {
        let ok = category_result_from_parts(category_row(
            "temp_user",
            "blocked",
            Some("different_user"),
        ))
        .unwrap();
        assert_eq!(ok.category, JunkCategory::TempUser);
        assert_eq!(ok.status, CategoryStatus::Blocked);
        assert_eq!(
            ok.block_reason,
            Some(crate::domain::privileges::MismatchReason::DifferentUser)
        );
        assert_eq!(
            (
                ok.item_count,
                ok.size_bytes,
                ok.skipped_count,
                ok.inaccessible_count
            ),
            (1, 2, 3, 4)
        );
        for bad in [
            category_row("documents", "done", None),
            category_row("temp_user", "ready", None),
            category_row("temp_user", "blocked", Some("otro")),
            RawCategoryRow {
                scope: "global".into(),
                ..category_row("temp_user", "done", None)
            },
            RawCategoryRow {
                item_count: -1,
                ..category_row("temp_user", "done", None)
            },
        ] {
            assert!(matches!(
                category_result_from_parts(bad),
                Err(StoreError::Corrupt(_))
            ));
        }
    }
}

// --- C-08: escaneos huerfanos (D-16) ---
mod orphan_scans {
    use super::*;
    use crate::domain::model::NewScan;
    use crate::domain::ports::ScanStore;

    fn new_scan(kind: ScanKind, started: i64) -> NewScan {
        NewScan {
            kind,
            trigger: ScanTrigger::Manual,
            started_at: EpochMillis(started),
        }
    }

    fn scans_by_id(store: &SqliteStore) -> Vec<Scan> {
        let mut scans = store.list_scans().unwrap();
        scans.sort_by_key(|s| s.id);
        scans
    }

    #[test]
    fn open_should_close_every_running_scan_left_by_a_previous_run_as_error() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("data.db");
        let (junk, full, done) = {
            let store = SqliteStore::open(&path).unwrap();
            let junk = store.create_scan(&new_scan(ScanKind::Junk, 1_000)).unwrap();
            let full = store.create_scan(&new_scan(ScanKind::Full, 2_000)).unwrap();
            let done = store.create_scan(&new_scan(ScanKind::Junk, 3_000)).unwrap();
            store
                .finish_scan(done, ScanStatus::Done, EpochMillis(3_500))
                .unwrap();
            (junk, full, done)
        };
        let store = SqliteStore::open(&path).unwrap();
        for id in [junk, full] {
            let scan = store.get_scan(id).unwrap().unwrap();
            assert_eq!(scan.status, ScanStatus::Error, "{id:?}");
            let finished = scan.finished_at.unwrap();
            assert!(finished >= scan.started_at, "{id:?}");
            assert!(
                finished.0 > 1_700_000_000_000,
                "fin = momento de apertura, no el inicio"
            );
        }
        let untouched = store.get_scan(done).unwrap().unwrap();
        assert_eq!(untouched.status, ScanStatus::Done);
        assert_eq!(untouched.finished_at, Some(EpochMillis(3_500)));
    }

    #[test]
    fn open_should_never_end_a_scan_before_it_started() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("data.db");
        let future = 9_999_999_999_999;
        let id = {
            let store = SqliteStore::open(&path).unwrap();
            store
                .create_scan(&new_scan(ScanKind::Junk, future))
                .unwrap()
        };
        let scan = SqliteStore::open(&path)
            .unwrap()
            .get_scan(id)
            .unwrap()
            .unwrap();
        assert_eq!(scan.status, ScanStatus::Error);
        assert_eq!(scan.finished_at, Some(EpochMillis(future)));
    }

    #[test]
    fn open_should_change_nothing_without_running_scans_and_stay_idempotent() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("data.db");
        {
            let store = SqliteStore::open(&path).unwrap();
            let done = store.create_scan(&new_scan(ScanKind::Junk, 1_000)).unwrap();
            store
                .finish_scan(done, ScanStatus::Done, EpochMillis(1_100))
                .unwrap();
            let failed = store.create_scan(&new_scan(ScanKind::Full, 2_000)).unwrap();
            store
                .finish_scan(failed, ScanStatus::Cancelled, EpochMillis(2_100))
                .unwrap();
        }
        let before = scans_by_id(&SqliteStore::open(&path).unwrap());
        assert_eq!(before.len(), 2);
        let after = scans_by_id(&SqliteStore::open(&path).unwrap());
        assert_eq!(before, after);
    }

    #[test]
    fn open_should_close_an_orphan_only_once_keeping_its_first_end() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("data.db");
        let id = {
            let store = SqliteStore::open(&path).unwrap();
            store.create_scan(&new_scan(ScanKind::Junk, 1_000)).unwrap()
        };
        let first = SqliteStore::open(&path).unwrap().get_scan(id).unwrap();
        let second = SqliteStore::open(&path).unwrap().get_scan(id).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn open_should_drop_findings_and_summaries_of_an_orphan_scan() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("data.db");
        let id = {
            let store = SqliteStore::open(&path).unwrap();
            store.create_scan(&new_scan(ScanKind::Junk, 1_000)).unwrap()
        };
        {
            // Filas que el guardado atomico (D-04) nunca deja, escritas a mano.
            let raw = rusqlite::Connection::open(&path).unwrap();
            raw.execute(
                "INSERT INTO junk_finding(scan_id, category, path, size_bytes, protected_flag, \
                 skip_reason) VALUES (?1, 'temp_user', 'C:/a', 1, 1, 'reparse_point')",
                [id.0],
            )
            .unwrap();
            raw.execute(
                "INSERT INTO junk_category_result(scan_id, category, scope, status, item_count, \
                 size_bytes, skipped_count, inaccessible_count) \
                 VALUES (?1, 'temp_user', 'per_user', 'done', 0, 0, 1, 0)",
                [id.0],
            )
            .unwrap();
        }
        let _store = SqliteStore::open(&path).unwrap();
        let raw = rusqlite::Connection::open(&path).unwrap();
        for table in ["junk_finding", "junk_category_result"] {
            let rows: i64 = raw
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
                .unwrap();
            assert_eq!(rows, 0, "{table}");
        }
    }
}

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
    assert_eq!(version, 1);
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

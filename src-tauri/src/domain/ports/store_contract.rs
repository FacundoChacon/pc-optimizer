//! Bateria de contrato compartida por todas las implementaciones de `Store` (solo tests).
//!
//! Cada funcion recibe una fabrica que devuelve un store nuevo y vacio (con el seed).

use serde_json::json;

use crate::domain::model::{
    CleanupLogEntry, CleanupOutcome, DEFAULT_SETTINGS, EpochMillis, NewCleanupLogEntry, NewScan,
    SCAN_RETENTION, ScanId, ScanKind, ScanStatus, ScanTrigger,
};
use crate::domain::ports::store::{Store, StoreError};

fn new_scan(started: i64) -> NewScan {
    NewScan {
        kind: ScanKind::Full,
        trigger: ScanTrigger::Manual,
        started_at: EpochMillis(started),
    }
}

/// Crea un escaneo y lo finaliza como `Done`.
fn create_done_scan<S: Store>(store: &S, started: i64) -> Result<ScanId, StoreError> {
    let id = store.create_scan(&new_scan(started))?;
    store.finish_scan(id, ScanStatus::Done, EpochMillis(started + 1))?;
    Ok(id)
}

pub(crate) fn log_entry(ts: i64, path: &str, scan_id: Option<ScanId>) -> NewCleanupLogEntry {
    NewCleanupLogEntry {
        ts: EpochMillis(ts),
        category: "temp_files".into(),
        path: path.into(),
        size_bytes: 1024,
        outcome: CleanupOutcome::MovedToTrash,
        scan_id,
        error_detail: None,
    }
}

fn same_content(new: &NewCleanupLogEntry, stored: &CleanupLogEntry) -> bool {
    new.ts == stored.ts
        && new.category == stored.category
        && new.path == stored.path
        && new.size_bytes == stored.size_bytes
        && new.outcome == stored.outcome
        && new.scan_id == stored.scan_id
        && new.error_detail == stored.error_detail
}

pub(crate) fn contract_seed_defaults<S: Store>(make: impl Fn() -> S) {
    let store = make();
    for (key, raw) in DEFAULT_SETTINGS {
        let expected: serde_json::Value = serde_json::from_str(raw).unwrap();
        assert_eq!(store.get_setting(key).unwrap(), Some(expected), "key {key}");
    }
    assert_eq!(store.get_setting("ui_mode").unwrap(), Some(json!("simple")));
    assert_eq!(
        store.get_setting("autostart_tray").unwrap(),
        Some(json!(false))
    );
    assert_eq!(store.get_setting("language").unwrap(), Some(json!("es")));
    assert_eq!(store.list_settings().unwrap().len(), DEFAULT_SETTINGS.len());
}

pub(crate) fn contract_settings<S: Store>(make: impl Fn() -> S) {
    let store = make();
    assert_eq!(store.get_setting("no_existe").unwrap(), None);

    store.set_setting("ui_mode", &json!("advanced")).unwrap();
    store.set_setting("ui_mode", &json!("simple")).unwrap();
    store.set_setting("zeta", &json!({"a": [1, 2]})).unwrap();
    assert_eq!(store.get_setting("ui_mode").unwrap(), Some(json!("simple")));
    assert_eq!(
        store.get_setting("zeta").unwrap(),
        Some(json!({"a": [1, 2]}))
    );

    let listed = store.list_settings().unwrap();
    let keys: Vec<&str> = listed.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(keys, vec!["autostart_tray", "language", "ui_mode", "zeta"]);

    assert!(matches!(
        store.set_setting("", &json!(1)),
        Err(StoreError::Constraint(_))
    ));
}

pub(crate) fn contract_scan_lifecycle<S: Store>(make: impl Fn() -> S) {
    let store = make();
    let id = store.create_scan(&new_scan(1000)).unwrap();
    let created = store.get_scan(id).unwrap().unwrap();
    assert_eq!(created.status, ScanStatus::Running);
    assert_eq!(created.finished_at, None);
    assert_eq!(created.started_at, EpochMillis(1000));
    assert_eq!(created.kind, ScanKind::Full);
    assert_eq!(created.trigger, ScanTrigger::Manual);

    store
        .finish_scan(id, ScanStatus::Done, EpochMillis(2000))
        .unwrap();
    let done = store.get_scan(id).unwrap().unwrap();
    assert_eq!(done.status, ScanStatus::Done);
    assert_eq!(done.finished_at, Some(EpochMillis(2000)));

    // Doble finalizacion: error y el estado no cambia.
    assert!(matches!(
        store.finish_scan(id, ScanStatus::Error, EpochMillis(3000)),
        Err(StoreError::InvalidState(_))
    ));
    assert_eq!(store.get_scan(id).unwrap().unwrap(), done);

    // Id inexistente.
    assert!(matches!(
        store.finish_scan(ScanId(9_999), ScanStatus::Done, EpochMillis(3000)),
        Err(StoreError::NotFound(_))
    ));
    assert_eq!(store.get_scan(ScanId(9_999)).unwrap(), None);

    // Finalizar con un estado no terminal.
    let running = store.create_scan(&new_scan(1500)).unwrap();
    assert!(matches!(
        store.finish_scan(running, ScanStatus::Running, EpochMillis(3000)),
        Err(StoreError::InvalidState(_))
    ));
    assert_eq!(
        store.get_scan(running).unwrap().unwrap().status,
        ScanStatus::Running
    );
}

pub(crate) fn contract_scan_listing_order<S: Store>(make: impl Fn() -> S) {
    let store = make();
    let a = store.create_scan(&new_scan(100)).unwrap();
    let c = store.create_scan(&new_scan(300)).unwrap();
    let b = store.create_scan(&new_scan(200)).unwrap();
    let c2 = store.create_scan(&new_scan(300)).unwrap();
    let ids: Vec<ScanId> = store
        .list_scans()
        .unwrap()
        .into_iter()
        .map(|s| s.id)
        .collect();
    assert_eq!(ids, vec![c2, c, b, a]);
}

pub(crate) fn contract_retention<S: Store>(make: impl Fn() -> S) {
    // 50 finalizados + 1 nuevo: queda el limite y falta el mas antiguo.
    let store = make();
    let first = create_done_scan(&store, 1_000).unwrap();
    for i in 1..SCAN_RETENTION as i64 {
        create_done_scan(&store, 1_000 + i).unwrap();
    }
    assert_eq!(store.list_scans().unwrap().len(), SCAN_RETENTION);
    let newest = store.create_scan(&new_scan(5_000)).unwrap();
    let scans = store.list_scans().unwrap();
    assert_eq!(scans.len(), SCAN_RETENTION);
    assert_eq!(store.get_scan(first).unwrap(), None);
    assert!(store.get_scan(newest).unwrap().is_some());

    // El mas antiguo `running` se conserva; se poda el finalizado siguiente.
    let store = make();
    let running = store.create_scan(&new_scan(10)).unwrap();
    let oldest_done = create_done_scan(&store, 1_000).unwrap();
    for i in 1..SCAN_RETENTION as i64 {
        create_done_scan(&store, 1_000 + i).unwrap();
    }
    assert_eq!(store.list_scans().unwrap().len(), SCAN_RETENTION + 1);
    assert!(store.get_scan(running).unwrap().is_some());
    store.create_scan(&new_scan(9_000)).unwrap();
    assert!(store.get_scan(running).unwrap().is_some());
    assert_eq!(store.get_scan(oldest_done).unwrap(), None);
    assert_eq!(store.list_scans().unwrap().len(), SCAN_RETENTION + 1);
}

pub(crate) fn contract_log_survives_scan_pruning<S: Store>(make: impl Fn() -> S) {
    let store = make();
    let doomed = create_done_scan(&store, 1_000).unwrap();
    let entry = log_entry(1_500, r"C:\tmp\a.tmp", Some(doomed));
    store
        .append_cleanup_log(std::slice::from_ref(&entry))
        .unwrap();
    for i in 1..=SCAN_RETENTION as i64 {
        create_done_scan(&store, 1_000 + i).unwrap();
    }
    assert_eq!(store.get_scan(doomed).unwrap(), None);
    let log = store.recent_cleanup_log(10).unwrap();
    assert_eq!(log.len(), 1);
    assert!(same_content(&entry, &log[0]));
    assert_eq!(log[0].scan_id, Some(doomed));
}

pub(crate) fn contract_log<S: Store>(make: impl Fn() -> S) {
    let store = make();
    let batch = vec![
        log_entry(100, r"C:\tmp\uno", None),
        log_entry(300, r"C:\tmp\dos", None),
        NewCleanupLogEntry {
            outcome: CleanupOutcome::Error,
            error_detail: Some("access denied".into()),
            ..log_entry(300, r"C:\tmp\tres", None)
        },
    ];
    store.append_cleanup_log(&batch).unwrap();

    let log = store.recent_cleanup_log(10).unwrap();
    assert_eq!(log.len(), 3);
    // ts DESC, id DESC: los dos de ts=300 van primero, el insertado despues antes.
    assert!(same_content(&batch[2], &log[0]));
    assert!(same_content(&batch[1], &log[1]));
    assert!(same_content(&batch[0], &log[2]));
    assert!(log[0].id > log[1].id && log[1].id > log[2].id);

    assert_eq!(store.recent_cleanup_log(2).unwrap().len(), 2);
    assert!(store.recent_cleanup_log(0).unwrap().is_empty());
}

pub(crate) fn contract_log_batch_is_atomic<S: Store>(make: impl Fn() -> S) {
    let empty_category = NewCleanupLogEntry {
        category: String::new(),
        ..log_entry(200, "x", None)
    };
    let oversized = NewCleanupLogEntry {
        size_bytes: u64::MAX,
        ..log_entry(200, "x", None)
    };
    for invalid in [empty_category, oversized] {
        let store = make();
        let result = store.append_cleanup_log(&[log_entry(100, "ok", None), invalid]);
        assert!(
            matches!(result, Err(StoreError::Constraint(_))),
            "{result:?}"
        );
        assert!(store.recent_cleanup_log(10).unwrap().is_empty());
    }
}

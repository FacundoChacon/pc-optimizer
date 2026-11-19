//! Bateria de contrato compartida por todas las implementaciones de `Store` (solo tests).
//!
//! Cada funcion recibe una fabrica que devuelve un store nuevo y vacio (con el seed).

use serde_json::json;

use crate::domain::junk::category::JunkCategory;
use crate::domain::junk::model::{
    CategoryResult, CategoryStatus, FindingState, JunkFinding, JunkFindingPage, JunkScanResults,
};
use crate::domain::model::{
    CleanupLogEntry, CleanupOutcome, DEFAULT_SETTINGS, EpochMillis, NewCleanupLogEntry, NewScan,
    SCAN_RETENTION, ScanId, ScanKind, ScanStatus, ScanTrigger,
};
use crate::domain::ports::FileIdentity;
use crate::domain::ports::store::{CleanupRunKey, RunCursor, Store, StoreError};

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

// ---- C-08: hallazgos de basura ----

fn junk_scan<S: Store>(store: &S, started: i64) -> ScanId {
    store
        .create_scan(&NewScan {
            kind: ScanKind::Junk,
            trigger: ScanTrigger::Manual,
            started_at: EpochMillis(started),
        })
        .unwrap()
}

fn identity(n: u128) -> FileIdentity {
    FileIdentity {
        volume_serial: 0x8000_0000_dead_beef,
        file_id: n,
    }
}

fn eligible(category: JunkCategory, path: &str, size: u64, n: u128) -> JunkFinding {
    JunkFinding {
        category,
        path: path.to_string(),
        size_bytes: size,
        modified_at: Some(EpochMillis(1_700_000_000_000)),
        state: FindingState::Eligible(identity(n)),
    }
}

fn skipped(category: JunkCategory, path: &str, reason: &str) -> JunkFinding {
    JunkFinding {
        category,
        path: path.to_string(),
        size_bytes: 7,
        modified_at: None,
        state: FindingState::Skipped(reason.to_string()),
    }
}

/// Resumen con las 11 categorias: contadores derivados de los hallazgos y la Papelera medida.
fn results_for(findings: Vec<JunkFinding>) -> JunkScanResults {
    let categories = JunkCategory::ALL
        .into_iter()
        .map(|category| {
            let mine = || findings.iter().filter(|f| f.category == category);
            let status = match category {
                JunkCategory::OldLogs => CategoryStatus::Unavailable,
                _ => CategoryStatus::Done,
            };
            let result = CategoryResult {
                item_count: mine().filter(|f| f.is_eligible()).count() as u64,
                size_bytes: mine()
                    .filter(|f| f.is_eligible())
                    .map(|f| f.size_bytes)
                    .sum(),
                skipped_count: mine().filter(|f| !f.is_eligible()).count() as u64,
                ..CategoryResult::empty(category, status, None)
            };
            match category {
                JunkCategory::RecycleBin => CategoryResult {
                    item_count: 12,
                    size_bytes: 340,
                    ..result
                },
                JunkCategory::TempUser => CategoryResult {
                    inaccessible_count: 2,
                    ..result
                },
                _ => result,
            }
        })
        .collect();
    JunkScanResults {
        findings,
        categories,
    }
}

fn page<S: Store>(store: &S, scan: ScanId, category: JunkCategory) -> JunkFindingPage {
    store.list_junk_findings(scan, category, 0, 500).unwrap()
}

fn paths(page: &JunkFindingPage) -> Vec<&str> {
    page.items.iter().map(|i| i.finding.path.as_str()).collect()
}

pub(crate) fn contract_junk_scan_round_trip<S: Store>(make: impl Fn() -> S) {
    let store = make();
    assert_eq!(store.last_done_junk_scan().unwrap(), None);
    let scan = junk_scan(&store, 1_000);
    let results = results_for(vec![
        eligible(JunkCategory::TempUser, r"C:\t\small", 10, 1),
        eligible(JunkCategory::TempUser, r"C:\t\big", 900, u128::MAX),
        eligible(JunkCategory::TempUser, r"C:\t\tie-a", 10, 3),
        skipped(JunkCategory::TempUser, r"C:\t\link", "reparse_point"),
        eligible(JunkCategory::WuCache, r"C:\w\x", 5, 4),
    ]);
    store
        .complete_junk_scan(scan, &results, EpochMillis(1_500))
        .unwrap();

    let stored = store.get_scan(scan).unwrap().unwrap();
    assert_eq!(stored.status, ScanStatus::Done);
    assert_eq!(stored.finished_at, Some(EpochMillis(1_500)));
    let summary = store.last_done_junk_scan().unwrap().unwrap();
    assert_eq!(summary.scan, stored);
    assert_eq!(summary.categories, results.categories);
    assert_eq!(summary.categories.len(), 11);
    assert_eq!(summary.total_items(), 4);
    assert_eq!(summary.total_bytes(), 925);

    let temp = page(&store, scan, JunkCategory::TempUser);
    assert_eq!(temp.total, 4);
    // size_bytes DESC, id ASC (el empate 10/10 conserva el orden de insercion; el omitido
    // pesa 7).
    assert_eq!(
        paths(&temp),
        vec![r"C:\t\big", r"C:\t\small", r"C:\t\tie-a", r"C:\t\link"]
    );
    let big = &temp.items[0].finding;
    assert_eq!(big.state, FindingState::Eligible(identity(u128::MAX)));
    assert_eq!(big.modified_at, Some(EpochMillis(1_700_000_000_000)));
    let link = &temp.items[3].finding;
    assert_eq!(link.state, FindingState::Skipped("reparse_point".into()));
    assert_eq!((link.size_bytes, link.modified_at), (7, None));
    assert!(temp.items[0].id != temp.items[1].id);
    assert!(temp.items[1].id < temp.items[2].id, "empate: id ascendente");
    assert_eq!(page(&store, scan, JunkCategory::WuCache).total, 1);
    assert_eq!(page(&store, scan, JunkCategory::RecycleBin).total, 0);
}

pub(crate) fn contract_junk_second_scan_purges_previous_findings_keeping_summaries<S: Store>(
    make: impl Fn() -> S,
) {
    let store = make();
    let first = junk_scan(&store, 1_000);
    let first_results = results_for(
        (0..10)
            .map(|i| eligible(JunkCategory::TempUser, &format!(r"C:\t\{i}"), 5, i + 1))
            .collect(),
    );
    store
        .complete_junk_scan(first, &first_results, EpochMillis(1_100))
        .unwrap();
    // Un escaneo de otro tipo no se toca.
    let full = create_done_scan(&store, 1_200).unwrap();

    let second = junk_scan(&store, 2_000);
    let second_results = results_for(
        (0..4)
            .map(|i| eligible(JunkCategory::TempUser, &format!(r"C:\t\{i}"), 9, i + 1))
            .collect(),
    );
    store
        .complete_junk_scan(second, &second_results, EpochMillis(2_100))
        .unwrap();

    assert_eq!(page(&store, first, JunkCategory::TempUser).total, 0);
    assert_eq!(page(&store, second, JunkCategory::TempUser).total, 4);
    assert_eq!(
        store.get_scan(first).unwrap().unwrap().status,
        ScanStatus::Done
    );
    assert!(store.get_scan(full).unwrap().is_some());
    let latest = store.last_done_junk_scan().unwrap().unwrap();
    assert_eq!(latest.scan.id, second);
    assert_eq!(latest.categories, second_results.categories);
}

pub(crate) fn contract_junk_failed_save_is_atomic<S: Store>(make: impl Fn() -> S) {
    let store = make();
    let first = junk_scan(&store, 1_000);
    let first_results = results_for(vec![eligible(JunkCategory::TempUser, r"C:\t\a", 5, 1)]);
    store
        .complete_junk_scan(first, &first_results, EpochMillis(1_100))
        .unwrap();

    let scan = junk_scan(&store, 2_000);
    // Dos hallazgos con la misma ruta: el segundo viola la unicidad a mitad del guardado.
    let duplicated = results_for(vec![
        eligible(JunkCategory::TempUser, r"C:\t\dup", 5, 10),
        eligible(JunkCategory::WuCache, r"C:\w\ok", 5, 11),
        eligible(JunkCategory::TempUser, r"C:\t\dup", 6, 12),
    ]);
    let result = store.complete_junk_scan(scan, &duplicated, EpochMillis(2_100));
    assert!(
        matches!(result, Err(StoreError::Constraint(_))),
        "{result:?}"
    );
    // Un resumen incoherente (bloqueada sin motivo) tambien se rechaza entero.
    let mut incoherent = results_for(vec![eligible(JunkCategory::WuCache, r"C:\w\ok", 5, 11)]);
    incoherent.categories[0].status = CategoryStatus::Blocked;
    let result = store.complete_junk_scan(scan, &incoherent, EpochMillis(2_100));
    assert!(
        matches!(result, Err(StoreError::Constraint(_))),
        "{result:?}"
    );

    assert_eq!(
        store.get_scan(scan).unwrap().unwrap().status,
        ScanStatus::Running
    );
    assert_eq!(page(&store, scan, JunkCategory::WuCache).total, 0);
    assert_eq!(page(&store, scan, JunkCategory::TempUser).total, 0);
    assert_eq!(
        page(&store, first, JunkCategory::TempUser).total,
        1,
        "el escaneo anterior sigue intacto"
    );
    assert_eq!(store.last_done_junk_scan().unwrap().unwrap().scan.id, first);
    // El escaneo sigue `Running`: puede cerrarse como error.
    store
        .finish_scan(scan, ScanStatus::Error, EpochMillis(2_200))
        .unwrap();
}

pub(crate) fn contract_junk_complete_requires_a_running_junk_scan<S: Store>(make: impl Fn() -> S) {
    let store = make();
    let results = results_for(vec![]);
    let missing = store.complete_junk_scan(ScanId(999), &results, EpochMillis(10));
    assert!(
        matches!(missing, Err(StoreError::NotFound(_))),
        "{missing:?}"
    );

    let full = store.create_scan(&new_scan(1_000)).unwrap();
    let wrong_kind = store.complete_junk_scan(full, &results, EpochMillis(2_000));
    assert!(
        matches!(wrong_kind, Err(StoreError::InvalidState(_))),
        "{wrong_kind:?}"
    );

    let scan = junk_scan(&store, 3_000);
    store
        .complete_junk_scan(scan, &results, EpochMillis(3_100))
        .unwrap();
    let twice = store.complete_junk_scan(scan, &results, EpochMillis(3_200));
    assert!(
        matches!(twice, Err(StoreError::InvalidState(_))),
        "{twice:?}"
    );
}

pub(crate) fn contract_junk_findings_pagination<S: Store>(make: impl Fn() -> S) {
    let store = make();
    let scan = junk_scan(&store, 1_000);
    let findings: Vec<JunkFinding> = (0..1_200u64)
        .map(|i| {
            eligible(
                JunkCategory::TempUser,
                &format!(r"C:\t\f{i:04}"),
                i + 1,
                u128::from(i) + 1,
            )
        })
        .collect();
    store
        .complete_junk_scan(scan, &results_for(findings), EpochMillis(1_100))
        .unwrap();

    let first = store
        .list_junk_findings(scan, JunkCategory::TempUser, 0, 500)
        .unwrap();
    assert_eq!((first.total, first.items.len()), (1_200, 500));
    assert_eq!(
        first.items[0].finding.size_bytes, 1_200,
        "el mas grande primero"
    );
    let last = store
        .list_junk_findings(scan, JunkCategory::TempUser, 1_000, 500)
        .unwrap();
    assert_eq!((last.total, last.items.len()), (1_200, 200));
    assert_eq!(last.items[199].finding.size_bytes, 1);
    let beyond = store
        .list_junk_findings(scan, JunkCategory::TempUser, 5_000, 500)
        .unwrap();
    assert_eq!((beyond.total, beyond.items.len()), (1_200, 0));
    let none = store
        .list_junk_findings(scan, JunkCategory::TempUser, 0, 0)
        .unwrap();
    assert_eq!((none.total, none.items.len()), (1_200, 0));
    let unknown = store
        .list_junk_findings(ScanId(999), JunkCategory::TempUser, 0, 10)
        .unwrap();
    assert_eq!((unknown.total, unknown.items.len()), (0, 0));
}

pub(crate) fn contract_last_done_junk_scan_ignores_unfinished_and_other_kinds<S: Store>(
    make: impl Fn() -> S,
) {
    let store = make();
    let cancelled = junk_scan(&store, 1_000);
    store
        .finish_scan(cancelled, ScanStatus::Cancelled, EpochMillis(1_100))
        .unwrap();
    let errored = junk_scan(&store, 1_200);
    store
        .finish_scan(errored, ScanStatus::Error, EpochMillis(1_300))
        .unwrap();
    junk_scan(&store, 1_400); // sigue `running`
    create_done_scan(&store, 1_500).unwrap(); // `full` terminado
    assert_eq!(store.last_done_junk_scan().unwrap(), None);

    let older = junk_scan(&store, 2_000);
    store
        .complete_junk_scan(older, &results_for(vec![]), EpochMillis(2_100))
        .unwrap();
    let newer = junk_scan(&store, 3_000);
    store
        .complete_junk_scan(newer, &results_for(vec![]), EpochMillis(3_100))
        .unwrap();
    junk_scan(&store, 4_000); // uno posterior, todavia `running`
    assert_eq!(store.last_done_junk_scan().unwrap().unwrap().scan.id, newer);
}

pub(crate) fn contract_junk_findings_follow_scan_retention<S: Store>(make: impl Fn() -> S) {
    let store = make();
    let doomed = junk_scan(&store, 1_000);
    store
        .complete_junk_scan(
            doomed,
            &results_for(vec![eligible(JunkCategory::TempUser, r"C:\t\a", 5, 1)]),
            EpochMillis(1_001),
        )
        .unwrap();
    for i in 1..=SCAN_RETENTION as i64 {
        create_done_scan(&store, 1_000 + i).unwrap();
    }
    assert_eq!(store.get_scan(doomed).unwrap(), None);
    assert_eq!(page(&store, doomed, JunkCategory::TempUser).total, 0);
    assert_eq!(store.last_done_junk_scan().unwrap(), None);
}

// ---- C-09: candidatos de limpieza y fecha de limpieza de un escaneo ----

fn candidate_paths(candidates: &[crate::domain::junk::model::StoredFinding]) -> Vec<&str> {
    candidates.iter().map(|c| c.finding.path.as_str()).collect()
}

pub(crate) fn contract_cleanup_candidates_only_eligible_of_requested_categories<S: Store>(
    make: impl Fn() -> S,
) {
    let store = make();
    let scan = junk_scan(&store, 1_000);
    let results = results_for(vec![
        eligible(JunkCategory::TempUser, r"C:\t\small", 10, 1),
        skipped(JunkCategory::TempUser, r"C:\t\link", "reparse_point"),
        eligible(JunkCategory::TempUser, r"C:\t\big", 900, 2),
        eligible(JunkCategory::WuCache, r"C:\w\a", 5, 3),
        eligible(JunkCategory::WuCache, r"C:\w\b", 6, 4),
        eligible(JunkCategory::WuCache, r"C:\w\c", 7, 5),
    ]);
    store
        .complete_junk_scan(scan, &results, EpochMillis(1_500))
        .unwrap();

    let temp = store
        .cleanup_candidates(scan, &[JunkCategory::TempUser])
        .unwrap();
    assert_eq!(candidate_paths(&temp), vec![r"C:\t\big", r"C:\t\small"]);
    assert!(temp.iter().all(|c| c.finding.is_eligible()));
    assert_eq!(temp[0].finding.size_bytes, 900);
    assert_eq!(
        temp[0].finding.state,
        FindingState::Eligible(identity(2)),
        "trae la identidad persistida"
    );
    let wu = store
        .cleanup_candidates(scan, &[JunkCategory::WuCache])
        .unwrap();
    assert_eq!(wu.len(), 3);
    // La fecha y el id de fila acompanan al candidato.
    assert_eq!(
        wu[0].finding.modified_at,
        Some(EpochMillis(1_700_000_000_000))
    );
    assert!(wu[0].id > 0);
}

pub(crate) fn contract_cleanup_candidates_order_by_category_then_size_then_id<S: Store>(
    make: impl Fn() -> S,
) {
    let store = make();
    let scan = junk_scan(&store, 1_000);
    let results = results_for(vec![
        eligible(JunkCategory::WuCache, r"C:\w\x", 50, 1),
        eligible(JunkCategory::TempUser, r"C:\t\tie-1", 10, 2),
        eligible(JunkCategory::TempUser, r"C:\t\tie-2", 10, 3),
        eligible(JunkCategory::TempUser, r"C:\t\big", 99, 4),
        eligible(JunkCategory::CrashDumps, r"C:\d\z", 1, 5),
    ]);
    store
        .complete_junk_scan(scan, &results, EpochMillis(1_500))
        .unwrap();
    // El orden no depende del orden pedido: sigue `JunkCategory::ALL`.
    let candidates = store
        .cleanup_candidates(
            scan,
            &[
                JunkCategory::CrashDumps,
                JunkCategory::WuCache,
                JunkCategory::TempUser,
            ],
        )
        .unwrap();
    assert_eq!(
        candidate_paths(&candidates),
        vec![
            r"C:\t\big",
            r"C:\t\tie-1",
            r"C:\t\tie-2",
            r"C:\w\x",
            r"C:\d\z"
        ]
    );
}

pub(crate) fn contract_cleanup_candidates_keep_the_identity_bits_intact<S: Store>(
    make: impl Fn() -> S,
) {
    let store = make();
    let scan = junk_scan(&store, 1_000);
    let results = results_for(vec![
        eligible(JunkCategory::TempUser, r"C:\t\max", 2, u128::MAX),
        eligible(
            JunkCategory::TempUser,
            r"C:\t\beef",
            1,
            0x8000_0000_dead_beef,
        ),
    ]);
    store
        .complete_junk_scan(scan, &results, EpochMillis(1_500))
        .unwrap();
    let candidates = store
        .cleanup_candidates(scan, &[JunkCategory::TempUser])
        .unwrap();
    assert_eq!(
        candidates[0].finding.state,
        FindingState::Eligible(identity(u128::MAX))
    );
    assert_eq!(
        candidates[1].finding.state,
        FindingState::Eligible(identity(0x8000_0000_dead_beef))
    );
}

pub(crate) fn contract_cleanup_candidates_are_empty_without_findings<S: Store>(
    make: impl Fn() -> S,
) {
    let store = make();
    // Escaneo inexistente.
    assert!(
        store
            .cleanup_candidates(ScanId(999), &[JunkCategory::TempUser])
            .unwrap()
            .is_empty()
    );
    let scan = junk_scan(&store, 1_000);
    store
        .complete_junk_scan(
            scan,
            &results_for(vec![eligible(JunkCategory::TempUser, r"C:\t\a", 1, 1)]),
            EpochMillis(1_500),
        )
        .unwrap();
    // Sin categorias pedidas, o una categoria sin hallazgos.
    assert!(store.cleanup_candidates(scan, &[]).unwrap().is_empty());
    assert!(
        store
            .cleanup_candidates(scan, &[JunkCategory::WuCache])
            .unwrap()
            .is_empty()
    );
    // Un escaneo posterior purga los hallazgos del anterior.
    let newer = junk_scan(&store, 2_000);
    store
        .complete_junk_scan(newer, &results_for(vec![]), EpochMillis(2_100))
        .unwrap();
    assert!(
        store
            .cleanup_candidates(scan, &[JunkCategory::TempUser])
            .unwrap()
            .is_empty()
    );
}

pub(crate) fn contract_scan_cleaned_at_is_the_latest_log_entry_since_the_scan_ended<S: Store>(
    make: impl Fn() -> S,
) {
    let store = make();
    let scan = ScanId(7);
    // El escaneo 7 termino en t=1000.
    let since = EpochMillis(1_000);
    assert_eq!(store.scan_cleaned_at(scan, since).unwrap(), None);
    store
        .append_cleanup_log(&[log_entry(1_500, r"C:\t\a", Some(scan))])
        .unwrap();
    store
        .append_cleanup_log(&[
            log_entry(1_800, r"C:\t\b", Some(scan)),
            log_entry(1_600, r"C:\t\c", Some(scan)),
        ])
        .unwrap();
    assert_eq!(
        store.scan_cleaned_at(scan, since).unwrap(),
        Some(EpochMillis(1_800))
    );
    // El limite inferior es inclusivo.
    assert_eq!(
        store.scan_cleaned_at(scan, EpochMillis(1_800)).unwrap(),
        Some(EpochMillis(1_800))
    );
    assert_eq!(
        store.scan_cleaned_at(scan, EpochMillis(1_801)).unwrap(),
        None
    );
}

pub(crate) fn contract_scan_cleaned_at_ignores_other_scans_and_older_entries<S: Store>(
    make: impl Fn() -> S,
) {
    let store = make();
    store
        .append_cleanup_log(&[
            log_entry(900, r"C:\t\old", Some(ScanId(7))),
            log_entry(5_000, r"C:\t\other", Some(ScanId(8))),
            log_entry(6_000, r"C:\t\none", None),
        ])
        .unwrap();
    assert_eq!(
        store
            .scan_cleaned_at(ScanId(7), EpochMillis(1_000))
            .unwrap(),
        None,
        "entradas anteriores al fin del escaneo, de otro escaneo o sin escaneo no cuentan"
    );
    assert_eq!(
        store
            .scan_cleaned_at(ScanId(8), EpochMillis(1_000))
            .unwrap(),
        Some(EpochMillis(5_000))
    );
}

/// C-10 D-09: la fila de un vaciado (`scan_id` nulo) se lee igual y no marca ningun escaneo como
/// limpiado; una entrada de limpieza del mismo escaneo si.
pub(crate) fn contract_empty_bin_entry_does_not_mark_the_scan_as_cleaned<S: Store>(
    make: impl Fn() -> S,
) {
    let store = make();
    let scan = create_done_scan(&store, 1_000).unwrap();
    let since = EpochMillis(1_001);
    let emptied = NewCleanupLogEntry {
        ts: EpochMillis(1_500),
        category: "recycle_bin".into(),
        path: r"C:\".into(),
        size_bytes: 340_000_000,
        outcome: CleanupOutcome::RecycleBinEmptied,
        scan_id: None,
        error_detail: Some("partial".into()),
    };
    store
        .append_cleanup_log(std::slice::from_ref(&emptied))
        .unwrap();
    assert_eq!(store.scan_cleaned_at(scan, since).unwrap(), None);
    let log = store.recent_cleanup_log(10).unwrap();
    assert_eq!(log.len(), 1);
    assert!(same_content(&emptied, &log[0]));

    store
        .append_cleanup_log(&[log_entry(1_600, r"C:\t\a", Some(scan))])
        .unwrap();
    assert_eq!(
        store.scan_cleaned_at(scan, since).unwrap(),
        Some(EpochMillis(1_600))
    );
}

// ---- C-12: lectura agrupada del log ----

fn run_row(
    ts: i64,
    category: &str,
    outcome: CleanupOutcome,
    size_bytes: u64,
    scan_id: Option<ScanId>,
) -> NewCleanupLogEntry {
    NewCleanupLogEntry {
        ts: EpochMillis(ts),
        category: category.into(),
        path: format!(r"C:\x\{ts}-{size_bytes}"),
        size_bytes,
        outcome,
        scan_id,
        error_detail: None,
    }
}

pub(crate) fn contract_cleanup_runs_group_rows_of_one_scan<S: Store>(make: impl Fn() -> S) {
    let store = make();
    let scan = Some(ScanId(7));
    store
        .append_cleanup_log(&[
            run_row(1_000, "temp_user", CleanupOutcome::MovedToTrash, 100, scan),
            run_row(1_010, "temp_user", CleanupOutcome::MovedToTrash, 200, scan),
            run_row(1_020, "temp_user", CleanupOutcome::SkippedLocked, 50, scan),
        ])
        .unwrap();

    let runs = store.cleanup_runs(None, 10).unwrap();

    assert_eq!(runs.len(), 1);
    let run = &runs[0];
    assert_eq!(run.key, CleanupRunKey::Scan(ScanId(7)));
    assert_eq!(run.entry_count, 3);
    assert_eq!(run.outcomes.moved_to_trash, 2);
    assert_eq!(run.outcomes.skipped_locked, 1);
    assert_eq!(run.moved_bytes, 300);
    assert_eq!(run.started_at, EpochMillis(1_000));
    assert_eq!(run.finished_at, EpochMillis(1_020));
}

pub(crate) fn contract_cleanup_runs_split_entries_without_scan<S: Store>(make: impl Fn() -> S) {
    let store = make();
    store
        .append_cleanup_log(&[
            run_row(
                2_000,
                "recycle_bin",
                CleanupOutcome::RecycleBinEmptied,
                1_000,
                None,
            ),
            run_row(
                3_000,
                "recycle_bin",
                CleanupOutcome::RecycleBinEmptied,
                2_000,
                None,
            ),
        ])
        .unwrap();

    let runs = store.cleanup_runs(None, 10).unwrap();

    assert_eq!(runs.len(), 2);
    // Mas reciente primero: la fila de ts 3 000 se inserto segunda (id 2).
    assert_eq!(runs[0].key, CleanupRunKey::Entry(2));
    assert_eq!(runs[0].emptied_bytes, 2_000);
    assert_eq!(runs[0].outcomes.recycle_bin_emptied, 1);
    assert_eq!(runs[0].moved_bytes, 0);
    assert_eq!(runs[1].key, CleanupRunKey::Entry(1));
    assert_eq!(runs[1].emptied_bytes, 1_000);
}

pub(crate) fn contract_cleanup_runs_empty_log_is_an_empty_list<S: Store>(make: impl Fn() -> S) {
    let store = make();
    assert!(store.cleanup_runs(None, 10).unwrap().is_empty());
}

pub(crate) fn contract_cleanup_runs_list_distinct_categories_and_keep_detail<S: Store>(
    make: impl Fn() -> S,
) {
    let store = make();
    let scan = Some(ScanId(3));
    let emptied = NewCleanupLogEntry {
        error_detail: Some("partial".into()),
        ..run_row(
            5_000,
            "recycle_bin",
            CleanupOutcome::RecycleBinEmptied,
            700,
            None,
        )
    };
    store
        .append_cleanup_log(&[
            run_row(4_000, "temp_user", CleanupOutcome::MovedToTrash, 10, scan),
            run_row(
                4_001,
                "browser_cache_edge",
                CleanupOutcome::MovedToTrash,
                20,
                scan,
            ),
            run_row(4_002, "temp_user", CleanupOutcome::SkippedCloud, 30, scan),
            run_row(
                4_003,
                "temp_user",
                CleanupOutcome::SkippedProtected,
                40,
                scan,
            ),
            run_row(4_004, "temp_user", CleanupOutcome::Error, 50, scan),
            emptied,
        ])
        .unwrap();

    let runs = store.cleanup_runs(None, 10).unwrap();

    assert_eq!(runs.len(), 2);
    let scan_run = runs
        .iter()
        .find(|run| run.key == CleanupRunKey::Scan(ScanId(3)))
        .unwrap();
    assert_eq!(scan_run.categories, ["browser_cache_edge", "temp_user"]);
    assert_eq!(scan_run.outcomes.skipped_cloud, 1);
    assert_eq!(scan_run.outcomes.skipped_protected, 1);
    assert_eq!(scan_run.outcomes.error, 1);
    assert_eq!(scan_run.empty_detail, None);
    let bin_run = runs
        .iter()
        .find(|run| matches!(run.key, CleanupRunKey::Entry(_)))
        .unwrap();
    assert_eq!(bin_run.empty_detail.as_deref(), Some("partial"));
    assert_eq!(bin_run.categories, ["recycle_bin"]);
}

/// Cursor de la ultima limpieza de una pagina.
fn cursor_after(run: &crate::domain::ports::store::CleanupRunRow) -> RunCursor {
    RunCursor {
        finished_at: run.finished_at,
        key: run.key,
    }
}

fn seed_five_runs<S: Store>(store: &S) {
    for scan in 1..=5_i64 {
        store
            .append_cleanup_log(&[run_row(
                scan * 1_000,
                "temp_user",
                CleanupOutcome::MovedToTrash,
                10,
                Some(ScanId(scan)),
            )])
            .unwrap();
    }
}

pub(crate) fn contract_cleanup_runs_page_by_cursor_without_repeats<S: Store>(make: impl Fn() -> S) {
    let store = make();
    seed_five_runs(&store);

    let first = store.cleanup_runs(None, 2).unwrap();
    let second = store
        .cleanup_runs(Some(cursor_after(&first[1])), 2)
        .unwrap();
    let third = store
        .cleanup_runs(Some(cursor_after(&second[1])), 2)
        .unwrap();

    let keys = |page: &[crate::domain::ports::store::CleanupRunRow]| {
        page.iter().map(|run| run.key).collect::<Vec<_>>()
    };
    assert_eq!(
        keys(&first),
        [
            CleanupRunKey::Scan(ScanId(5)),
            CleanupRunKey::Scan(ScanId(4))
        ]
    );
    assert_eq!(
        keys(&second),
        [
            CleanupRunKey::Scan(ScanId(3)),
            CleanupRunKey::Scan(ScanId(2))
        ]
    );
    assert_eq!(keys(&third), [CleanupRunKey::Scan(ScanId(1))]);
    assert!(
        store
            .cleanup_runs(Some(cursor_after(&third[0])), 2)
            .unwrap()
            .is_empty()
    );
}

pub(crate) fn contract_cleanup_runs_same_timestamp_has_a_stable_order<S: Store>(
    make: impl Fn() -> S,
) {
    let store = make();
    // Dos limpiezas de escaneo y una fila suelta, todas con la misma fecha maxima.
    store
        .append_cleanup_log(&[
            run_row(
                9_000,
                "temp_user",
                CleanupOutcome::MovedToTrash,
                1,
                Some(ScanId(10)),
            ),
            run_row(
                9_000,
                "temp_user",
                CleanupOutcome::MovedToTrash,
                2,
                Some(ScanId(20)),
            ),
            run_row(
                9_000,
                "recycle_bin",
                CleanupOutcome::RecycleBinEmptied,
                3,
                None,
            ),
        ])
        .unwrap();

    let all = store.cleanup_runs(None, 10).unwrap();
    let keys: Vec<_> = all.iter().map(|run| run.key).collect();
    assert_eq!(
        keys,
        [
            CleanupRunKey::Scan(ScanId(20)),
            CleanupRunKey::Scan(ScanId(10)),
            CleanupRunKey::Entry(3),
        ]
    );

    // Paginando de a una no se pierde ninguna ni se repite.
    let mut seen = Vec::new();
    let mut cursor = None;
    loop {
        let page = store.cleanup_runs(cursor, 1).unwrap();
        let Some(run) = page.first() else { break };
        seen.push(run.key);
        cursor = Some(cursor_after(run));
    }
    assert_eq!(seen, keys);
}

pub(crate) fn contract_cleanup_runs_zero_limit_is_empty<S: Store>(make: impl Fn() -> S) {
    let store = make();
    seed_five_runs(&store);
    assert!(store.cleanup_runs(None, 0).unwrap().is_empty());
}

/// Documenta el comportamiento: una limpieza con filas nuevas sube al principio (su fecha
/// maxima cambia), por lo que ya no queda "despues" del cursor viejo.
pub(crate) fn contract_cleanup_runs_new_rows_move_a_run_to_the_top<S: Store>(make: impl Fn() -> S) {
    let store = make();
    seed_five_runs(&store);
    store
        .append_cleanup_log(&[run_row(
            9_999,
            "duplicates",
            CleanupOutcome::MovedToTrash,
            5,
            Some(ScanId(1)),
        )])
        .unwrap();

    let top = store.cleanup_runs(None, 1).unwrap();

    assert_eq!(top[0].key, CleanupRunKey::Scan(ScanId(1)));
    assert_eq!(top[0].entry_count, 2);
    assert_eq!(top[0].categories, ["duplicates", "temp_user"]);
}

pub(crate) fn contract_cleanup_run_entries_page_in_insertion_order<S: Store>(make: impl Fn() -> S) {
    let store = make();
    let scan = Some(ScanId(7));
    let rows: Vec<NewCleanupLogEntry> = (0..250)
        .map(|n| {
            run_row(
                1_000 + n,
                "temp_user",
                CleanupOutcome::MovedToTrash,
                10,
                scan,
            )
        })
        .collect();
    store.append_cleanup_log(&rows).unwrap();

    let (total, page) = store
        .cleanup_run_entries(CleanupRunKey::Scan(ScanId(7)), 200, 100)
        .unwrap();

    assert_eq!(total, 250);
    assert_eq!(page.len(), 50);
    assert_eq!(page[0].path, rows[200].path);
    assert_eq!(page[49].path, rows[249].path);
    assert!(page.windows(2).all(|pair| pair[0].id < pair[1].id));
}

pub(crate) fn contract_cleanup_run_entries_entry_key_returns_only_that_row<S: Store>(
    make: impl Fn() -> S,
) {
    let store = make();
    store
        .append_cleanup_log(&[
            run_row(
                1_000,
                "temp_user",
                CleanupOutcome::MovedToTrash,
                5,
                Some(ScanId(1)),
            ),
            run_row(
                2_000,
                "recycle_bin",
                CleanupOutcome::RecycleBinEmptied,
                900,
                None,
            ),
            run_row(
                3_000,
                "recycle_bin",
                CleanupOutcome::RecycleBinEmptied,
                800,
                None,
            ),
        ])
        .unwrap();

    let (total, page) = store
        .cleanup_run_entries(CleanupRunKey::Entry(3), 0, 100)
        .unwrap();

    assert_eq!(total, 1);
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].id, 3);
    assert_eq!(page[0].size_bytes, 800);
    // Un identificador de fila que pertenece a un escaneo no es una limpieza `Entry`.
    let (total, page) = store
        .cleanup_run_entries(CleanupRunKey::Entry(1), 0, 100)
        .unwrap();
    assert_eq!((total, page.len()), (0, 0));
}

pub(crate) fn contract_cleanup_run_entries_unknown_key_is_an_empty_page<S: Store>(
    make: impl Fn() -> S,
) {
    let store = make();
    store
        .append_cleanup_log(&[run_row(
            1_000,
            "temp_user",
            CleanupOutcome::MovedToTrash,
            5,
            Some(ScanId(1)),
        )])
        .unwrap();

    for key in [CleanupRunKey::Scan(ScanId(999)), CleanupRunKey::Entry(999)] {
        let (total, page) = store.cleanup_run_entries(key, 0, 100).unwrap();
        assert_eq!(total, 0, "{key:?}");
        assert!(page.is_empty(), "{key:?}");
    }
}

pub(crate) fn contract_cleanup_run_entries_never_mix_other_scans<S: Store>(make: impl Fn() -> S) {
    let store = make();
    store
        .append_cleanup_log(&[
            run_row(
                1_000,
                "temp_user",
                CleanupOutcome::MovedToTrash,
                1,
                Some(ScanId(1)),
            ),
            run_row(
                1_001,
                "temp_user",
                CleanupOutcome::MovedToTrash,
                2,
                Some(ScanId(2)),
            ),
            run_row(
                1_002,
                "temp_user",
                CleanupOutcome::SkippedLocked,
                3,
                Some(ScanId(1)),
            ),
            run_row(
                1_003,
                "recycle_bin",
                CleanupOutcome::RecycleBinEmptied,
                4,
                None,
            ),
        ])
        .unwrap();

    let (total, page) = store
        .cleanup_run_entries(CleanupRunKey::Scan(ScanId(1)), 0, 100)
        .unwrap();

    assert_eq!(total, 2);
    let sizes: Vec<u64> = page.iter().map(|entry| entry.size_bytes).collect();
    assert_eq!(sizes, [1, 3]);
    let (_, limited) = store
        .cleanup_run_entries(CleanupRunKey::Scan(ScanId(1)), 1, 1)
        .unwrap();
    assert_eq!(limited.len(), 1);
    assert_eq!(limited[0].size_bytes, 3);
}

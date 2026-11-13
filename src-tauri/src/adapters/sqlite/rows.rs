//! Conversiones puras fila -> modelo de dominio.

use crate::domain::model::{
    CleanupLogEntry, CleanupOutcome, EpochMillis, Scan, ScanId, ScanKind, ScanStatus, ScanTrigger,
};
use crate::domain::ports::StoreError;

fn corrupt(what: &str, raw: &str) -> StoreError {
    StoreError::Corrupt(format!("unknown {what} '{raw}'"))
}

/// Arma un `Scan` desde los valores crudos de una fila; un enum desconocido es `Corrupt`.
pub(super) fn scan_from_parts(
    id: i64,
    kind: &str,
    trigger: &str,
    status: &str,
    started_at: i64,
    finished_at: Option<i64>,
) -> Result<Scan, StoreError> {
    Ok(Scan {
        id: ScanId(id),
        kind: ScanKind::parse(kind).ok_or_else(|| corrupt("scan kind", kind))?,
        trigger: ScanTrigger::parse(trigger).ok_or_else(|| corrupt("scan trigger", trigger))?,
        status: ScanStatus::parse(status).ok_or_else(|| corrupt("scan status", status))?,
        started_at: EpochMillis(started_at),
        finished_at: finished_at.map(EpochMillis),
    })
}

/// Valores crudos de una fila de `cleanup_log`.
pub(super) struct RawLogRow {
    pub id: i64,
    pub ts: i64,
    pub category: String,
    pub path: String,
    pub size_bytes: i64,
    pub outcome: String,
    pub scan_id: Option<i64>,
    pub error_detail: Option<String>,
}

/// Arma un `CleanupLogEntry`; resultado desconocido o tamano negativo es `Corrupt`.
pub(super) fn log_entry_from_parts(row: RawLogRow) -> Result<CleanupLogEntry, StoreError> {
    Ok(CleanupLogEntry {
        id: row.id,
        ts: EpochMillis(row.ts),
        category: row.category,
        path: row.path,
        size_bytes: u64::try_from(row.size_bytes)
            .map_err(|_| StoreError::Corrupt("negative size_bytes".into()))?,
        outcome: CleanupOutcome::parse(&row.outcome)
            .ok_or_else(|| corrupt("cleanup outcome", &row.outcome))?,
        scan_id: row.scan_id.map(ScanId),
        error_detail: row.error_detail,
    })
}

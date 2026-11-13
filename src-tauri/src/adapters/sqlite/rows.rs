//! Conversiones puras fila -> modelo de dominio.

use crate::domain::junk::category::{CategoryScope, JunkCategory};
use crate::domain::junk::model::{
    CategoryResult, CategoryStatus, FindingState, JunkFinding, StoredFinding,
};
use crate::domain::model::{
    CleanupLogEntry, CleanupOutcome, EpochMillis, Scan, ScanId, ScanKind, ScanStatus, ScanTrigger,
};
use crate::domain::ports::{FileIdentity, StoreError};
use crate::domain::privileges::MismatchReason;

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

fn non_negative(what: &str, raw: i64) -> Result<u64, StoreError> {
    u64::try_from(raw).map_err(|_| StoreError::Corrupt(format!("negative {what}")))
}

/// Valores crudos de una fila de `junk_finding` (la categoria ya la fija la consulta).
pub(super) struct RawFindingRow {
    pub id: i64,
    pub category: JunkCategory,
    pub path: String,
    pub size_bytes: i64,
    pub modified_at: Option<i64>,
    pub skip_reason: Option<String>,
    pub volume_serial: Option<i64>,
    pub file_id: Option<String>,
}

/// Arma un hallazgo guardado; identidad ausente en un elegible o inconsistente es `Corrupt`.
pub(super) fn finding_from_parts(row: RawFindingRow) -> Result<StoredFinding, StoreError> {
    let state = match (row.skip_reason, row.volume_serial, row.file_id) {
        (Some(reason), _, _) => FindingState::Skipped(reason),
        (None, Some(volume), Some(file_id)) => FindingState::Eligible(FileIdentity {
            volume_serial: u64::from_ne_bytes(volume.to_ne_bytes()),
            file_id: u128::from_str_radix(&file_id, 16)
                .map_err(|_| StoreError::Corrupt("invalid file_id".into()))?,
        }),
        (None, _, _) => {
            return Err(StoreError::Corrupt(
                "eligible finding without identity".into(),
            ));
        }
    };
    Ok(StoredFinding {
        id: row.id,
        finding: JunkFinding {
            category: row.category,
            path: row.path,
            size_bytes: non_negative("size_bytes", row.size_bytes)?,
            modified_at: row.modified_at.map(EpochMillis),
            state,
        },
    })
}

/// Valores crudos de una fila de `junk_category_result`.
pub(super) struct RawCategoryRow {
    pub category: String,
    pub scope: String,
    pub status: String,
    pub block_reason: Option<String>,
    pub item_count: i64,
    pub size_bytes: i64,
    pub skipped_count: i64,
    pub inaccessible_count: i64,
}

/// Arma un resumen de categoria; un valor desconocido o un contador negativo es `Corrupt`.
pub(super) fn category_result_from_parts(
    row: RawCategoryRow,
) -> Result<CategoryResult, StoreError> {
    let block_reason = match row.block_reason.as_deref() {
        None => None,
        Some(raw) => Some(MismatchReason::parse(raw).ok_or_else(|| corrupt("block reason", raw))?),
    };
    Ok(CategoryResult {
        category: JunkCategory::parse(&row.category)
            .ok_or_else(|| corrupt("junk category", &row.category))?,
        scope: CategoryScope::parse(&row.scope).ok_or_else(|| corrupt("junk scope", &row.scope))?,
        status: CategoryStatus::parse(&row.status)
            .ok_or_else(|| corrupt("junk status", &row.status))?,
        block_reason,
        item_count: non_negative("item_count", row.item_count)?,
        size_bytes: non_negative("size_bytes", row.size_bytes)?,
        skipped_count: non_negative("skipped_count", row.skipped_count)?,
        inaccessible_count: non_negative("inaccessible_count", row.inaccessible_count)?,
    })
}

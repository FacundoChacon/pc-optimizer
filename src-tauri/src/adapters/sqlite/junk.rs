//! Hallazgos de basura sobre SQLite (C-08, D-02, D-04): guardado atomico, ultimo escaneo
//! terminado y consulta paginada. Solo parametros enlazados: ninguna entrada se concatena en SQL.

use rusqlite::{OptionalExtension, TransactionBehavior, params};

use super::SqliteStore;
use super::errors::map_sqlite_error;
use super::rows::{
    RawCategoryRow, RawFindingRow, category_result_from_parts, finding_from_parts, scan_from_parts,
};
use crate::domain::junk::category::JunkCategory;
use crate::domain::junk::model::{
    CategoryResult, FindingState, JunkFinding, JunkFindingPage, JunkScanResults, JunkScanSummary,
    StoredFinding,
};
use crate::domain::model::{EpochMillis, ScanId};
use crate::domain::ports::{JunkFindingStore, StoreError};

const SQL_SCAN_KIND_STATUS: &str = "SELECT kind, status FROM scan WHERE id = ?1";
/// Elimina los hallazgos de todo escaneo de basura distinto del que se esta guardando; su
/// resumen por categoria se conserva.
const SQL_PURGE_PREVIOUS_FINDINGS: &str = "DELETE FROM junk_finding WHERE scan_id IN \
     (SELECT id FROM scan WHERE kind = 'junk' AND id <> ?1)";
const SQL_INSERT_FINDING: &str = "INSERT INTO junk_finding(scan_id, category, path, size_bytes, \
     modified_at, protected_flag, skip_reason, volume_serial, file_id) \
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)";
const SQL_INSERT_CATEGORY: &str = "INSERT INTO junk_category_result(scan_id, category, scope, \
     status, block_reason, item_count, size_bytes, skipped_count, inaccessible_count) \
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)";
const SQL_FINISH_DONE: &str =
    "UPDATE scan SET status = 'done', finished_at = ?1 WHERE id = ?2 AND status = 'running'";
const SQL_LAST_DONE: &str = "SELECT id, kind, triggered_by, status, started_at, finished_at \
     FROM scan WHERE kind = 'junk' AND status = 'done' ORDER BY started_at DESC, id DESC LIMIT 1";
const SQL_CATEGORIES: &str = "SELECT category, scope, status, block_reason, item_count, \
     size_bytes, skipped_count, inaccessible_count FROM junk_category_result WHERE scan_id = ?1";
const SQL_COUNT_FINDINGS: &str =
    "SELECT count(*) FROM junk_finding WHERE scan_id = ?1 AND category = ?2";
const SQL_LIST_FINDINGS: &str = "SELECT id, path, size_bytes, modified_at, skip_reason, \
     volume_serial, file_id FROM junk_finding WHERE scan_id = ?1 AND category = ?2 \
     ORDER BY size_bytes DESC, id ASC LIMIT ?3 OFFSET ?4";

fn to_i64(value: u64, what: &str) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| StoreError::Constraint(format!("{what} exceeds i64::MAX")))
}

/// Columnas `(skip_reason, protected, volume_serial, file_id)` de un hallazgo. El volumen (u64)
/// se guarda como i64 por reinterpretacion de bits y el id de archivo (u128) como hex de 32.
fn state_columns(finding: &JunkFinding) -> (Option<&str>, i64, Option<i64>, Option<String>) {
    match &finding.state {
        FindingState::Skipped(reason) => (Some(reason.as_str()), 1, None, None),
        FindingState::Eligible(identity) => (
            None,
            0,
            Some(i64::from_ne_bytes(identity.volume_serial.to_ne_bytes())),
            Some(format!("{:032x}", identity.file_id)),
        ),
    }
}

fn insert_finding(
    stmt: &mut rusqlite::Statement<'_>,
    scan_id: ScanId,
    finding: &JunkFinding,
) -> Result<(), StoreError> {
    let (skip_reason, protected, volume_serial, file_id) = state_columns(finding);
    stmt.execute(params![
        scan_id.0,
        finding.category.as_str(),
        finding.path,
        to_i64(finding.size_bytes, "size_bytes")?,
        finding.modified_at.map(|at| at.0),
        protected,
        skip_reason,
        volume_serial,
        file_id,
    ])
    .map_err(map_sqlite_error)?;
    Ok(())
}

fn insert_category(
    stmt: &mut rusqlite::Statement<'_>,
    scan_id: ScanId,
    result: &CategoryResult,
) -> Result<(), StoreError> {
    stmt.execute(params![
        scan_id.0,
        result.category.as_str(),
        result.scope.as_str(),
        result.status.as_str(),
        result.block_reason.map(|reason| reason.as_str()),
        to_i64(result.item_count, "item_count")?,
        to_i64(result.size_bytes, "size_bytes")?,
        to_i64(result.skipped_count, "skipped_count")?,
        to_i64(result.inaccessible_count, "inaccessible_count")?,
    ])
    .map_err(map_sqlite_error)?;
    Ok(())
}

impl JunkFindingStore for SqliteStore {
    fn complete_junk_scan(
        &self,
        scan_id: ScanId,
        results: &JunkScanResults,
        finished_at: EpochMillis,
    ) -> Result<(), StoreError> {
        let mut conn = self.conn()?;
        // Si algo falla, el `Drop` de la transaccion revierte todo: el escaneo sigue `running`.
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite_error)?;
        let (kind, status): (String, String) = tx
            .query_row(SQL_SCAN_KIND_STATUS, [scan_id.0], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .optional()
            .map_err(map_sqlite_error)?
            .ok_or_else(|| StoreError::NotFound(format!("scan {}", scan_id.0)))?;
        if kind != "junk" || status != "running" {
            return Err(StoreError::InvalidState(format!(
                "scan {} is not a running junk scan",
                scan_id.0
            )));
        }
        tx.execute(SQL_PURGE_PREVIOUS_FINDINGS, [scan_id.0])
            .map_err(map_sqlite_error)?;
        {
            let mut insert = tx.prepare(SQL_INSERT_FINDING).map_err(map_sqlite_error)?;
            for finding in &results.findings {
                insert_finding(&mut insert, scan_id, finding)?;
            }
            let mut insert = tx.prepare(SQL_INSERT_CATEGORY).map_err(map_sqlite_error)?;
            for result in &results.categories {
                insert_category(&mut insert, scan_id, result)?;
            }
        }
        tx.execute(SQL_FINISH_DONE, params![finished_at.0, scan_id.0])
            .map_err(map_sqlite_error)?;
        tx.commit().map_err(map_sqlite_error)
    }

    fn last_done_junk_scan(&self) -> Result<Option<JunkScanSummary>, StoreError> {
        let conn = self.conn()?;
        let scan = conn
            .query_row(SQL_LAST_DONE, [], |row| {
                Ok(scan_from_parts(
                    row.get(0)?,
                    &row.get::<_, String>(1)?,
                    &row.get::<_, String>(2)?,
                    &row.get::<_, String>(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            })
            .optional()
            .map_err(map_sqlite_error)?
            .transpose()?;
        let Some(scan) = scan else {
            return Ok(None);
        };
        let mut stmt = conn.prepare(SQL_CATEGORIES).map_err(map_sqlite_error)?;
        let rows = stmt
            .query_map([scan.id.0], |row| {
                Ok(category_result_from_parts(RawCategoryRow {
                    category: row.get(0)?,
                    scope: row.get(1)?,
                    status: row.get(2)?,
                    block_reason: row.get(3)?,
                    item_count: row.get(4)?,
                    size_bytes: row.get(5)?,
                    skipped_count: row.get(6)?,
                    inaccessible_count: row.get(7)?,
                }))
            })
            .map_err(map_sqlite_error)?;
        let mut categories = rows
            .map(|row| row.map_err(map_sqlite_error)?)
            .collect::<Result<Vec<_>, _>>()?;
        categories.sort_by_key(|result| {
            JunkCategory::ALL
                .iter()
                .position(|known| *known == result.category)
        });
        Ok(Some(JunkScanSummary { scan, categories }))
    }

    fn list_junk_findings(
        &self,
        scan_id: ScanId,
        category: JunkCategory,
        offset: u64,
        limit: u32,
    ) -> Result<JunkFindingPage, StoreError> {
        let conn = self.conn()?;
        let total: i64 = conn
            .query_row(
                SQL_COUNT_FINDINGS,
                params![scan_id.0, category.as_str()],
                |row| row.get(0),
            )
            .map_err(map_sqlite_error)?;
        let total = u64::try_from(total)
            .map_err(|_| StoreError::Corrupt("negative finding count".into()))?;
        let offset = i64::try_from(offset).unwrap_or(i64::MAX);
        let mut stmt = conn.prepare(SQL_LIST_FINDINGS).map_err(map_sqlite_error)?;
        let rows = stmt
            .query_map(
                params![scan_id.0, category.as_str(), i64::from(limit), offset],
                |row| {
                    Ok(finding_from_parts(RawFindingRow {
                        id: row.get(0)?,
                        category,
                        path: row.get(1)?,
                        size_bytes: row.get(2)?,
                        modified_at: row.get(3)?,
                        skip_reason: row.get(4)?,
                        volume_serial: row.get(5)?,
                        file_id: row.get(6)?,
                    }))
                },
            )
            .map_err(map_sqlite_error)?;
        let items = rows
            .map(|row| row.map_err(map_sqlite_error)?)
            .collect::<Result<Vec<StoredFinding>, _>>()?;
        Ok(JunkFindingPage { total, items })
    }
}

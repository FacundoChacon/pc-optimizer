//! Lectura agrupada del log de limpieza (C-12, D-09/D-10): solo `SELECT`, sin migracion.

use rusqlite::{Connection, params};

use super::errors::map_sqlite_error;
use super::rows::{RawLogRow, log_entry_from_parts};
use crate::domain::model::ScanId;
use crate::domain::model::{CleanupLogEntry, EpochMillis};
use crate::domain::ports::{CleanupRunKey, CleanupRunRow, OutcomeCounts, RunCursor, StoreError};

/// Tipo de clave en la base: 0 = fila sin escaneo (`Entry`), 1 = escaneo (`Scan`). Es el mismo
/// orden que `CleanupRunKey::cmp`.
const KIND_ENTRY: i64 = 0;
const KIND_SCAN: i64 = 1;

/// Limpiezas agrupadas por `scan_id` (o por `id` si es nulo), mas recientes primero.
/// `?1` fecha del cursor (o NULL), `?2` tipo, `?3` identificador, `?4` limite.
const SQL_RUNS: &str = "\
WITH runs AS ( \
  SELECT CASE WHEN scan_id IS NULL THEN 0 ELSE 1 END AS kind, \
         COALESCE(scan_id, id) AS key_id, \
         min(ts) AS started_at, max(ts) AS finished_at \
  FROM cleanup_log \
  GROUP BY kind, key_id \
), page AS ( \
  SELECT kind, key_id, started_at, finished_at FROM runs \
  WHERE ?1 IS NULL OR (finished_at, kind, key_id) < (?1, ?2, ?3) \
  ORDER BY finished_at DESC, kind DESC, key_id DESC \
  LIMIT ?4 \
) \
SELECT p.kind, p.key_id, p.started_at, p.finished_at, count(*), \
       sum(l.outcome = 'moved_to_trash'), \
       sum(l.outcome = 'skipped_locked'), \
       sum(l.outcome = 'skipped_protected'), \
       sum(l.outcome = 'skipped_cloud'), \
       sum(l.outcome = 'error'), \
       sum(l.outcome = 'recycle_bin_emptied'), \
       sum(CASE WHEN l.outcome = 'moved_to_trash' THEN l.size_bytes ELSE 0 END), \
       sum(CASE WHEN l.outcome = 'recycle_bin_emptied' THEN l.size_bytes ELSE 0 END), \
       max(CASE WHEN l.outcome = 'recycle_bin_emptied' THEN l.error_detail END), \
       group_concat(DISTINCT l.category) \
FROM cleanup_log l \
JOIN page p ON p.kind = (l.scan_id IS NOT NULL) AND p.key_id = COALESCE(l.scan_id, l.id) \
GROUP BY p.kind, p.key_id \
ORDER BY p.finished_at DESC, p.kind DESC, p.key_id DESC";

/// Filtro de una limpieza: `?1` tipo (1 = escaneo, 0 = fila sin escaneo) y `?2` identificador.
const SQL_RUN_FILTER: &str =
    "((?1 = 1 AND scan_id = ?2) OR (?1 = 0 AND scan_id IS NULL AND id = ?2))";

fn count(what: &str, raw: i64) -> Result<u64, StoreError> {
    u64::try_from(raw).map_err(|_| StoreError::Corrupt(format!("negative {what}")))
}

fn key_parts(key: CleanupRunKey) -> (i64, i64) {
    match key {
        CleanupRunKey::Entry(id) => (KIND_ENTRY, id),
        CleanupRunKey::Scan(id) => (KIND_SCAN, id.0),
    }
}

pub(super) fn cleanup_runs(
    conn: &Connection,
    before: Option<RunCursor>,
    limit: u32,
) -> Result<Vec<CleanupRunRow>, StoreError> {
    let (cursor_ts, cursor_kind, cursor_id) = match before {
        Some(cursor) => {
            let (kind, id) = key_parts(cursor.key);
            (Some(cursor.finished_at.0), kind, id)
        }
        None => (None, KIND_ENTRY, 0),
    };
    let mut stmt = conn.prepare(SQL_RUNS).map_err(map_sqlite_error)?;
    let rows = stmt
        .query_map(
            params![cursor_ts, cursor_kind, cursor_id, i64::from(limit)],
            |row| {
                let mut ints = [0_i64; 13];
                for (index, slot) in ints.iter_mut().enumerate() {
                    *slot = row.get(index)?;
                }
                Ok((
                    ints,
                    row.get::<_, Option<String>>(13)?,
                    row.get::<_, Option<String>>(14)?,
                ))
            },
        )
        .map_err(map_sqlite_error)?;
    rows.map(|row| {
        let (n, empty_detail, categories) = row.map_err(map_sqlite_error)?;
        run_from_parts(n, empty_detail, categories)
    })
    .collect()
}

/// `n`: kind, key_id, started, finished, entries, moved, locked, protected, cloud, errors,
/// emptied, moved_bytes, emptied_bytes.
fn run_from_parts(
    n: [i64; 13],
    empty_detail: Option<String>,
    categories: Option<String>,
) -> Result<CleanupRunRow, StoreError> {
    let key = match n[0] {
        KIND_ENTRY => CleanupRunKey::Entry(n[1]),
        _ => CleanupRunKey::Scan(ScanId(n[1])),
    };
    let mut categories: Vec<String> = categories
        .map(|joined| joined.split(',').map(str::to_owned).collect())
        .unwrap_or_default();
    categories.sort();
    categories.dedup();
    Ok(CleanupRunRow {
        key,
        started_at: EpochMillis(n[2]),
        finished_at: EpochMillis(n[3]),
        entry_count: count("entry_count", n[4])?,
        outcomes: OutcomeCounts {
            moved_to_trash: count("moved", n[5])?,
            skipped_locked: count("locked", n[6])?,
            skipped_protected: count("protected", n[7])?,
            skipped_cloud: count("cloud", n[8])?,
            error: count("errors", n[9])?,
            recycle_bin_emptied: count("emptied", n[10])?,
        },
        moved_bytes: count("moved_bytes", n[11])?,
        emptied_bytes: count("emptied_bytes", n[12])?,
        empty_detail,
        categories,
    })
}

pub(super) fn cleanup_run_entries(
    conn: &Connection,
    key: CleanupRunKey,
    offset: u64,
    limit: u32,
) -> Result<(u64, Vec<CleanupLogEntry>), StoreError> {
    let (kind, id) = key_parts(key);
    let total: i64 = conn
        .query_row(
            &format!("SELECT count(*) FROM cleanup_log WHERE {SQL_RUN_FILTER}"),
            params![kind, id],
            |row| row.get(0),
        )
        .map_err(map_sqlite_error)?;
    let sql = format!(
        "SELECT id, ts, category, path, size_bytes, outcome, scan_id, error_detail \
         FROM cleanup_log WHERE {SQL_RUN_FILTER} ORDER BY id LIMIT ?3 OFFSET ?4"
    );
    let mut stmt = conn.prepare(&sql).map_err(map_sqlite_error)?;
    let rows = stmt
        .query_map(
            params![
                kind,
                id,
                i64::from(limit),
                i64::try_from(offset).unwrap_or(i64::MAX)
            ],
            |row| {
                Ok(log_entry_from_parts(RawLogRow {
                    id: row.get(0)?,
                    ts: row.get(1)?,
                    category: row.get(2)?,
                    path: row.get(3)?,
                    size_bytes: row.get(4)?,
                    outcome: row.get(5)?,
                    scan_id: row.get(6)?,
                    error_detail: row.get(7)?,
                }))
            },
        )
        .map_err(map_sqlite_error)?;
    let page = rows
        .map(|row| row.map_err(map_sqlite_error)?)
        .collect::<Result<Vec<_>, _>>()?;
    Ok((count("total", total)?, page))
}

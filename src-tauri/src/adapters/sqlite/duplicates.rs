//! Duplicados sobre SQLite (C-11, D-08, D-09, D-17): guardado atomico, ultimo escaneo terminado,
//! consulta paginada y resolucion de grupos. Solo parametros enlazados: ninguna entrada se
//! concatena en SQL.
//!
//! La marca `kept` de una copia SOLO la escribe `resolve_duplicate_groups`; el test de
//! arquitectura `kept_should_only_be_assigned_by_the_group_resolution` lo vigila.

use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};

use super::SqliteStore;
use super::errors::map_sqlite_error;
use super::rows::scan_from_parts;
use crate::domain::duplicates::model::{
    DuplicateFile, DuplicateGroupPage, DuplicateScanResults, DuplicateScanSummary, GroupResolution,
    MinSize, RootResult, RootStatus, ScanCounters, StoredDuplicateFile, StoredDuplicateGroup,
};
use crate::domain::model::{EpochMillis, ScanId};
use crate::domain::ports::{DuplicateStore, FileIdentity, StoreError};
use crate::domain::protection::context::ContentFolder;

const SQL_SCAN_KIND_STATUS: &str = "SELECT kind, status FROM scan WHERE id = ?1";
/// Elimina los grupos (y sus copias, en cascada) de todo escaneo de duplicados distinto del que
/// se esta guardando; su escaneo y su resultado se conservan.
const SQL_PURGE_PREVIOUS_GROUPS: &str = "DELETE FROM duplicate_group WHERE scan_id IN \
     (SELECT id FROM scan WHERE kind = 'duplicates' AND id <> ?1)";
const SQL_INSERT_RESULT: &str = "INSERT INTO duplicate_scan_result(scan_id, min_size_bytes, \
     files_seen, below_min_size, protected_count, cloud_count, hard_linked_count, changed_count, \
     unreadable_count, inaccessible_dirs, excluded_dirs, hashed_bytes) \
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)";
const SQL_INSERT_ROOT: &str = "INSERT INTO duplicate_scan_root(scan_id, root, status, reason, \
     files_seen) VALUES (?1, ?2, ?3, ?4, ?5)";
const SQL_INSERT_GROUP: &str = "INSERT INTO duplicate_group(scan_id, size_bytes, blake3, \
     file_count) VALUES (?1, ?2, ?3, ?4)";
/// Sin la columna `kept`: toda copia nace sin conservar (por defecto 0).
const SQL_INSERT_FILE: &str = "INSERT INTO duplicate_file(group_id, root, path, mtime, \
     volume_serial, file_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6)";
const SQL_FINISH_DONE: &str =
    "UPDATE scan SET status = 'done', finished_at = ?1 WHERE id = ?2 AND status = 'running'";
const SQL_LAST_DONE: &str = "SELECT id, kind, triggered_by, status, started_at, finished_at \
     FROM scan WHERE kind = 'duplicates' AND status = 'done' \
     ORDER BY started_at DESC, id DESC LIMIT 1";
const SQL_RESULT: &str = "SELECT min_size_bytes, files_seen, below_min_size, protected_count, \
     cloud_count, hard_linked_count, changed_count, unreadable_count, inaccessible_dirs, \
     excluded_dirs, hashed_bytes FROM duplicate_scan_result WHERE scan_id = ?1";
const SQL_ROOTS: &str =
    "SELECT root, status, reason, files_seen FROM duplicate_scan_root WHERE scan_id = ?1";
/// Grupos, copias, espacio recuperable de los grupos sin resolver y cantidad sin resolver.
const SQL_GROUP_TOTALS: &str = "SELECT count(*), coalesce(sum(file_count), 0), \
     coalesce(sum(CASE WHEN resolved_at IS NULL THEN size_bytes * (file_count - 1) ELSE 0 END), 0), \
     coalesce(sum(CASE WHEN resolved_at IS NULL THEN 1 ELSE 0 END), 0) \
     FROM duplicate_group WHERE scan_id = ?1";
/// Filtros opcionales: `?2` carpeta, `?3` tamano minimo (NULL = sin filtro).
const SQL_GROUP_FILTER: &str = "g.scan_id = ?1 \
     AND (?2 IS NULL OR EXISTS (SELECT 1 FROM duplicate_file f WHERE f.group_id = g.id AND f.root = ?2)) \
     AND (?3 IS NULL OR g.size_bytes >= ?3)";
const SQL_GROUP_COLUMNS: &str = "SELECT g.id, g.size_bytes, g.blake3, g.resolved_at, g.file_count \
     FROM duplicate_group g";
const SQL_FILES_PREFIX: &str = "SELECT id, group_id, root, path, mtime, volume_serial, file_id, \
     kept FROM duplicate_file WHERE group_id IN (";
const SQL_GROUP_OWNER: &str = "SELECT scan_id, resolved_at FROM duplicate_group WHERE id = ?1";
const SQL_MARK_KEPT: &str = "UPDATE duplicate_file SET kept = 1 WHERE id = ?1 AND group_id = ?2";
const SQL_MARK_RESOLVED: &str =
    "UPDATE duplicate_group SET resolved_at = ?1 WHERE id = ?2 AND resolved_at IS NULL";

fn to_i64(value: u64, what: &str) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| StoreError::Constraint(format!("{what} exceeds i64::MAX")))
}

fn non_negative(what: &str, raw: i64) -> Result<u64, StoreError> {
    u64::try_from(raw).map_err(|_| StoreError::Corrupt(format!("negative {what}")))
}

fn corrupt(what: &str, raw: &str) -> StoreError {
    StoreError::Corrupt(format!("unknown {what} '{raw}'"))
}

fn insert_result(
    tx: &Transaction<'_>,
    scan_id: ScanId,
    r: &DuplicateScanResults,
) -> Result<(), StoreError> {
    let c = &r.counters;
    tx.execute(
        SQL_INSERT_RESULT,
        params![
            scan_id.0,
            to_i64(r.min_size.bytes(), "min_size_bytes")?,
            to_i64(c.files_seen, "files_seen")?,
            to_i64(c.below_min_size, "below_min_size")?,
            to_i64(c.protected_count, "protected_count")?,
            to_i64(c.cloud_count, "cloud_count")?,
            to_i64(c.hard_linked_count, "hard_linked_count")?,
            to_i64(c.changed_count, "changed_count")?,
            to_i64(c.unreadable_count, "unreadable_count")?,
            to_i64(c.inaccessible_dirs, "inaccessible_dirs")?,
            to_i64(c.excluded_dirs, "excluded_dirs")?,
            to_i64(c.hashed_bytes, "hashed_bytes")?,
        ],
    )
    .map_err(map_sqlite_error)?;
    let mut insert = tx.prepare(SQL_INSERT_ROOT).map_err(map_sqlite_error)?;
    for root in &r.roots {
        insert
            .execute(params![
                scan_id.0,
                root.folder.as_str(),
                root.status.as_str(),
                root.status.reason().map(|reason| reason.as_str()),
                to_i64(root.files_seen, "files_seen")?,
            ])
            .map_err(map_sqlite_error)?;
    }
    Ok(())
}

fn insert_groups(
    tx: &Transaction<'_>,
    scan_id: ScanId,
    r: &DuplicateScanResults,
) -> Result<(), StoreError> {
    let mut insert_group = tx.prepare(SQL_INSERT_GROUP).map_err(map_sqlite_error)?;
    let mut insert_file = tx.prepare(SQL_INSERT_FILE).map_err(map_sqlite_error)?;
    for group in &r.groups {
        insert_group
            .execute(params![
                scan_id.0,
                to_i64(group.size_bytes, "size_bytes")?,
                group.blake3,
                to_i64(group.files.len() as u64, "file_count")?,
            ])
            .map_err(map_sqlite_error)?;
        let group_id = tx.last_insert_rowid();
        for file in &group.files {
            insert_file
                .execute(params![
                    group_id,
                    file.root.as_str(),
                    file.path,
                    file.modified.map(|at| at.0),
                    // El volumen (u64) se guarda como i64 por reinterpretacion de bits y el id de
                    // archivo (u128) como hex de 32.
                    i64::from_ne_bytes(file.identity.volume_serial.to_ne_bytes()),
                    format!("{:032x}", file.identity.file_id),
                ])
                .map_err(map_sqlite_error)?;
        }
    }
    Ok(())
}

struct RawGroup {
    id: i64,
    size_bytes: i64,
    blake3: String,
    resolved_at: Option<i64>,
}

struct RawFile {
    id: i64,
    group_id: i64,
    root: String,
    path: String,
    mtime: Option<i64>,
    volume_serial: i64,
    file_id: String,
    kept: i64,
}

fn file_from_raw(raw: RawFile) -> Result<(i64, StoredDuplicateFile), StoreError> {
    let root =
        ContentFolder::parse(&raw.root).ok_or_else(|| corrupt("content folder", &raw.root))?;
    let file_id = u128::from_str_radix(&raw.file_id, 16)
        .map_err(|_| StoreError::Corrupt("invalid file_id".into()))?;
    Ok((
        raw.group_id,
        StoredDuplicateFile {
            id: raw.id,
            file: DuplicateFile {
                root,
                path: raw.path,
                modified: raw.mtime.map(EpochMillis),
                identity: FileIdentity {
                    volume_serial: u64::from_ne_bytes(raw.volume_serial.to_ne_bytes()),
                    file_id,
                },
            },
            kept: raw.kept == 1,
        },
    ))
}

/// Arma los grupos `raw` (en ese orden) con sus copias, consultando sus archivos.
fn load_groups(
    conn: &rusqlite::Connection,
    raw: Vec<RawGroup>,
) -> Result<Vec<StoredDuplicateGroup>, StoreError> {
    if raw.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = (1..=raw.len())
        .map(|n| format!("?{n}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!("{SQL_FILES_PREFIX}{placeholders}) ORDER BY path COLLATE NOCASE, id");
    let mut stmt = conn.prepare(&sql).map_err(map_sqlite_error)?;
    let rows = stmt
        .query_map(
            rusqlite::params_from_iter(raw.iter().map(|group| group.id)),
            |row| {
                Ok(RawFile {
                    id: row.get(0)?,
                    group_id: row.get(1)?,
                    root: row.get(2)?,
                    path: row.get(3)?,
                    mtime: row.get(4)?,
                    volume_serial: row.get(5)?,
                    file_id: row.get(6)?,
                    kept: row.get(7)?,
                })
            },
        )
        .map_err(map_sqlite_error)?;
    let mut files: Vec<(i64, StoredDuplicateFile)> = Vec::new();
    for row in rows {
        files.push(file_from_raw(row.map_err(map_sqlite_error)?)?);
    }
    raw.into_iter()
        .map(|group| {
            Ok(StoredDuplicateGroup {
                id: group.id,
                size_bytes: non_negative("size_bytes", group.size_bytes)?,
                blake3: group.blake3,
                resolved_at: group.resolved_at.map(EpochMillis),
                files: files
                    .iter()
                    .filter(|(group_id, _)| *group_id == group.id)
                    .map(|(_, file)| file.clone())
                    .collect(),
            })
        })
        .collect()
}

fn summary_from_conn(
    conn: &rusqlite::Connection,
    scan: crate::domain::model::Scan,
) -> Result<DuplicateScanSummary, StoreError> {
    let (min_size, counters) = conn
        .query_row(SQL_RESULT, [scan.id.0], |row| {
            let mut values = [0i64; 11];
            for (index, slot) in values.iter_mut().enumerate() {
                *slot = row.get(index)?;
            }
            Ok(values)
        })
        .optional()
        .map_err(map_sqlite_error)?
        .map(|v| {
            let min_size = u64::try_from(v[0])
                .ok()
                .and_then(MinSize::new)
                .ok_or_else(|| StoreError::Corrupt("invalid min_size_bytes".into()))?;
            let n = |i: usize, what: &str| non_negative(what, v[i]);
            Ok::<_, StoreError>((
                min_size,
                ScanCounters {
                    files_seen: n(1, "files_seen")?,
                    below_min_size: n(2, "below_min_size")?,
                    protected_count: n(3, "protected_count")?,
                    cloud_count: n(4, "cloud_count")?,
                    hard_linked_count: n(5, "hard_linked_count")?,
                    changed_count: n(6, "changed_count")?,
                    unreadable_count: n(7, "unreadable_count")?,
                    inaccessible_dirs: n(8, "inaccessible_dirs")?,
                    excluded_dirs: n(9, "excluded_dirs")?,
                    hashed_bytes: n(10, "hashed_bytes")?,
                },
            ))
        })
        .transpose()?
        .ok_or_else(|| StoreError::Corrupt("duplicate scan without result".into()))?;
    let mut stmt = conn.prepare(SQL_ROOTS).map_err(map_sqlite_error)?;
    let rows = stmt
        .query_map([scan.id.0], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })
        .map_err(map_sqlite_error)?;
    let mut roots = Vec::new();
    for row in rows {
        let (root, status, reason, files_seen) = row.map_err(map_sqlite_error)?;
        roots.push(RootResult {
            folder: ContentFolder::parse(&root).ok_or_else(|| corrupt("content folder", &root))?,
            status: RootStatus::parse(&status, reason.as_deref())
                .ok_or_else(|| corrupt("root status", &status))?,
            files_seen: non_negative("files_seen", files_seen)?,
        });
    }
    roots.sort_by_key(|root| root.folder);
    let (groups, files, reclaimable, open): (i64, i64, i64, i64) = conn
        .query_row(SQL_GROUP_TOTALS, [scan.id.0], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .map_err(map_sqlite_error)?;
    Ok(DuplicateScanSummary {
        scan,
        min_size,
        roots,
        counters,
        group_count: non_negative("group count", groups)?,
        duplicate_file_count: non_negative("file count", files)?,
        reclaimable_bytes: non_negative("reclaimable bytes", reclaimable)?,
        open_group_count: non_negative("open group count", open)?,
    })
}

impl DuplicateStore for SqliteStore {
    fn complete_duplicate_scan(
        &self,
        scan_id: ScanId,
        results: &DuplicateScanResults,
        finished_at: EpochMillis,
    ) -> Result<(), StoreError> {
        results
            .validate()
            .map_err(|reason| StoreError::Constraint(reason.to_string()))?;
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
        if kind != "duplicates" || status != "running" {
            return Err(StoreError::InvalidState(format!(
                "scan {} is not a running duplicates scan",
                scan_id.0
            )));
        }
        tx.execute(SQL_PURGE_PREVIOUS_GROUPS, [scan_id.0])
            .map_err(map_sqlite_error)?;
        insert_result(&tx, scan_id, results)?;
        insert_groups(&tx, scan_id, results)?;
        tx.execute(SQL_FINISH_DONE, params![finished_at.0, scan_id.0])
            .map_err(map_sqlite_error)?;
        tx.commit().map_err(map_sqlite_error)
    }

    fn last_done_duplicate_scan(&self) -> Result<Option<DuplicateScanSummary>, StoreError> {
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
        scan.map(|scan| summary_from_conn(&conn, scan)).transpose()
    }

    fn list_duplicate_groups(
        &self,
        scan_id: ScanId,
        offset: u64,
        limit: u32,
        root: Option<ContentFolder>,
        min_size: Option<u64>,
    ) -> Result<DuplicateGroupPage, StoreError> {
        let conn = self.conn()?;
        let root = root.map(|folder| folder.as_str());
        let min_size = min_size
            .map(|bytes| to_i64(bytes, "min_size"))
            .transpose()?;
        let total: i64 = conn
            .query_row(
                &format!("SELECT count(*) FROM duplicate_group g WHERE {SQL_GROUP_FILTER}"),
                params![scan_id.0, root, min_size],
                |row| row.get(0),
            )
            .map_err(map_sqlite_error)?;
        let offset = i64::try_from(offset).unwrap_or(i64::MAX);
        let sql = format!(
            "{SQL_GROUP_COLUMNS} WHERE {SQL_GROUP_FILTER} \
             ORDER BY g.size_bytes * (g.file_count - 1) DESC, g.id ASC LIMIT ?4 OFFSET ?5"
        );
        let mut stmt = conn.prepare(&sql).map_err(map_sqlite_error)?;
        let rows = stmt
            .query_map(
                params![scan_id.0, root, min_size, i64::from(limit), offset],
                |row| {
                    Ok(RawGroup {
                        id: row.get(0)?,
                        size_bytes: row.get(1)?,
                        blake3: row.get(2)?,
                        resolved_at: row.get(3)?,
                    })
                },
            )
            .map_err(map_sqlite_error)?;
        let raw = rows
            .map(|row| row.map_err(map_sqlite_error))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(DuplicateGroupPage {
            total: non_negative("group count", total)?,
            groups: load_groups(&conn, raw)?,
        })
    }

    fn get_duplicate_groups(
        &self,
        scan_id: ScanId,
        group_ids: &[i64],
    ) -> Result<Vec<StoredDuplicateGroup>, StoreError> {
        if group_ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = (2..group_ids.len() + 2)
            .map(|n| format!("?{n}"))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "{SQL_GROUP_COLUMNS} WHERE g.scan_id = ?1 AND g.id IN ({placeholders}) ORDER BY g.id"
        );
        let conn = self.conn()?;
        let mut stmt = conn.prepare(&sql).map_err(map_sqlite_error)?;
        let args = std::iter::once(scan_id.0).chain(group_ids.iter().copied());
        let rows = stmt
            .query_map(rusqlite::params_from_iter(args), |row| {
                Ok(RawGroup {
                    id: row.get(0)?,
                    size_bytes: row.get(1)?,
                    blake3: row.get(2)?,
                    resolved_at: row.get(3)?,
                })
            })
            .map_err(map_sqlite_error)?;
        let raw = rows
            .map(|row| row.map_err(map_sqlite_error))
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);
        load_groups(&conn, raw)
    }

    fn resolve_duplicate_groups(
        &self,
        scan_id: ScanId,
        resolutions: &[GroupResolution],
        now: EpochMillis,
    ) -> Result<(), StoreError> {
        let mut conn = self.conn()?;
        // Todo o nada: si una resolucion falla, el `Drop` de la transaccion revierte las demas.
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite_error)?;
        for resolution in resolutions {
            let (owner, resolved_at): (i64, Option<i64>) = tx
                .query_row(SQL_GROUP_OWNER, [resolution.group_id], |row| {
                    Ok((row.get(0)?, row.get(1)?))
                })
                .optional()
                .map_err(map_sqlite_error)?
                .ok_or_else(|| StoreError::NotFound(format!("group {}", resolution.group_id)))?;
            if owner != scan_id.0 {
                return Err(StoreError::NotFound(format!(
                    "group {} of scan {}",
                    resolution.group_id, scan_id.0
                )));
            }
            if resolved_at.is_some() {
                return Err(StoreError::InvalidState(format!(
                    "group {} is already resolved",
                    resolution.group_id
                )));
            }
            if resolution.kept_file_ids.is_empty() {
                return Err(StoreError::Constraint(format!(
                    "group {} has no kept copy",
                    resolution.group_id
                )));
            }
            for file_id in &resolution.kept_file_ids {
                let changed = tx
                    .execute(SQL_MARK_KEPT, params![file_id, resolution.group_id])
                    .map_err(map_sqlite_error)?;
                if changed != 1 {
                    return Err(StoreError::Constraint(format!(
                        "copy {file_id} is not in group {}",
                        resolution.group_id
                    )));
                }
            }
            tx.execute(SQL_MARK_RESOLVED, params![now.0, resolution.group_id])
                .map_err(map_sqlite_error)?;
        }
        tx.commit().map_err(map_sqlite_error)
    }
}

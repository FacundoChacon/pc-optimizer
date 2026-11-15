//! Analisis de espacio en disco sobre SQLite (C-07, D-09, D-10): guardado atomico, ultimo analisis
//! terminado, hijos de una carpeta y archivos grandes paginados. Solo parametros enlazados:
//! ninguna entrada se concatena en SQL.

use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};

use super::SqliteStore;
use super::errors::map_sqlite_error;
use super::rows::scan_from_parts;
use crate::domain::disk_usage::model::{
    Access, FolderCounters, FolderCrumb, FolderIssue, FolderPage, FolderRow, FolderScanResults,
    FolderScanSummary, FolderSort, IssueReason, LargeFilePage, LargeFileRow, LargeFileSort,
    OpaqueReason, RootKind,
};
use crate::domain::disk_usage::select::remainder_bytes;
use crate::domain::model::{EpochMillis, ScanId};
use crate::domain::ports::{FolderStore, StoreError, VolumeSpaceInfo};
use crate::domain::protection::path::fold_component;

const SQL_SCAN_KIND_STATUS: &str = "SELECT kind, status FROM scan WHERE id = ?1";
/// Elimina las carpetas (y, en cascada, sus archivos grandes), archivos e incidencias de todo
/// analisis distinto del que se esta guardando; su escaneo y su resultado se conservan.
const SQL_PURGE_FOLDERS: &str = "DELETE FROM folder_snapshot WHERE scan_id IN \
     (SELECT id FROM scan WHERE kind = 'folders' AND id <> ?1)";
const SQL_PURGE_LARGE: &str = "DELETE FROM large_file WHERE scan_id IN \
     (SELECT id FROM scan WHERE kind = 'folders' AND id <> ?1)";
const SQL_PURGE_ISSUES: &str = "DELETE FROM folder_scan_issue WHERE scan_id IN \
     (SELECT id FROM scan WHERE kind = 'folders' AND id <> ?1)";
const SQL_INSERT_RESULT: &str = "INSERT INTO folder_scan_result(scan_id, root_kind, root_path, \
     volume_total_bytes, volume_free_bytes, total_bytes, file_count, dir_count, \
     inaccessible_dirs, reparse_skipped, cloud_files, opaque_dirs, folders_stored, \
     folders_truncated) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)";
const SQL_INSERT_FOLDER: &str = "INSERT INTO folder_snapshot(scan_id, parent_id, path, depth, \
     size_bytes, direct_bytes, file_count, dir_count, access, opaque_reason, system, junk_hint) \
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)";
const SQL_INSERT_LARGE: &str = "INSERT INTO large_file(scan_id, folder_id, path, size_bytes, \
     modified_at) VALUES (?1, ?2, ?3, ?4, ?5)";
const SQL_INSERT_ISSUE: &str =
    "INSERT INTO folder_scan_issue(scan_id, path, reason) VALUES (?1, ?2, ?3)";
const SQL_FINISH_DONE: &str =
    "UPDATE scan SET status = 'done', finished_at = ?1 WHERE id = ?2 AND status = 'running'";
const SQL_LAST_DONE: &str = "SELECT id, kind, triggered_by, status, started_at, finished_at \
     FROM scan WHERE kind = 'folders' AND status = 'done' \
     ORDER BY started_at DESC, id DESC LIMIT 1";
const SQL_SCAN_BY_ID: &str = "SELECT id, kind, triggered_by, status, started_at, finished_at \
     FROM scan WHERE id = ?1";
const SQL_RESULT: &str = "SELECT root_kind, root_path, volume_total_bytes, volume_free_bytes, \
     total_bytes, file_count, dir_count, inaccessible_dirs, reparse_skipped, cloud_files, \
     opaque_dirs, folders_stored, folders_truncated FROM folder_scan_result WHERE scan_id = ?1";
const SQL_ROOT_ID: &str = "SELECT id FROM folder_snapshot WHERE scan_id = ?1 AND parent_id IS NULL";
const SQL_ISSUES: &str =
    "SELECT path, reason FROM folder_scan_issue WHERE scan_id = ?1 ORDER BY rowid";
const SQL_FOLDER_COLUMNS: &str = "SELECT f.id, f.parent_id, f.path, f.depth, f.size_bytes, \
     f.direct_bytes, f.file_count, f.dir_count, f.access, f.opaque_reason, f.system, f.junk_hint, \
     EXISTS(SELECT 1 FROM folder_snapshot c WHERE c.parent_id = f.id) FROM folder_snapshot f";
const SQL_CHILD_TOTALS: &str =
    "SELECT count(*), coalesce(sum(size_bytes), 0) FROM folder_snapshot WHERE parent_id = ?1";
const SQL_FILE_COLUMNS: &str =
    "SELECT id, folder_id, path, size_bytes, modified_at FROM large_file";

fn to_i64(value: u64, what: &str) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| StoreError::Constraint(format!("{what} exceeds i64::MAX")))
}

fn non_negative(what: &str, raw: i64) -> Result<u64, StoreError> {
    u64::try_from(raw).map_err(|_| StoreError::Corrupt(format!("negative {what}")))
}

fn corrupt(what: &str, raw: &str) -> StoreError {
    StoreError::Corrupt(format!("unknown {what} '{raw}'"))
}

fn insert_all(
    tx: &Transaction<'_>,
    scan_id: ScanId,
    r: &FolderScanResults,
) -> Result<(), StoreError> {
    let c = &r.counters;
    tx.execute(
        SQL_INSERT_RESULT,
        params![
            scan_id.0,
            r.root_kind.as_str(),
            r.root_path,
            r.volume
                .map(|v| to_i64(v.total_bytes, "volume_total_bytes"))
                .transpose()?,
            r.volume
                .map(|v| to_i64(v.free_bytes, "volume_free_bytes"))
                .transpose()?,
            to_i64(c.total_bytes, "total_bytes")?,
            to_i64(c.file_count, "file_count")?,
            to_i64(c.dir_count, "dir_count")?,
            to_i64(c.inaccessible_dirs, "inaccessible_dirs")?,
            to_i64(c.reparse_skipped, "reparse_skipped")?,
            to_i64(c.cloud_files, "cloud_files")?,
            to_i64(c.opaque_dirs, "opaque_dirs")?,
            to_i64(c.folders_stored, "folders_stored")?,
            i64::from(r.folders_truncated),
        ],
    )
    .map_err(map_sqlite_error)?;
    // Padre antes que hija: las claves foraneas estan activas.
    let mut ids: Vec<i64> = Vec::with_capacity(r.folders.len());
    {
        let mut insert = tx.prepare(SQL_INSERT_FOLDER).map_err(map_sqlite_error)?;
        for folder in &r.folders {
            let parent = folder.parent.and_then(|index| ids.get(index).copied());
            insert
                .execute(params![
                    scan_id.0,
                    parent,
                    folder.path,
                    i64::from(folder.depth),
                    to_i64(folder.size_bytes, "size_bytes")?,
                    to_i64(folder.direct_bytes, "direct_bytes")?,
                    to_i64(folder.file_count, "file_count")?,
                    to_i64(folder.dir_count, "dir_count")?,
                    folder.access.as_str(),
                    folder.opaque.map(|reason| reason.as_str()),
                    i64::from(folder.system),
                    i64::from(folder.junk_hint),
                ])
                .map_err(map_sqlite_error)?;
            ids.push(tx.last_insert_rowid());
        }
    }
    let mut insert_large = tx.prepare(SQL_INSERT_LARGE).map_err(map_sqlite_error)?;
    for file in &r.large_files {
        let folder = ids
            .get(file.folder)
            .copied()
            .ok_or_else(|| StoreError::Constraint("large file folder out of range".into()))?;
        insert_large
            .execute(params![
                scan_id.0,
                folder,
                file.path,
                to_i64(file.size_bytes, "size_bytes")?,
                file.modified_at.map(|at| at.0),
            ])
            .map_err(map_sqlite_error)?;
    }
    let mut insert_issue = tx.prepare(SQL_INSERT_ISSUE).map_err(map_sqlite_error)?;
    for issue in &r.issues {
        insert_issue
            .execute(params![scan_id.0, issue.path, issue.reason.as_str()])
            .map_err(map_sqlite_error)?;
    }
    Ok(())
}

fn summary_from_conn(
    conn: &Connection,
    scan: crate::domain::model::Scan,
) -> Result<Option<FolderScanSummary>, StoreError> {
    type Raw = (String, String, Option<i64>, Option<i64>, [i64; 8], i64);
    let raw: Option<Raw> = conn
        .query_row(SQL_RESULT, [scan.id.0], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                [
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                    row.get(10)?,
                    row.get(11)?,
                ],
                row.get(12)?,
            ))
        })
        .optional()
        .map_err(map_sqlite_error)?;
    let Some((kind, root_path, total, free, counters, truncated)) = raw else {
        return Ok(None);
    };
    let n = |i: usize, what: &str| non_negative(what, counters[i]);
    let volume = match (total, free) {
        (Some(total), Some(free)) => Some(VolumeSpaceInfo {
            total_bytes: non_negative("volume total", total)?,
            free_bytes: non_negative("volume free", free)?,
        }),
        _ => None,
    };
    let root_folder_id: Option<i64> = conn
        .query_row(SQL_ROOT_ID, [scan.id.0], |row| row.get(0))
        .optional()
        .map_err(map_sqlite_error)?;
    let mut stmt = conn.prepare(SQL_ISSUES).map_err(map_sqlite_error)?;
    let rows = stmt
        .query_map([scan.id.0], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(map_sqlite_error)?;
    let mut issues = Vec::new();
    for row in rows {
        let (path, reason) = row.map_err(map_sqlite_error)?;
        issues.push(FolderIssue {
            path,
            reason: IssueReason::parse(&reason).ok_or_else(|| corrupt("issue reason", &reason))?,
        });
    }
    Ok(Some(FolderScanSummary {
        scan,
        root_kind: RootKind::parse(&kind).ok_or_else(|| corrupt("root kind", &kind))?,
        root_path,
        volume,
        counters: FolderCounters {
            total_bytes: n(0, "total_bytes")?,
            file_count: n(1, "file_count")?,
            dir_count: n(2, "dir_count")?,
            inaccessible_dirs: n(3, "inaccessible_dirs")?,
            reparse_skipped: n(4, "reparse_skipped")?,
            cloud_files: n(5, "cloud_files")?,
            opaque_dirs: n(6, "opaque_dirs")?,
            folders_stored: n(7, "folders_stored")?,
        },
        folders_truncated: truncated == 1,
        root_folder_id,
        issues,
    }))
}

fn folder_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Result<FolderRow, StoreError>> {
    let access: String = row.get(8)?;
    let opaque: Option<String> = row.get(9)?;
    let sizes: [i64; 5] = [
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
    ];
    let (id, parent_id, path): (i64, Option<i64>, String) = (row.get(0)?, row.get(1)?, row.get(2)?);
    let (system, junk_hint, has_children): (i64, i64, i64) =
        (row.get(10)?, row.get(11)?, row.get(12)?);
    Ok((|| {
        Ok(FolderRow {
            id,
            parent_id,
            path,
            depth: u32::try_from(sizes[0]).map_err(|_| StoreError::Corrupt("depth".into()))?,
            size_bytes: non_negative("size_bytes", sizes[1])?,
            direct_bytes: non_negative("direct_bytes", sizes[2])?,
            file_count: non_negative("file_count", sizes[3])?,
            dir_count: non_negative("dir_count", sizes[4])?,
            access: Access::parse(&access).ok_or_else(|| corrupt("access", &access))?,
            opaque: opaque
                .as_deref()
                .map(|raw| OpaqueReason::parse(raw).ok_or_else(|| corrupt("opaque reason", raw)))
                .transpose()?,
            system: system == 1,
            junk_hint: junk_hint == 1,
            has_children: has_children == 1,
        })
    })())
}

fn file_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Result<LargeFileRow, StoreError>> {
    let size: i64 = row.get(3)?;
    let (id, folder_id, path): (i64, i64, String) = (row.get(0)?, row.get(1)?, row.get(2)?);
    let modified: Option<i64> = row.get(4)?;
    Ok(
        non_negative("size_bytes", size).map(|size_bytes| LargeFileRow {
            id,
            folder_id,
            path,
            size_bytes,
            modified_at: modified.map(EpochMillis),
        }),
    )
}

fn get_folder(conn: &Connection, scan: ScanId, id: i64) -> Result<Option<FolderRow>, StoreError> {
    conn.query_row(
        &format!("{SQL_FOLDER_COLUMNS} WHERE f.scan_id = ?1 AND f.id = ?2"),
        params![scan.0, id],
        folder_from_row,
    )
    .optional()
    .map_err(map_sqlite_error)?
    .transpose()
}

fn crumb(row: &FolderRow) -> FolderCrumb {
    FolderCrumb {
        id: row.id,
        name: row.name(),
    }
}

impl FolderStore for SqliteStore {
    fn complete_folder_scan(
        &self,
        scan_id: ScanId,
        results: &FolderScanResults,
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
        if kind != "folders" || status != "running" {
            return Err(StoreError::InvalidState(format!(
                "scan {} is not a running folders scan",
                scan_id.0
            )));
        }
        for sql in [SQL_PURGE_LARGE, SQL_PURGE_FOLDERS, SQL_PURGE_ISSUES] {
            tx.execute(sql, [scan_id.0]).map_err(map_sqlite_error)?;
        }
        insert_all(&tx, scan_id, results)?;
        tx.execute(SQL_FINISH_DONE, params![finished_at.0, scan_id.0])
            .map_err(map_sqlite_error)?;
        tx.commit().map_err(map_sqlite_error)
    }

    fn last_done_folder_scan(&self) -> Result<Option<FolderScanSummary>, StoreError> {
        let conn = self.conn()?;
        let scan = conn
            .query_row(SQL_LAST_DONE, [], scan_row)
            .optional()
            .map_err(map_sqlite_error)?
            .transpose()?;
        let Some(scan) = scan else { return Ok(None) };
        summary_from_conn(&conn, scan)?
            .ok_or_else(|| StoreError::Corrupt("folder scan without result".into()))
            .map(Some)
    }

    fn folder_scan_summary(
        &self,
        scan_id: ScanId,
    ) -> Result<Option<FolderScanSummary>, StoreError> {
        let conn = self.conn()?;
        let scan = conn
            .query_row(SQL_SCAN_BY_ID, [scan_id.0], scan_row)
            .optional()
            .map_err(map_sqlite_error)?
            .transpose()?;
        match scan {
            None => Ok(None),
            Some(scan) => summary_from_conn(&conn, scan),
        }
    }

    fn list_folder_children(
        &self,
        scan_id: ScanId,
        folder_id: Option<i64>,
        sort: FolderSort,
        offset: u64,
        limit: u32,
    ) -> Result<FolderPage, StoreError> {
        let conn = self.conn()?;
        let target_id = match folder_id {
            Some(id) => Some(id),
            None => conn
                .query_row(SQL_ROOT_ID, [scan_id.0], |row| row.get::<_, i64>(0))
                .optional()
                .map_err(map_sqlite_error)?,
        };
        let target = target_id
            .map(|id| get_folder(&conn, scan_id, id))
            .transpose()?
            .flatten()
            .ok_or_else(|| StoreError::NotFound(format!("folder of scan {}", scan_id.0)))?;
        let mut breadcrumb = vec![crumb(&target)];
        let mut parent = target.parent_id;
        while let Some(id) = parent {
            let Some(row) = get_folder(&conn, scan_id, id)? else {
                break;
            };
            breadcrumb.push(crumb(&row));
            parent = row.parent_id;
        }
        breadcrumb.reverse();
        let (total, children_sum): (i64, i64) = conn
            .query_row(SQL_CHILD_TOTALS, [target.id], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .map_err(map_sqlite_error)?;
        let skip = i64::try_from(offset).unwrap_or(i64::MAX);
        let items = match sort {
            FolderSort::Size => {
                let sql = format!(
                    "{SQL_FOLDER_COLUMNS} WHERE f.parent_id = ?1 \
                     ORDER BY f.size_bytes DESC, f.id ASC LIMIT ?2 OFFSET ?3"
                );
                let mut stmt = conn.prepare(&sql).map_err(map_sqlite_error)?;
                let rows = stmt
                    .query_map(params![target.id, i64::from(limit), skip], folder_from_row)
                    .map_err(map_sqlite_error)?;
                rows.map(|row| row.map_err(map_sqlite_error)?)
                    .collect::<Result<Vec<_>, _>>()?
            }
            FolderSort::Name => {
                // El plegado de nombres es el de NTFS (Rust), no el de SQLite: se ordena aca.
                let sql = format!("{SQL_FOLDER_COLUMNS} WHERE f.parent_id = ?1");
                let mut stmt = conn.prepare(&sql).map_err(map_sqlite_error)?;
                let rows = stmt
                    .query_map([target.id], folder_from_row)
                    .map_err(map_sqlite_error)?;
                let mut all = rows
                    .map(|row| row.map_err(map_sqlite_error)?)
                    .collect::<Result<Vec<_>, _>>()?;
                all.sort_by_cached_key(|r| (fold_component(&r.name()), r.id));
                all.into_iter()
                    .skip(usize::try_from(offset).unwrap_or(usize::MAX))
                    .take(limit as usize)
                    .collect()
            }
        };
        let remainder = remainder_bytes(
            target.size_bytes,
            target.direct_bytes,
            [non_negative("children size", children_sum)?],
        );
        Ok(FolderPage {
            folder: target,
            breadcrumb,
            total: non_negative("children count", total)?,
            items,
            remainder_bytes: remainder,
        })
    }

    fn list_large_files(
        &self,
        scan_id: ScanId,
        sort: LargeFileSort,
        offset: u64,
        limit: u32,
    ) -> Result<LargeFilePage, StoreError> {
        let conn = self.conn()?;
        let total: i64 = conn
            .query_row(
                "SELECT count(*) FROM large_file WHERE scan_id = ?1",
                [scan_id.0],
                |row| row.get(0),
            )
            .map_err(map_sqlite_error)?;
        let order = match sort {
            LargeFileSort::Size => "size_bytes DESC, id ASC",
            // Sin fecha al final en ambos ordenes.
            LargeFileSort::Newest => "modified_at IS NULL, modified_at DESC, id ASC",
            LargeFileSort::Oldest => "modified_at IS NULL, modified_at ASC, id ASC",
        };
        let sql =
            format!("{SQL_FILE_COLUMNS} WHERE scan_id = ?1 ORDER BY {order} LIMIT ?2 OFFSET ?3");
        let mut stmt = conn.prepare(&sql).map_err(map_sqlite_error)?;
        let rows = stmt
            .query_map(
                params![
                    scan_id.0,
                    i64::from(limit),
                    i64::try_from(offset).unwrap_or(i64::MAX)
                ],
                file_from_row,
            )
            .map_err(map_sqlite_error)?;
        let items = rows
            .map(|row| row.map_err(map_sqlite_error)?)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(LargeFilePage {
            total: non_negative("large file count", total)?,
            items,
        })
    }

    fn get_folder(&self, scan_id: ScanId, folder_id: i64) -> Result<Option<FolderRow>, StoreError> {
        let conn = self.conn()?;
        get_folder(&conn, scan_id, folder_id)
    }

    fn get_large_file(
        &self,
        scan_id: ScanId,
        file_id: i64,
    ) -> Result<Option<LargeFileRow>, StoreError> {
        self.conn()?
            .query_row(
                &format!("{SQL_FILE_COLUMNS} WHERE scan_id = ?1 AND id = ?2"),
                params![scan_id.0, file_id],
                file_from_row,
            )
            .optional()
            .map_err(map_sqlite_error)?
            .transpose()
    }
}

fn scan_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<Result<crate::domain::model::Scan, StoreError>> {
    Ok(scan_from_parts(
        row.get(0)?,
        &row.get::<_, String>(1)?,
        &row.get::<_, String>(2)?,
        &row.get::<_, String>(3)?,
        row.get(4)?,
        row.get(5)?,
    ))
}

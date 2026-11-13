//! Adaptador SQLite de la persistencia.

pub mod connection;
pub mod errors;
pub mod migrations;
pub mod recovery;
mod rows;
pub mod state;

pub use state::{StoreState, open_store_state};

use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use crate::domain::model::{
    CleanupLogEntry, EpochMillis, NewCleanupLogEntry, NewScan, SCAN_RETENTION, Scan, ScanId,
    ScanStatus,
};
use crate::domain::ports::{CleanupLogStore, ScanStore, SettingsStore, StoreError};
use errors::map_sqlite_error;
use rows::{RawLogRow, log_entry_from_parts, scan_from_parts};

const SQL_GET_SETTING: &str = "SELECT value_json FROM settings WHERE key = ?1";
const SQL_LIST_SETTINGS: &str = "SELECT key, value_json FROM settings ORDER BY key";
const SQL_UPSERT_SETTING: &str = "INSERT INTO settings(key, value_json) VALUES (?1, ?2) \
     ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json";
const SQL_INSERT_SCAN: &str = "INSERT INTO scan(kind, triggered_by, status, started_at) \
     VALUES (?1, ?2, 'running', ?3)";
/// Retencion: se conservan los `?1` mas recientes y todo escaneo `running`.
const SQL_PRUNE_SCANS: &str = "DELETE FROM scan WHERE status <> 'running' AND id NOT IN \
     (SELECT id FROM scan ORDER BY started_at DESC, id DESC LIMIT ?1)";
const SQL_FINISH_SCAN: &str =
    "UPDATE scan SET status = ?1, finished_at = ?2 WHERE id = ?3 AND status = 'running'";
const SQL_SCAN_COLUMNS: &str =
    "SELECT id, kind, triggered_by, status, started_at, finished_at FROM scan";
const SQL_INSERT_LOG: &str = "INSERT INTO cleanup_log(ts, category, path, size_bytes, outcome, scan_id, error_detail) \
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)";
const SQL_RECENT_LOG: &str = "SELECT id, ts, category, path, size_bytes, outcome, scan_id, error_detail \
     FROM cleanup_log ORDER BY ts DESC, id DESC LIMIT ?1";

/// Persistencia sobre una unica conexion SQLite protegida por `Mutex`.
///
/// Una sola escritora y volumen bajo: un pool no aporta (ver design D-08).
pub struct SqliteStore {
    conn: Mutex<Connection>,
}

impl SqliteStore {
    /// Abre (o crea) la base en `path`, valida, configura y migra.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        Ok(Self {
            conn: Mutex::new(connection::open_connection(path)?),
        })
    }

    fn conn(&self) -> Result<MutexGuard<'_, Connection>, StoreError> {
        self.conn
            .lock()
            .map_err(|_| StoreError::Unavailable("lock poisoned".into()))
    }
}

fn scan_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Result<Scan, StoreError>> {
    Ok(scan_from_parts(
        row.get(0)?,
        &row.get::<_, String>(1)?,
        &row.get::<_, String>(2)?,
        &row.get::<_, String>(3)?,
        row.get(4)?,
        row.get(5)?,
    ))
}

impl SettingsStore for SqliteStore {
    fn get_setting(&self, key: &str) -> Result<Option<serde_json::Value>, StoreError> {
        let raw: Option<String> = self
            .conn()?
            .query_row(SQL_GET_SETTING, [key], |row| row.get(0))
            .optional()
            .map_err(map_sqlite_error)?;
        raw.map(|json| parse_json(&json)).transpose()
    }

    fn list_settings(&self) -> Result<Vec<(String, serde_json::Value)>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(SQL_LIST_SETTINGS).map_err(map_sqlite_error)?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(map_sqlite_error)?;
        rows.map(|row| {
            let (key, json) = row.map_err(map_sqlite_error)?;
            Ok((key, parse_json(&json)?))
        })
        .collect()
    }

    fn set_setting(&self, key: &str, value: &serde_json::Value) -> Result<(), StoreError> {
        if key.is_empty() {
            return Err(StoreError::Constraint("empty setting key".into()));
        }
        let json = serde_json::to_string(value)
            .map_err(|_| StoreError::Constraint("value is not serializable".into()))?;
        self.conn()?
            .execute(SQL_UPSERT_SETTING, params![key, json])
            .map_err(map_sqlite_error)?;
        Ok(())
    }
}

fn parse_json(raw: &str) -> Result<serde_json::Value, StoreError> {
    serde_json::from_str(raw).map_err(|_| StoreError::Corrupt("invalid JSON in settings".into()))
}

impl ScanStore for SqliteStore {
    fn create_scan(&self, scan: &NewScan) -> Result<ScanId, StoreError> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite_error)?;
        tx.execute(
            SQL_INSERT_SCAN,
            params![scan.kind.as_str(), scan.trigger.as_str(), scan.started_at.0],
        )
        .map_err(map_sqlite_error)?;
        let id = tx.last_insert_rowid();
        let keep = i64::try_from(SCAN_RETENTION).unwrap_or(i64::MAX);
        tx.execute(SQL_PRUNE_SCANS, [keep])
            .map_err(map_sqlite_error)?;
        tx.commit().map_err(map_sqlite_error)?;
        Ok(ScanId(id))
    }

    fn finish_scan(
        &self,
        id: ScanId,
        status: ScanStatus,
        finished_at: EpochMillis,
    ) -> Result<(), StoreError> {
        if !status.is_terminal() {
            return Err(StoreError::InvalidState(format!(
                "cannot finish a scan with status {}",
                status.as_str()
            )));
        }
        let conn = self.conn()?;
        let changed = conn
            .execute(
                SQL_FINISH_SCAN,
                params![status.as_str(), finished_at.0, id.0],
            )
            .map_err(map_sqlite_error)?;
        if changed == 1 {
            return Ok(());
        }
        let exists = conn
            .query_row("SELECT 1 FROM scan WHERE id = ?1", [id.0], |_| Ok(()))
            .optional()
            .map_err(map_sqlite_error)?
            .is_some();
        Err(if exists {
            StoreError::InvalidState(format!("scan {} already finished", id.0))
        } else {
            StoreError::NotFound(format!("scan {}", id.0))
        })
    }

    fn get_scan(&self, id: ScanId) -> Result<Option<Scan>, StoreError> {
        let sql = format!("{SQL_SCAN_COLUMNS} WHERE id = ?1");
        self.conn()?
            .query_row(&sql, [id.0], scan_from_row)
            .optional()
            .map_err(map_sqlite_error)?
            .transpose()
    }

    fn list_scans(&self) -> Result<Vec<Scan>, StoreError> {
        let conn = self.conn()?;
        let sql = format!("{SQL_SCAN_COLUMNS} ORDER BY started_at DESC, id DESC");
        let mut stmt = conn.prepare(&sql).map_err(map_sqlite_error)?;
        let rows = stmt
            .query_map([], scan_from_row)
            .map_err(map_sqlite_error)?;
        rows.map(|row| row.map_err(map_sqlite_error)?).collect()
    }
}

impl CleanupLogStore for SqliteStore {
    fn append_cleanup_log(&self, entries: &[NewCleanupLogEntry]) -> Result<(), StoreError> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite_error)?;
        {
            let mut stmt = tx.prepare(SQL_INSERT_LOG).map_err(map_sqlite_error)?;
            for entry in entries {
                let size = i64::try_from(entry.size_bytes)
                    .map_err(|_| StoreError::Constraint("size_bytes exceeds i64::MAX".into()))?;
                stmt.execute(params![
                    entry.ts.0,
                    entry.category,
                    entry.path,
                    size,
                    entry.outcome.as_str(),
                    entry.scan_id.map(|id| id.0),
                    entry.error_detail,
                ])
                .map_err(map_sqlite_error)?;
            }
        }
        tx.commit().map_err(map_sqlite_error)
    }

    fn recent_cleanup_log(&self, limit: u32) -> Result<Vec<CleanupLogEntry>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(SQL_RECENT_LOG).map_err(map_sqlite_error)?;
        let rows = stmt
            .query_map([i64::from(limit)], |row| {
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
            })
            .map_err(map_sqlite_error)?;
        rows.map(|row| row.map_err(map_sqlite_error)?).collect()
    }
}

#[cfg(test)]
mod schema_tests;
#[cfg(test)]
mod store_tests;

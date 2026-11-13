//! Apertura de la conexion: pragmas, deteccion de corrupcion y migraciones.

use std::path::Path;
use std::time::Duration;

use rusqlite::Connection;

use super::errors::map_sqlite_error;
use super::migrations::{MIGRATIONS, latest_version, migrate, read_user_version};
use crate::domain::ports::StoreError;

/// Espera maxima por un bloqueo antes de fallar con `Unavailable`.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// Abre (o crea) la base en `path`: valida integridad y version ANTES de escribir nada
/// (ni siquiera el modo WAL), aplica pragmas y migra.
pub fn open_connection(path: &Path) -> Result<Connection, StoreError> {
    let mut conn = Connection::open(path).map_err(map_sqlite_error)?;
    conn.busy_timeout(BUSY_TIMEOUT).map_err(map_sqlite_error)?;
    check_integrity(&conn)?;
    let found = read_user_version(&conn)?;
    let supported = latest_version(MIGRATIONS);
    if found > supported {
        return Err(StoreError::SchemaTooNew { found, supported });
    }
    apply_pragmas(&conn)?;
    migrate(&mut conn, MIGRATIONS)?;
    Ok(conn)
}

/// Conexion en memoria con el esquema aplicado (solo tests; WAL no aplica).
#[cfg(test)]
pub fn open_in_memory_connection() -> Result<Connection, StoreError> {
    let mut conn = Connection::open_in_memory().map_err(map_sqlite_error)?;
    conn.pragma_update(None, "foreign_keys", true)
        .map_err(map_sqlite_error)?;
    migrate(&mut conn, MIGRATIONS)?;
    Ok(conn)
}

/// `PRAGMA quick_check` debe devolver `ok`; abrir basura recien falla en la primera lectura.
fn check_integrity(conn: &Connection) -> Result<(), StoreError> {
    let verdict: String = conn
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .map_err(map_sqlite_error)?;
    if verdict == "ok" {
        Ok(())
    } else {
        Err(StoreError::Corrupt("quick_check failed".into()))
    }
}

fn apply_pragmas(conn: &Connection) -> Result<(), StoreError> {
    conn.pragma_update(None, "foreign_keys", true)
        .map_err(map_sqlite_error)?;
    // `journal_mode` devuelve una fila (el modo resultante), por eso se lee con query_row.
    let _mode: String = conn
        .query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))
        .map_err(map_sqlite_error)?;
    conn.pragma_update(None, "synchronous", "NORMAL")
        .map_err(map_sqlite_error)
}

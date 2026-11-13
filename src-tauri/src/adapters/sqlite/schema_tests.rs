//! Pruebas del esquema 001 y de la apertura de conexion sobre archivos temporales.

use rusqlite::Connection;
use tempfile::TempDir;

use super::connection::open_connection;
use super::migrations::read_user_version;
use crate::domain::ports::StoreError;

fn fresh() -> (TempDir, std::path::PathBuf, Connection) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("data.db");
    let conn = open_connection(&path).unwrap();
    (dir, path, conn)
}

fn names(conn: &Connection, kind: &str) -> Vec<String> {
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type=?1 AND name NOT LIKE 'sqlite_%' ORDER BY name")
        .unwrap();
    stmt.query_map([kind], |r| r.get::<_, String>(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn insert_scan(
    conn: &Connection,
    kind: &str,
    status: &str,
    started: i64,
    finished: Option<i64>,
) -> rusqlite::Result<usize> {
    conn.execute(
        "INSERT INTO scan(kind, triggered_by, status, started_at, finished_at) VALUES (?1, 'manual', ?2, ?3, ?4)",
        rusqlite::params![kind, status, started, finished],
    )
}

fn insert_log(conn: &Connection, outcome: &str, size: i64) -> rusqlite::Result<usize> {
    conn.execute(
        "INSERT INTO cleanup_log(ts, category, path, size_bytes, outcome) VALUES (1700000000000, 'temp', 'C:/x', ?1, ?2)",
        rusqlite::params![size, outcome],
    )
}

// --- 5.3 esquema ---
#[test]
fn open_should_create_exactly_the_001_schema_and_seed() {
    let (_dir, _path, conn) = fresh();
    assert_eq!(read_user_version(&conn).unwrap(), 1);
    assert_eq!(
        names(&conn, "table"),
        vec!["cleanup_log", "scan", "settings"]
    );
    let indexes = names(&conn, "index");
    assert!(
        indexes.contains(&"idx_scan_started_at".to_string()),
        "{indexes:?}"
    );
    assert!(
        indexes.contains(&"idx_cleanup_log_ts".to_string()),
        "{indexes:?}"
    );
    let ui_mode: String = conn
        .query_row(
            "SELECT value_json FROM settings WHERE key='ui_mode'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(ui_mode, "\"simple\"");
}

#[test]
fn schema_should_reject_invalid_scan_rows() {
    let (_dir, _path, conn) = fresh();
    assert!(insert_scan(&conn, "quick", "running", 1000, None).is_err());
    assert!(insert_scan(&conn, "full", "running", 0, None).is_err());
    assert!(
        insert_scan(&conn, "full", "done", 1000, None).is_err(),
        "done sin finished_at"
    );
    assert!(
        insert_scan(&conn, "full", "running", 1000, Some(2000)).is_err(),
        "running con finished_at"
    );
    assert!(
        insert_scan(&conn, "full", "done", 1000, Some(500)).is_err(),
        "finished < started"
    );
    // Control positivo.
    assert_eq!(
        insert_scan(&conn, "full", "running", 1000, None).unwrap(),
        1
    );
    assert_eq!(
        insert_scan(&conn, "junk", "done", 1000, Some(1000)).unwrap(),
        1
    );
}

#[test]
fn schema_should_reject_invalid_log_and_settings_rows() {
    let (_dir, _path, conn) = fresh();
    assert!(insert_log(&conn, "deleted", 10).is_err());
    assert!(insert_log(&conn, "moved_to_trash", -1).is_err());
    assert!(insert_log(&conn, "moved_to_trash", 10).is_ok());
    assert!(
        conn.execute(
            "INSERT INTO settings(key, value_json) VALUES ('k', 'no-json')",
            []
        )
        .is_err()
    );
    assert!(
        conn.execute("INSERT INTO settings(key, value_json) VALUES ('', '1')", [])
            .is_err()
    );
    assert!(
        conn.execute(
            "INSERT INTO settings(key, value_json) VALUES ('k', '{\"a\":1}')",
            []
        )
        .is_ok()
    );
}

// --- 5.4 append-only ---
#[test]
fn cleanup_log_should_be_append_only_at_database_level() {
    let (_dir, path, conn) = fresh();
    insert_log(&conn, "moved_to_trash", 10).unwrap();
    drop(conn);
    // Conexion directa, sin pasar por el adaptador.
    let raw = Connection::open(&path).unwrap();
    let update = raw
        .execute("UPDATE cleanup_log SET path='C:/otro'", [])
        .unwrap_err();
    assert!(update.to_string().contains("append-only"), "{update}");
    let delete = raw.execute("DELETE FROM cleanup_log", []).unwrap_err();
    assert!(delete.to_string().contains("append-only"), "{delete}");
    let (count, stored): (i64, String) = raw
        .query_row("SELECT count(*), min(path) FROM cleanup_log", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!((count, stored.as_str()), (1, "C:/x"));
    insert_log(&raw, "skipped_locked", 5).unwrap();
    let count: i64 = raw
        .query_row("SELECT count(*) FROM cleanup_log", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 2);
}

// --- 5.5 conexion ---
#[test]
fn open_connection_should_apply_pragmas() {
    let (_dir, _path, conn) = fresh();
    let pragma = |name: &str| -> String {
        conn.query_row(&format!("PRAGMA {name}"), [], |r| {
            r.get::<_, rusqlite::types::Value>(0)
        })
        .map(|v| match v {
            rusqlite::types::Value::Integer(i) => i.to_string(),
            rusqlite::types::Value::Text(t) => t,
            other => format!("{other:?}"),
        })
        .unwrap()
    };
    assert_eq!(pragma("foreign_keys"), "1");
    assert_eq!(pragma("journal_mode"), "wal");
    assert_eq!(pragma("synchronous"), "1");
    assert!(pragma("busy_timeout").parse::<i64>().unwrap() >= 5000);
}

#[test]
fn open_connection_should_report_corrupt_and_leave_a_garbage_file_untouched() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("data.db");
    let garbage = b"esto no es una base de datos sqlite, solo texto plano".repeat(40);
    std::fs::write(&path, &garbage).unwrap();
    let err = open_connection(&path).unwrap_err();
    assert!(matches!(err, StoreError::Corrupt(_)), "{err:?}");
    assert_eq!(std::fs::read(&path).unwrap(), garbage);
    assert!(!dir.path().join("data.db-wal").exists());
}

#[test]
fn open_connection_should_report_schema_too_new_and_leave_the_file_untouched() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("data.db");
    {
        let raw = Connection::open(&path).unwrap();
        raw.execute_batch("CREATE TABLE futuro(x INTEGER); PRAGMA user_version = 99;")
            .unwrap();
    }
    let before = std::fs::read(&path).unwrap();
    let err = open_connection(&path).unwrap_err();
    assert_eq!(
        err,
        StoreError::SchemaTooNew {
            found: 99,
            supported: 1
        }
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(!dir.path().join("data.db-wal").exists());
}

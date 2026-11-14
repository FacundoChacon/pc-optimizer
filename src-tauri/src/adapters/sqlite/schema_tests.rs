//! Pruebas del esquema 001 y de la apertura de conexion sobre archivos temporales.

use rusqlite::Connection;
use tempfile::TempDir;

use super::connection::open_connection;
use super::migrations::{MIGRATIONS, migrate, read_user_version};
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
fn open_should_create_exactly_the_current_schema_and_seed() {
    let (_dir, _path, conn) = fresh();
    assert_eq!(read_user_version(&conn).unwrap(), 2);
    assert_eq!(
        names(&conn, "table"),
        vec![
            "cleanup_log",
            "junk_category_result",
            "junk_finding",
            "scan",
            "settings"
        ]
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
            supported: 2
        }
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(!dir.path().join("data.db-wal").exists());
}

// --- C-08: migracion 002 (hallazgos de basura) ---
const FILE_ID: &str = "00000000000000000000000000000001";
const WHITELIST: [&str; 11] = [
    "temp_user",
    "temp_windows",
    "browser_cache_chrome",
    "browser_cache_edge",
    "browser_cache_firefox",
    "browser_cache_brave",
    "old_logs",
    "wu_cache",
    "thumbnail_cache",
    "crash_dumps",
    "recycle_bin",
];

/// Un escaneo de basura `running` y su id.
fn junk_scan(conn: &Connection) -> i64 {
    insert_scan(conn, "junk", "running", 1000, None).unwrap();
    conn.last_insert_rowid()
}

/// Columnas variables de un hallazgo de prueba.
struct Finding<'a> {
    scan_id: i64,
    category: &'a str,
    path: &'a str,
    protected: i64,
    skip_reason: Option<&'a str>,
    volume_serial: Option<i64>,
    file_id: Option<&'a str>,
}

impl<'a> Finding<'a> {
    /// Hallazgo elegible valido.
    fn eligible(scan_id: i64, path: &'a str) -> Self {
        Self {
            scan_id,
            category: "temp_user",
            path,
            protected: 0,
            skip_reason: None,
            volume_serial: Some(7),
            file_id: Some(FILE_ID),
        }
    }

    /// Hallazgo omitido valido.
    fn skipped(scan_id: i64, path: &'a str) -> Self {
        Self {
            protected: 1,
            skip_reason: Some("reparse_point"),
            volume_serial: None,
            file_id: None,
            ..Self::eligible(scan_id, path)
        }
    }

    fn insert(&self, conn: &Connection) -> rusqlite::Result<usize> {
        conn.execute(
            "INSERT INTO junk_finding(scan_id, category, path, size_bytes, modified_at, \
             protected_flag, skip_reason, volume_serial, file_id) \
             VALUES (?1, ?2, ?3, 10, NULL, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                self.scan_id,
                self.category,
                self.path,
                self.protected,
                self.skip_reason,
                self.volume_serial,
                self.file_id
            ],
        )
    }
}

fn insert_result(
    conn: &Connection,
    scan_id: i64,
    category: &str,
    status: &str,
    block_reason: Option<&str>,
) -> rusqlite::Result<usize> {
    conn.execute(
        "INSERT INTO junk_category_result(scan_id, category, scope, status, block_reason, \
         item_count, size_bytes, skipped_count, inaccessible_count) \
         VALUES (?1, ?2, 'per_user', ?3, ?4, 1, 10, 0, 0)",
        rusqlite::params![scan_id, category, status, block_reason],
    )
}

fn count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

#[test]
fn junk_finding_should_accept_the_whitelist_and_custom_rule_but_reject_other_categories() {
    let (_dir, _path, conn) = fresh();
    let scan = junk_scan(&conn);
    for (index, category) in WHITELIST.iter().chain(&["custom_rule"]).enumerate() {
        let path = format!("C:/x/{index}");
        let finding = Finding {
            category,
            ..Finding::eligible(scan, &path)
        };
        assert_eq!(finding.insert(&conn).unwrap(), 1, "{category}");
        assert_eq!(
            insert_result(&conn, scan, category, "done", None).unwrap(),
            1,
            "{category}"
        );
    }
    for category in ["documents", "program_files", "", "TEMP_USER"] {
        let finding = Finding {
            category,
            ..Finding::eligible(scan, "C:/otro")
        };
        assert!(finding.insert(&conn).is_err(), "{category:?}");
        assert!(
            insert_result(&conn, scan, category, "done", None).is_err(),
            "{category:?}"
        );
    }
}

#[test]
fn junk_finding_should_require_a_reason_exactly_when_skipped() {
    let (_dir, _path, conn) = fresh();
    let scan = junk_scan(&conn);
    let skipped_without_reason = Finding {
        skip_reason: None,
        ..Finding::skipped(scan, "C:/a")
    };
    assert!(skipped_without_reason.insert(&conn).is_err());
    let eligible_with_reason = Finding {
        skip_reason: Some("reparse_point"),
        ..Finding::eligible(scan, "C:/b")
    };
    assert!(eligible_with_reason.insert(&conn).is_err());
    assert_eq!(Finding::skipped(scan, "C:/c").insert(&conn).unwrap(), 1);
    assert_eq!(Finding::eligible(scan, "C:/d").insert(&conn).unwrap(), 1);
}

#[test]
fn junk_finding_should_require_identity_for_eligible_files_with_a_32_char_file_id() {
    let (_dir, _path, conn) = fresh();
    let scan = junk_scan(&conn);
    let no_volume = Finding {
        volume_serial: None,
        ..Finding::eligible(scan, "C:/a")
    };
    let no_file_id = Finding {
        file_id: None,
        ..Finding::eligible(scan, "C:/b")
    };
    assert!(no_volume.insert(&conn).is_err());
    assert!(no_file_id.insert(&conn).is_err());
    for bad in ["", "abc", &"f".repeat(31), &"f".repeat(33)] {
        let finding = Finding {
            file_id: Some(bad),
            ..Finding::eligible(scan, "C:/c")
        };
        assert!(finding.insert(&conn).is_err(), "{bad:?}");
    }
    let max = Finding {
        file_id: Some("ffffffffffffffffffffffffffffffff"),
        ..Finding::eligible(scan, "C:/d")
    };
    assert_eq!(max.insert(&conn).unwrap(), 1);
}

#[test]
fn junk_finding_should_reject_a_repeated_path_in_the_same_scan_only() {
    let (_dir, _path, conn) = fresh();
    let first = junk_scan(&conn);
    insert_scan(&conn, "junk", "running", 2000, None).unwrap();
    let second = conn.last_insert_rowid();
    assert_eq!(Finding::eligible(first, "C:/a").insert(&conn).unwrap(), 1);
    assert!(Finding::eligible(first, "C:/a").insert(&conn).is_err());
    assert_eq!(Finding::eligible(second, "C:/a").insert(&conn).unwrap(), 1);
}

#[test]
fn junk_tables_should_reject_rows_of_an_unknown_scan() {
    let (_dir, _path, conn) = fresh();
    assert!(Finding::eligible(999, "C:/a").insert(&conn).is_err());
    assert!(insert_result(&conn, 999, "temp_user", "done", None).is_err());
}

#[test]
fn junk_category_result_should_require_a_block_reason_exactly_for_blocked_and_partial() {
    let (_dir, _path, conn) = fresh();
    let scan = junk_scan(&conn);
    for status in ["blocked", "partial"] {
        assert!(insert_result(&conn, scan, "temp_user", status, None).is_err());
    }
    for status in ["done", "unavailable"] {
        assert!(insert_result(&conn, scan, "temp_user", status, Some("different_user")).is_err());
    }
    assert!(insert_result(&conn, scan, "temp_user", "blocked", Some("other")).is_err());
    assert!(insert_result(&conn, scan, "temp_user", "ready", None).is_err());
    assert_eq!(
        insert_result(&conn, scan, "temp_user", "blocked", Some("different_user")).unwrap(),
        1
    );
    assert_eq!(
        insert_result(&conn, scan, "crash_dumps", "partial", Some("undetermined")).unwrap(),
        1
    );
    assert!(
        insert_result(&conn, scan, "temp_user", "done", None).is_err(),
        "clave primaria repetida"
    );
}

#[test]
fn junk_rows_should_be_deleted_in_cascade_with_their_scan_leaving_the_log_alone() {
    let (_dir, _path, conn) = fresh();
    let scan = junk_scan(&conn);
    Finding::eligible(scan, "C:/a").insert(&conn).unwrap();
    insert_result(&conn, scan, "temp_user", "done", None).unwrap();
    insert_log(&conn, "moved_to_trash", 10).unwrap();
    conn.execute("DELETE FROM scan WHERE id = ?1", [scan])
        .unwrap();
    assert_eq!(count(&conn, "junk_finding"), 0);
    assert_eq!(count(&conn, "junk_category_result"), 0);
    assert_eq!(count(&conn, "cleanup_log"), 1);
}

#[test]
fn open_should_migrate_a_version_1_database_keeping_all_its_data() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("data.db");
    {
        let mut v1 = Connection::open(&path).unwrap();
        migrate(&mut v1, &MIGRATIONS[..1]).unwrap();
        assert_eq!(read_user_version(&v1).unwrap(), 1);
        v1.execute(
            "UPDATE settings SET value_json = '\"advanced\"' WHERE key = 'ui_mode'",
            [],
        )
        .unwrap();
        insert_scan(&v1, "junk", "done", 1000, Some(2000)).unwrap();
        insert_log(&v1, "moved_to_trash", 10).unwrap();
    }
    let conn = open_connection(&path).unwrap();
    assert_eq!(read_user_version(&conn).unwrap(), 2);
    let ui_mode: String = conn
        .query_row(
            "SELECT value_json FROM settings WHERE key='ui_mode'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(ui_mode, "\"advanced\"");
    assert_eq!(count(&conn, "scan"), 1);
    assert_eq!(count(&conn, "cleanup_log"), 1);
    assert_eq!(count(&conn, "junk_finding"), 0);
}

//! Pruebas del esquema de la migracion 004 (C-07): analisis de espacio en disco.

use rusqlite::{Connection, params};
use tempfile::TempDir;

use super::connection::open_connection;
use super::migrations::{MIGRATIONS, migrate, read_user_version};

fn fresh() -> (TempDir, std::path::PathBuf, Connection) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("data.db");
    let conn = open_connection(&path).unwrap();
    (dir, path, conn)
}

fn count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

fn folder_scan(conn: &Connection) -> i64 {
    conn.execute(
        "INSERT INTO scan(kind, triggered_by, status, started_at, finished_at) \
         VALUES ('folders', 'manual', 'done', 1000, 2000)",
        [],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn insert_result(
    conn: &Connection,
    scan: i64,
    root_kind: &str,
    volume_total: Option<i64>,
    volume_free: Option<i64>,
    truncated: i64,
) -> rusqlite::Result<usize> {
    conn.execute(
        "INSERT INTO folder_scan_result(scan_id, root_kind, root_path, volume_total_bytes, \
         volume_free_bytes, total_bytes, file_count, dir_count, inaccessible_dirs, \
         reparse_skipped, cloud_files, opaque_dirs, folders_stored, folders_truncated) \
         VALUES (?1, ?2, 'C:\\', ?3, ?4, 10, 2, 1, 0, 0, 0, 0, 1, ?5)",
        params![scan, root_kind, volume_total, volume_free, truncated],
    )
}

#[allow(clippy::too_many_arguments)]
fn insert_folder(
    conn: &Connection,
    scan: i64,
    parent: Option<i64>,
    path: &str,
    depth: i64,
    size: i64,
    direct: i64,
    access: &str,
    opaque: Option<&str>,
) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO folder_snapshot(scan_id, parent_id, path, depth, size_bytes, direct_bytes, \
         file_count, dir_count, access, opaque_reason, system, junk_hint) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, 0, ?7, ?8, 0, 0)",
        params![scan, parent, path, depth, size, direct, access, opaque],
    )?;
    Ok(conn.last_insert_rowid())
}

fn root_folder(conn: &Connection, scan: i64) -> i64 {
    insert_folder(conn, scan, None, r"C:\", 0, 100, 10, "complete", None).unwrap()
}

fn insert_large(
    conn: &Connection,
    scan: i64,
    folder: i64,
    path: &str,
    size: i64,
    modified: Option<i64>,
) -> rusqlite::Result<usize> {
    conn.execute(
        "INSERT INTO large_file(scan_id, folder_id, path, size_bytes, modified_at) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![scan, folder, path, size, modified],
    )
}

#[test]
fn migration_004_should_leave_user_version_4_with_the_four_tables() {
    let (_dir, _path, conn) = fresh();
    assert_eq!(read_user_version(&conn).unwrap(), 4);
    for table in [
        "folder_scan_result",
        "folder_snapshot",
        "large_file",
        "folder_scan_issue",
    ] {
        assert_eq!(count(&conn, table), 0, "{table}");
    }
}

#[test]
fn folder_snapshot_should_require_a_parent_exactly_when_it_is_not_the_root() {
    let (_dir, _path, conn) = fresh();
    let scan = folder_scan(&conn);
    let root = root_folder(&conn, scan);
    // La raiz con padre y la no raiz sin padre se rechazan.
    assert!(insert_folder(&conn, scan, Some(root), r"C:\x", 0, 5, 5, "complete", None).is_err());
    assert!(insert_folder(&conn, scan, None, r"C:\y", 1, 5, 5, "complete", None).is_err());
    assert!(insert_folder(&conn, scan, Some(root), r"C:\z", 1, 5, 5, "complete", None).is_ok());
}

#[test]
fn folder_snapshot_should_reject_direct_bytes_above_the_total_and_negative_values() {
    let (_dir, _path, conn) = fresh();
    let scan = folder_scan(&conn);
    let root = root_folder(&conn, scan);
    assert!(insert_folder(&conn, scan, Some(root), r"C:\a", 1, 5, 6, "complete", None).is_err());
    assert!(insert_folder(&conn, scan, Some(root), r"C:\b", 1, -1, 0, "complete", None).is_err());
    assert!(insert_folder(&conn, scan, Some(root), r"C:\c", 1, 5, 5, "complete", None).is_ok());
}

#[test]
fn folder_snapshot_should_only_accept_the_closed_access_and_opacity_codes() {
    let (_dir, _path, conn) = fresh();
    let scan = folder_scan(&conn);
    let root = root_folder(&conn, scan);
    for access in ["partial", "", "COMPLETE"] {
        assert!(
            insert_folder(&conn, scan, Some(root), r"C:\a", 1, 5, 5, access, None).is_err(),
            "{access:?}"
        );
    }
    for opaque in ["appdata", "", "Recycle_Bin"] {
        assert!(
            insert_folder(
                &conn,
                scan,
                Some(root),
                r"C:\b",
                1,
                5,
                5,
                "complete",
                Some(opaque)
            )
            .is_err(),
            "{opaque:?}"
        );
    }
    for (path, access, opaque) in [
        (r"C:\ok1", "complete", Some("other_user_profile")),
        (r"C:\ok2", "incomplete", Some("recycle_bin")),
        (r"C:\ok3", "inaccessible", Some("system_volume_information")),
        (r"C:\ok4", "complete", None),
    ] {
        assert!(insert_folder(&conn, scan, Some(root), path, 1, 5, 5, access, opaque).is_ok());
    }
}

#[test]
fn folder_snapshot_should_reject_a_repeated_path_within_a_scan_but_not_across_scans() {
    let (_dir, _path, conn) = fresh();
    let first = folder_scan(&conn);
    let second = folder_scan(&conn);
    root_folder(&conn, first);
    assert!(insert_folder(&conn, first, None, r"C:\", 0, 1, 1, "complete", None).is_err());
    assert!(insert_folder(&conn, second, None, r"C:\", 0, 1, 1, "complete", None).is_ok());
}

#[test]
fn folder_snapshot_should_reject_flags_other_than_zero_or_one() {
    let (_dir, _path, conn) = fresh();
    let scan = folder_scan(&conn);
    for (column, value) in [("system", 2), ("junk_hint", -1)] {
        let sql = format!(
            "INSERT INTO folder_snapshot(scan_id, path, depth, size_bytes, direct_bytes, \
             file_count, dir_count, access, {column}) VALUES (?1, 'C:\\x', 0, 1, 1, 0, 0, 'complete', ?2)"
        );
        assert!(
            conn.execute(&sql, params![scan, value]).is_err(),
            "{column}"
        );
    }
}

#[test]
fn folder_scan_result_should_only_accept_known_root_kinds_and_boolean_truncation() {
    let (_dir, _path, conn) = fresh();
    for kind in ["other", "", "SYSTEM_DRIVE"] {
        let scan = folder_scan(&conn);
        assert!(
            insert_result(&conn, scan, kind, None, None, 0).is_err(),
            "{kind:?}"
        );
    }
    let scan = folder_scan(&conn);
    assert!(insert_result(&conn, scan, "system_drive", None, None, 2).is_err());
    assert!(insert_result(&conn, scan, "system_drive", None, None, 1).is_ok());
    let other = folder_scan(&conn);
    assert!(insert_result(&conn, other, "picked", None, None, 0).is_ok());
}

#[test]
fn folder_scan_result_should_require_both_volume_values_or_none() {
    let (_dir, _path, conn) = fresh();
    for (total, free) in [(Some(100), None), (None, Some(5))] {
        let scan = folder_scan(&conn);
        assert!(
            insert_result(&conn, scan, "system_drive", total, free, 0).is_err(),
            "{total:?}/{free:?}"
        );
    }
    let ok = folder_scan(&conn);
    assert!(insert_result(&conn, ok, "system_drive", Some(100), Some(40), 0).is_ok());
    let zero_total = folder_scan(&conn);
    assert!(insert_result(&conn, zero_total, "system_drive", Some(0), Some(0), 0).is_err());
}

#[test]
fn folder_scan_result_should_reject_negative_counters() {
    let (_dir, _path, conn) = fresh();
    let scan = folder_scan(&conn);
    let sql = "INSERT INTO folder_scan_result(scan_id, root_kind, root_path, total_bytes, \
         file_count, dir_count, inaccessible_dirs, reparse_skipped, cloud_files, opaque_dirs, \
         folders_stored, folders_truncated) VALUES (?1, 'picked', 'C:\\x', ?2, 0, 0, 0, 0, 0, 0, 0, 0)";
    assert!(conn.execute(sql, params![scan, -1]).is_err());
    assert!(conn.execute(sql, params![scan, 0]).is_ok());
}

#[test]
fn large_file_should_reject_empty_files_and_unknown_folders() {
    let (_dir, _path, conn) = fresh();
    let scan = folder_scan(&conn);
    let root = root_folder(&conn, scan);
    assert!(insert_large(&conn, scan, root, r"C:\a.bin", 0, None).is_err());
    assert!(insert_large(&conn, scan, root, r"C:\a.bin", -4, None).is_err());
    assert!(insert_large(&conn, scan, 9_999, r"C:\a.bin", 5, None).is_err());
    assert!(insert_large(&conn, scan, root, r"C:\a.bin", 5, Some(0)).is_err());
    assert!(insert_large(&conn, scan, root, r"C:\a.bin", 5, Some(10)).is_ok());
    assert!(
        insert_large(&conn, scan, root, r"C:\a.bin", 6, None).is_err(),
        "ruta repetida"
    );
    assert!(insert_large(&conn, scan, root, r"C:\b.bin", 6, None).is_ok());
}

#[test]
fn folder_scan_issue_should_only_accept_the_closed_reasons_and_one_row_per_path() {
    let (_dir, _path, conn) = fresh();
    let scan = folder_scan(&conn);
    let insert = |path: &str, reason: &str| {
        conn.execute(
            "INSERT INTO folder_scan_issue(scan_id, path, reason) VALUES (?1, ?2, ?3)",
            params![scan, path, reason],
        )
    };
    assert!(insert(r"C:\a", "timeout").is_err());
    assert!(insert(r"C:\a", "").is_err());
    assert!(insert(r"C:\a", "access_denied").is_ok());
    assert!(
        insert(r"C:\a", "io_error").is_err(),
        "clave primaria (escaneo, ruta)"
    );
    assert!(insert(r"C:\b", "not_found").is_ok());
    assert!(insert(r"C:\c", "io_error").is_ok());
}

#[test]
fn folder_rows_should_be_deleted_in_cascade_with_their_scan_and_leave_the_log_alone() {
    let (_dir, _path, conn) = fresh();
    let scan = folder_scan(&conn);
    insert_result(&conn, scan, "system_drive", Some(100), Some(40), 0).unwrap();
    let root = root_folder(&conn, scan);
    insert_folder(&conn, scan, Some(root), r"C:\a", 1, 5, 5, "complete", None).unwrap();
    insert_large(&conn, scan, root, r"C:\a.bin", 5, None).unwrap();
    conn.execute(
        "INSERT INTO folder_scan_issue(scan_id, path, reason) VALUES (?1, 'C:\\p', 'io_error')",
        [scan],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO cleanup_log(ts, category, path, size_bytes, outcome, scan_id) \
         VALUES (1, 'temp_user', 'C:\\t', 1, 'moved_to_trash', NULL)",
        [],
    )
    .unwrap();
    conn.execute("DELETE FROM scan WHERE id = ?1", [scan])
        .unwrap();
    for table in [
        "folder_scan_result",
        "folder_snapshot",
        "large_file",
        "folder_scan_issue",
    ] {
        assert_eq!(count(&conn, table), 0, "{table}");
    }
    assert_eq!(count(&conn, "cleanup_log"), 1);
}

#[test]
fn deleting_a_folder_should_cascade_to_its_children_and_large_files() {
    let (_dir, _path, conn) = fresh();
    let scan = folder_scan(&conn);
    let root = root_folder(&conn, scan);
    let child = insert_folder(&conn, scan, Some(root), r"C:\a", 1, 5, 5, "complete", None).unwrap();
    insert_folder(
        &conn,
        scan,
        Some(child),
        r"C:\a\b",
        2,
        1,
        1,
        "complete",
        None,
    )
    .unwrap();
    insert_large(&conn, scan, child, r"C:\a\f.bin", 5, None).unwrap();
    conn.execute("DELETE FROM folder_snapshot WHERE id = ?1", [child])
        .unwrap();
    assert_eq!(count(&conn, "folder_snapshot"), 1);
    assert_eq!(count(&conn, "large_file"), 0);
}

fn columns(conn: &Connection, table: &str) -> Vec<String> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .unwrap();
    stmt.query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[test]
fn earlier_tables_should_keep_their_columns() {
    let (_dir, _path, conn) = fresh();
    assert_eq!(
        columns(&conn, "scan"),
        [
            "id",
            "kind",
            "triggered_by",
            "status",
            "started_at",
            "finished_at"
        ]
    );
    assert_eq!(
        columns(&conn, "cleanup_log"),
        [
            "id",
            "ts",
            "category",
            "path",
            "size_bytes",
            "outcome",
            "scan_id",
            "error_detail"
        ]
    );
    assert!(columns(&conn, "junk_finding").contains(&"protected_flag".to_string()));
    assert!(columns(&conn, "duplicate_group").contains(&"blake3".to_string()));
}

fn migrations_up_to(version: u32) -> Vec<super::migrations::Migration> {
    MIGRATIONS
        .iter()
        .copied()
        .filter(|m| m.version <= version)
        .collect()
}

#[test]
fn open_should_migrate_versions_1_2_and_3_to_4_keeping_their_data() {
    for from in [1u32, 2, 3] {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("old.db");
        {
            let mut old = Connection::open(&path).unwrap();
            old.pragma_update(None, "foreign_keys", "ON").unwrap();
            migrate(&mut old, &migrations_up_to(from)).unwrap();
            assert_eq!(read_user_version(&old).unwrap(), from);
            old.execute(
                "UPDATE settings SET value_json = '\"advanced\"' WHERE key = 'ui_mode'",
                [],
            )
            .unwrap();
            old.execute(
                "INSERT INTO scan(kind, triggered_by, status, started_at, finished_at) \
                 VALUES ('full', 'manual', 'done', 1, 2)",
                [],
            )
            .unwrap();
            old.execute(
                "INSERT INTO cleanup_log(ts, category, path, size_bytes, outcome, scan_id) \
                 VALUES (7, 'temp_user', 'C:\\t', 3, 'moved_to_trash', 1)",
                [],
            )
            .unwrap();
        }
        let conn = open_connection(&path).unwrap();
        assert_eq!(read_user_version(&conn).unwrap(), 4, "desde v{from}");
        let mode: String = conn
            .query_row(
                "SELECT value_json FROM settings WHERE key = 'ui_mode'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(mode, "\"advanced\"");
        assert_eq!(count(&conn, "scan"), 1);
        assert_eq!(count(&conn, "cleanup_log"), 1);
        assert_eq!(count(&conn, "folder_snapshot"), 0);
    }
}

#[test]
fn open_should_close_an_interrupted_folders_scan_keeping_the_last_done_tree() {
    use super::SqliteStore;
    use crate::domain::disk_usage::model::LargeFileSort;
    use crate::domain::model::{EpochMillis, ScanStatus};
    use crate::domain::ports::folders_store_contract::{folder_scan, sample};
    use crate::domain::ports::{FolderStore, ScanStore};

    let dir = TempDir::new().unwrap();
    let path = dir.path().join("data.db");
    let (done, running);
    {
        let store = SqliteStore::open(&path).unwrap();
        done = folder_scan(&store, 1_000);
        store
            .complete_folder_scan(done, &sample(), EpochMillis(2_000))
            .unwrap();
        running = folder_scan(&store, 3_000);
    }
    // Restos de un cierre inesperado: todas las filas de un analisis que sigue `running`.
    {
        let conn = Connection::open(&path).unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        insert_result(&conn, running.0, "picked", None, None, 0).unwrap();
        let root = root_folder(&conn, running.0);
        insert_large(&conn, running.0, root, r"C:\left.bin", 5, None).unwrap();
        conn.execute(
            "INSERT INTO folder_scan_issue(scan_id, path, reason) VALUES (?1, 'C:\\p', 'io_error')",
            [running.0],
        )
        .unwrap();
    }
    let store = SqliteStore::open(&path).unwrap();
    let scan = store.get_scan(running).unwrap().unwrap();
    assert_eq!(scan.status, ScanStatus::Error);
    assert!(scan.finished_at.is_some());
    let conn = Connection::open(&path).unwrap();
    for table in [
        "folder_scan_result",
        "folder_snapshot",
        "large_file",
        "folder_scan_issue",
    ] {
        let leftover: i64 = conn
            .query_row(
                &format!("SELECT count(*) FROM {table} WHERE scan_id = ?1"),
                [running.0],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(leftover, 0, "{table}");
    }
    let summary = store.last_done_folder_scan().unwrap().unwrap();
    assert_eq!(summary.scan.id, done);
    assert_eq!(summary.issues.len(), 1);
    assert_eq!(
        store
            .list_large_files(done, LargeFileSort::Size, 0, 10)
            .unwrap()
            .total,
        3
    );
}

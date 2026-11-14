//! Pruebas del esquema de la migracion 003 (C-11) y del adaptador de duplicados sobre archivos
//! temporales.

use rusqlite::{Connection, params};
use tempfile::TempDir;

use super::SqliteStore;
use super::connection::open_connection;
use super::migrations::{MIGRATIONS, migrate, read_user_version};
use crate::domain::duplicates::model::GroupResolution;
use crate::domain::model::{EpochMillis, ScanId};
use crate::domain::ports::duplicates_store_contract as contract;
use crate::domain::ports::{DuplicateStore, ScanStore};

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

fn dup_scan(conn: &Connection) -> i64 {
    conn.execute(
        "INSERT INTO scan(kind, triggered_by, status, started_at, finished_at) \
         VALUES ('duplicates', 'manual', 'done', 1000, 2000)",
        [],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn valid_hash() -> String {
    "ab".repeat(32)
}

fn insert_group(
    conn: &Connection,
    scan: i64,
    size: i64,
    hash: &str,
    file_count: i64,
) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO duplicate_group(scan_id, size_bytes, blake3, file_count) VALUES (?1, ?2, ?3, ?4)",
        params![scan, size, hash, file_count],
    )?;
    Ok(conn.last_insert_rowid())
}

fn insert_file(
    conn: &Connection,
    group: i64,
    root: &str,
    path: &str,
    volume: Option<i64>,
    file_id: &str,
) -> rusqlite::Result<usize> {
    conn.execute(
        "INSERT INTO duplicate_file(group_id, root, path, mtime, volume_serial, file_id) \
         VALUES (?1, ?2, ?3, 5, ?4, ?5)",
        params![group, root, path, volume, file_id],
    )
}

fn fid(n: u8) -> String {
    format!("{n:02x}").repeat(16)
}

#[test]
fn duplicate_group_should_require_a_64_char_lowercase_hex_hash() {
    let (_dir, _path, conn) = fresh();
    let scan = dup_scan(&conn);
    for bad in [
        "abcdef0123".to_string(),
        "A".repeat(64),
        "g".repeat(64),
        "a".repeat(63),
        "a".repeat(65),
        String::new(),
    ] {
        assert!(
            insert_group(&conn, scan, 1_000, &bad, 2).is_err(),
            "{bad:?}"
        );
    }
    assert!(insert_group(&conn, scan, 1_000, &valid_hash(), 2).is_ok());
}

#[test]
fn duplicate_group_should_reject_zero_size_one_copy_and_a_non_positive_resolution_date() {
    let (_dir, _path, conn) = fresh();
    let scan = dup_scan(&conn);
    assert!(insert_group(&conn, scan, 0, &valid_hash(), 2).is_err());
    assert!(insert_group(&conn, scan, 1_000, &valid_hash(), 1).is_err());
    let id = insert_group(&conn, scan, 1_000, &valid_hash(), 2).unwrap();
    for bad in [0i64, -5] {
        assert!(
            conn.execute(
                "UPDATE duplicate_group SET resolved_at = ?1 WHERE id = ?2",
                params![bad, id]
            )
            .is_err(),
            "{bad}"
        );
    }
    assert_eq!(
        conn.execute(
            "UPDATE duplicate_group SET resolved_at = 5 WHERE id = ?1",
            [id]
        )
        .unwrap(),
        1
    );
}

#[test]
fn duplicate_group_should_be_unique_per_scan_size_and_hash_only() {
    let (_dir, _path, conn) = fresh();
    let first = dup_scan(&conn);
    let second = dup_scan(&conn);
    insert_group(&conn, first, 1_000, &valid_hash(), 2).unwrap();
    assert!(insert_group(&conn, first, 1_000, &valid_hash(), 2).is_err());
    assert!(insert_group(&conn, first, 2_000, &valid_hash(), 2).is_ok());
    assert!(insert_group(&conn, second, 1_000, &valid_hash(), 2).is_ok());
}

#[test]
fn duplicate_file_should_require_the_file_identity_with_a_32_char_id() {
    let (_dir, _path, conn) = fresh();
    let scan = dup_scan(&conn);
    let group = insert_group(&conn, scan, 1_000, &valid_hash(), 2).unwrap();
    // Sin volumen.
    assert!(insert_file(&conn, group, "downloads", "C:/a", None, &fid(1)).is_err());
    // Identificador de 31, 33 y 0 caracteres.
    for bad in ["f".repeat(31), "f".repeat(33), String::new()] {
        assert!(
            insert_file(&conn, group, "downloads", "C:/a", Some(1), &bad).is_err(),
            "{bad:?}"
        );
    }
    // Sin identificador (NULL).
    assert!(
        conn.execute(
            "INSERT INTO duplicate_file(group_id, root, path, volume_serial) VALUES (?1, 'downloads', 'C:/a', 1)",
            [group]
        )
        .is_err()
    );
    assert!(insert_file(&conn, group, "downloads", "C:/a", Some(1), &fid(1)).is_ok());
}

#[test]
fn duplicate_tables_should_reject_an_unknown_content_folder() {
    let (_dir, _path, conn) = fresh();
    let scan = dup_scan(&conn);
    let group = insert_group(&conn, scan, 1_000, &valid_hash(), 2).unwrap();
    assert!(insert_file(&conn, group, "appdata", "C:/a", Some(1), &fid(1)).is_err());
    assert!(insert_file(&conn, group, "Downloads", "C:/a", Some(1), &fid(1)).is_err());
    assert!(
        conn.execute(
            "INSERT INTO duplicate_scan_root(scan_id, root, status, files_seen) VALUES (?1, 'appdata', 'done', 0)",
            [scan]
        )
        .is_err()
    );
    for root in [
        "desktop",
        "documents",
        "downloads",
        "music",
        "pictures",
        "videos",
    ] {
        assert!(
            conn.execute(
                "INSERT INTO duplicate_scan_root(scan_id, root, status, files_seen) VALUES (?1, ?2, 'done', 0)",
                params![scan, root]
            )
            .is_ok(),
            "{root}"
        );
    }
}

#[test]
fn duplicate_file_should_default_kept_to_zero_and_accept_only_zero_or_one() {
    let (_dir, _path, conn) = fresh();
    let scan = dup_scan(&conn);
    let group = insert_group(&conn, scan, 1_000, &valid_hash(), 2).unwrap();
    insert_file(&conn, group, "downloads", "C:/a", Some(1), &fid(1)).unwrap();
    let kept: i64 = conn
        .query_row("SELECT kept FROM duplicate_file", [], |r| r.get(0))
        .unwrap();
    assert_eq!(kept, 0);
    assert!(
        conn.execute("UPDATE duplicate_file SET kept = 2", [])
            .is_err()
    );
    assert!(
        conn.execute("UPDATE duplicate_file SET kept = 1", [])
            .is_ok()
    );
}

#[test]
fn duplicate_file_should_reject_a_repeated_identity_or_path_inside_a_group_only() {
    let (_dir, _path, conn) = fresh();
    let scan = dup_scan(&conn);
    let one = insert_group(&conn, scan, 1_000, &valid_hash(), 2).unwrap();
    let two = insert_group(&conn, scan, 2_000, &valid_hash(), 2).unwrap();
    insert_file(&conn, one, "downloads", "C:/a", Some(1), &fid(1)).unwrap();
    // Misma identidad con otra ruta, y misma ruta con otra identidad.
    assert!(insert_file(&conn, one, "downloads", "C:/b", Some(1), &fid(1)).is_err());
    assert!(insert_file(&conn, one, "downloads", "C:/a", Some(1), &fid(2)).is_err());
    // Otro volumen con el mismo id de archivo si es otro archivo.
    assert!(insert_file(&conn, one, "downloads", "C:/c", Some(2), &fid(1)).is_ok());
    // En otro grupo la misma ruta o identidad es valida.
    assert!(insert_file(&conn, two, "downloads", "C:/a", Some(1), &fid(1)).is_ok());
}

#[test]
fn duplicate_scan_root_should_require_a_reason_exactly_when_unavailable() {
    let (_dir, _path, conn) = fresh();
    let scan = dup_scan(&conn);
    let insert = |root: &str, status: &str, reason: Option<&str>| {
        conn.execute(
            "INSERT INTO duplicate_scan_root(scan_id, root, status, reason, files_seen) VALUES (?1, ?2, ?3, ?4, 0)",
            params![scan, root, status, reason],
        )
    };
    assert!(insert("desktop", "unavailable", None).is_err());
    assert!(insert("desktop", "done", Some("cloud")).is_err());
    assert!(insert("desktop", "nested", Some("cloud")).is_err());
    assert!(insert("desktop", "gone", None).is_err());
    assert!(insert("desktop", "unavailable", Some("unsupported_volume")).is_ok());
    assert!(insert("documents", "done", None).is_ok());
    assert!(insert("music", "not_selected", None).is_ok());
    assert!(insert("videos", "nested", None).is_ok());
    // Un estado por carpeta y escaneo.
    assert!(insert("videos", "done", None).is_err());
}

#[test]
fn duplicate_scan_result_should_only_accept_the_closed_min_sizes_and_non_negative_counters() {
    let (_dir, _path, conn) = fresh();
    let scan = dup_scan(&conn);
    let insert = |scan: i64, min: i64, hashed: i64| {
        conn.execute(
            "INSERT INTO duplicate_scan_result(scan_id, min_size_bytes, files_seen, below_min_size, \
             protected_count, cloud_count, hard_linked_count, changed_count, unreadable_count, \
             inaccessible_dirs, excluded_dirs, hashed_bytes) VALUES (?1, ?2, 0, 0, 0, 0, 0, 0, 0, 0, 0, ?3)",
            params![scan, min, hashed],
        )
    };
    assert!(insert(scan, 5, 0).is_err());
    assert!(insert(scan, 0, 0).is_err());
    assert!(insert(scan, 1_048_576, -1).is_err());
    // Un escaneo desconocido.
    assert!(insert(9_999, 1_048_576, 0).is_err());
    for min in [102_400, 1_048_576, 10_485_760, 104_857_600] {
        let id = dup_scan(&conn);
        assert!(insert(id, min, 0).is_ok(), "{min}");
    }
    // Uno por escaneo.
    assert!(insert(scan, 1_048_576, 0).is_ok());
    assert!(insert(scan, 1_048_576, 0).is_err());
}

#[test]
fn duplicate_rows_should_be_deleted_in_cascade_with_their_scan_leaving_the_log_alone() {
    let (_dir, _path, conn) = fresh();
    let scan = dup_scan(&conn);
    conn.execute(
        "INSERT INTO duplicate_scan_result(scan_id, min_size_bytes, files_seen, below_min_size, \
         protected_count, cloud_count, hard_linked_count, changed_count, unreadable_count, \
         inaccessible_dirs, excluded_dirs, hashed_bytes) VALUES (?1, 1048576, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)",
        [scan],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO duplicate_scan_root(scan_id, root, status, files_seen) VALUES (?1, 'downloads', 'done', 0)",
        [scan],
    )
    .unwrap();
    let group = insert_group(&conn, scan, 1_000, &valid_hash(), 2).unwrap();
    insert_file(&conn, group, "downloads", "C:/a", Some(1), &fid(1)).unwrap();
    insert_file(&conn, group, "downloads", "C:/b", Some(2), &fid(2)).unwrap();
    conn.execute(
        "INSERT INTO cleanup_log(ts, category, path, size_bytes, outcome, scan_id) \
         VALUES (1700000000000, 'duplicates', 'C:/a', 1, 'moved_to_trash', ?1)",
        [scan],
    )
    .unwrap();
    conn.execute("DELETE FROM scan WHERE id = ?1", [scan])
        .unwrap();
    for table in [
        "duplicate_scan_result",
        "duplicate_scan_root",
        "duplicate_group",
        "duplicate_file",
    ] {
        assert_eq!(count(&conn, table), 0, "{table}");
    }
    assert_eq!(count(&conn, "cleanup_log"), 1);
}

#[test]
fn cleanup_log_should_be_byte_identical_to_the_one_of_schema_2() {
    let dir = TempDir::new().unwrap();
    let mut v2 = Connection::open(dir.path().join("v2.db")).unwrap();
    migrate(&mut v2, &MIGRATIONS[..2]).unwrap();
    let (_d, _p, v3) = fresh();
    let definition = |conn: &Connection| -> Vec<String> {
        let mut stmt = conn
            .prepare(
                "SELECT sql FROM sqlite_master WHERE tbl_name = 'cleanup_log' ORDER BY type, name",
            )
            .unwrap();
        stmt.query_map([], |r| r.get::<_, Option<String>>(0))
            .unwrap()
            .map(|r| r.unwrap().unwrap_or_default())
            .collect()
    };
    assert_eq!(definition(&v2), definition(&v3));
    assert!(!definition(&v3).is_empty());
    // `duplicates` entra como categoria libre del log, sin migrar.
    v3.execute(
        "INSERT INTO cleanup_log(ts, category, path, size_bytes, outcome) \
         VALUES (1700000000000, 'duplicates', 'C:/a', 1, 'moved_to_trash')",
        [],
    )
    .unwrap();
}

#[test]
fn open_should_migrate_a_version_2_database_keeping_findings_and_log() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("data.db");
    {
        let mut v2 = Connection::open(&path).unwrap();
        migrate(&mut v2, &MIGRATIONS[..2]).unwrap();
        assert_eq!(read_user_version(&v2).unwrap(), 2);
        v2.execute(
            "INSERT INTO scan(kind, triggered_by, status, started_at, finished_at) VALUES ('junk', 'manual', 'done', 1000, 2000)",
            [],
        )
        .unwrap();
        v2.execute(
            "INSERT INTO junk_finding(scan_id, category, path, size_bytes, protected_flag, volume_serial, file_id) \
             VALUES (1, 'temp_user', 'C:/t/a', 10, 0, 1, ?1)",
            ["a".repeat(32)],
        )
        .unwrap();
        v2.execute(
            "INSERT INTO cleanup_log(ts, category, path, size_bytes, outcome) VALUES (1700000000000, 'temp_user', 'C:/t/a', 10, 'moved_to_trash')",
            [],
        )
        .unwrap();
    }
    let conn = open_connection(&path).unwrap();
    assert_eq!(read_user_version(&conn).unwrap(), 3);
    assert_eq!(count(&conn, "scan"), 1);
    assert_eq!(count(&conn, "junk_finding"), 1);
    assert_eq!(count(&conn, "cleanup_log"), 1);
    assert_eq!(count(&conn, "duplicate_group"), 0);
}

#[test]
fn a_completed_duplicates_scan_should_keep_its_result_after_a_newer_scan_purges_its_groups() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("data.db");
    let store = SqliteStore::open(&path).unwrap();
    let first = contract::dup_scan(&store, 1_000);
    store
        .complete_duplicate_scan(
            first,
            &contract::results(vec![contract::group(4 << 20, 1, "a", 2, 100)]),
            EpochMillis(2_000),
        )
        .unwrap();
    let second = contract::dup_scan(&store, 3_000);
    store
        .complete_duplicate_scan(
            second,
            &contract::results(vec![contract::group(5 << 20, 2, "b", 2, 200)]),
            EpochMillis(4_000),
        )
        .unwrap();
    let conn = Connection::open(&path).unwrap();
    assert_eq!(count(&conn, "duplicate_scan_result"), 2);
    assert_eq!(count(&conn, "duplicate_scan_root"), 12);
    assert_eq!(count(&conn, "duplicate_group"), 1);
    assert_eq!(count(&conn, "duplicate_file"), 2);
}

#[test]
fn a_saved_scan_should_persist_the_copies_with_kept_zero_and_the_identity_bits() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("data.db");
    let store = SqliteStore::open(&path).unwrap();
    let scan = contract::dup_scan(&store, 1_000);
    store
        .complete_duplicate_scan(
            scan,
            &contract::results(vec![contract::group(4 << 20, 1, "a", 2, 100)]),
            EpochMillis(2_000),
        )
        .unwrap();
    let conn = Connection::open(&path).unwrap();
    let kept: i64 = conn
        .query_row("SELECT sum(kept) FROM duplicate_file", [], |r| r.get(0))
        .unwrap();
    assert_eq!(kept, 0);
    let file_count: i64 = conn
        .query_row("SELECT file_count FROM duplicate_group", [], |r| r.get(0))
        .unwrap();
    assert_eq!(file_count, 2);
    // El volumen u64 con el bit alto se guarda como i64 negativo y vuelve igual.
    let volume: i64 = conn
        .query_row(
            "SELECT volume_serial FROM duplicate_file LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(volume < 0);
}

#[test]
fn open_should_close_an_interrupted_duplicates_scan_without_touching_the_last_done_groups() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("data.db");
    let (done, running);
    {
        let store = SqliteStore::open(&path).unwrap();
        done = contract::dup_scan(&store, 1_000);
        store
            .complete_duplicate_scan(
                done,
                &contract::results(vec![contract::group(4 << 20, 1, "a", 2, 100)]),
                EpochMillis(2_000),
            )
            .unwrap();
        running = contract::dup_scan(&store, 3_000);
    }
    // Un resto de un cierre inesperado: resultado y raiz de un escaneo que sigue `running`.
    {
        let conn = Connection::open(&path).unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        conn.execute(
            "INSERT INTO duplicate_scan_root(scan_id, root, status, files_seen) VALUES (?1, 'music', 'done', 0)",
            [running.0],
        )
        .unwrap();
    }
    let store = SqliteStore::open(&path).unwrap();
    let scan = store.get_scan(running).unwrap().unwrap();
    assert_eq!(scan.status, crate::domain::model::ScanStatus::Error);
    assert!(scan.finished_at.is_some());
    let conn = Connection::open(&path).unwrap();
    let roots_of_running: i64 = conn
        .query_row(
            "SELECT count(*) FROM duplicate_scan_root WHERE scan_id = ?1",
            [running.0],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(roots_of_running, 0);
    assert_eq!(
        store
            .list_duplicate_groups(done, 0, 10, None, None)
            .unwrap()
            .total,
        1
    );
    assert_eq!(
        store.last_done_duplicate_scan().unwrap().unwrap().scan.id,
        done
    );
}

#[test]
fn resolve_should_write_kept_only_through_the_resolution_statement() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("data.db");
    let store = SqliteStore::open(&path).unwrap();
    let scan = contract::dup_scan(&store, 1_000);
    store
        .complete_duplicate_scan(
            scan,
            &contract::results(vec![contract::group(4 << 20, 1, "a", 2, 100)]),
            EpochMillis(2_000),
        )
        .unwrap();
    let group = store
        .list_duplicate_groups(scan, 0, 5, None, None)
        .unwrap()
        .groups[0]
        .clone();
    store
        .resolve_duplicate_groups(
            ScanId(scan.0),
            &[GroupResolution {
                group_id: group.id,
                kept_file_ids: vec![group.files[0].id],
            }],
            EpochMillis(3_000),
        )
        .unwrap();
    let conn = Connection::open(&path).unwrap();
    let kept: Vec<i64> = {
        let mut stmt = conn
            .prepare("SELECT kept FROM duplicate_file ORDER BY id")
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    assert_eq!(kept, [1, 0]);
}

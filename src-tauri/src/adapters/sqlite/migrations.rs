//! Migraciones versionadas por `PRAGMA user_version`.

use rusqlite::{Connection, TransactionBehavior};

use super::errors::map_sqlite_error;
use crate::domain::ports::StoreError;

/// Una migracion: `version` estrictamente creciente y contigua desde 1.
#[derive(Debug, Clone, Copy)]
pub struct Migration {
    pub version: u32,
    pub sql: &'static str,
}

/// Migraciones publicadas. NUNCA editar una existente: agregar `00N_*.sql`.
pub const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    sql: include_str!("migrations/001_init.sql"),
}];

/// Version mas alta de la lista.
pub fn latest_version(migrations: &[Migration]) -> u32 {
    migrations.iter().map(|m| m.version).max().unwrap_or(0)
}

/// Lee `PRAGMA user_version`.
pub fn read_user_version(conn: &Connection) -> Result<u32, StoreError> {
    let raw: i64 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(map_sqlite_error)?;
    u32::try_from(raw).map_err(|_| StoreError::Corrupt(format!("invalid user_version {raw}")))
}

/// Aplica las migraciones pendientes, cada una en su transaccion. Devuelve la version final.
pub fn migrate(conn: &mut Connection, migrations: &[Migration]) -> Result<u32, StoreError> {
    let supported = latest_version(migrations);
    let start = read_user_version(conn)?;
    let mut current = start;
    if start > supported {
        return Err(StoreError::SchemaTooNew {
            found: start,
            supported,
        });
    }
    for migration in migrations.iter().filter(|m| m.version > start) {
        apply(conn, migration).map_err(|error| StoreError::MigrationFailed {
            version: migration.version,
            detail: match error {
                StoreError::Unavailable(detail) | StoreError::Constraint(detail) => detail,
                other => other.to_string(),
            },
        })?;
        current = migration.version;
    }
    Ok(current)
}

/// Aplica una migracion y fija `user_version` en la misma transaccion; si algo falla el
/// `Drop` de la transaccion hace rollback.
fn apply(conn: &mut Connection, migration: &Migration) -> Result<(), StoreError> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(map_sqlite_error)?;
    tx.execute_batch(migration.sql).map_err(map_sqlite_error)?;
    tx.pragma_update(None, "user_version", migration.version)
        .map_err(map_sqlite_error)?;
    tx.commit().map_err(map_sqlite_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem() -> Connection {
        Connection::open_in_memory().unwrap()
    }

    fn table_exists(conn: &Connection, name: &str) -> bool {
        conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1",
            [name],
            |r| r.get::<_, i64>(0),
        )
        .unwrap()
            == 1
    }

    const V1: Migration = Migration {
        version: 1,
        sql: "CREATE TABLE a(x INTEGER); INSERT INTO a VALUES (1);",
    };
    const V2: Migration = Migration {
        version: 2,
        sql: "CREATE TABLE b(y INTEGER);",
    };

    #[test]
    fn migrate_should_apply_all_on_fresh_database() {
        let mut conn = mem();
        assert_eq!(migrate(&mut conn, &[V1, V2]).unwrap(), 2);
        assert_eq!(read_user_version(&conn).unwrap(), 2);
        assert!(table_exists(&conn, "a") && table_exists(&conn, "b"));
    }

    #[test]
    fn migrate_should_be_idempotent() {
        let mut conn = mem();
        migrate(&mut conn, &[V1]).unwrap();
        assert_eq!(migrate(&mut conn, &[V1]).unwrap(), 1);
        // V1 inserta una fila: si se reaplicara, habria 2 (o fallaria por la tabla existente).
        let rows: i64 = conn
            .query_row("SELECT count(*) FROM a", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 1);
    }

    #[test]
    fn migrate_should_apply_only_pending_versions() {
        let mut conn = mem();
        migrate(&mut conn, &[V1]).unwrap();
        assert_eq!(migrate(&mut conn, &[V1, V2]).unwrap(), 2);
        assert!(table_exists(&conn, "b"));
        let rows: i64 = conn
            .query_row("SELECT count(*) FROM a", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 1);
    }

    #[test]
    fn migrate_should_reject_newer_schema_without_writing() {
        let mut conn = mem();
        conn.pragma_update(None, "user_version", 99).unwrap();
        let err = migrate(&mut conn, &[V1, V2]).unwrap_err();
        assert_eq!(
            err,
            StoreError::SchemaTooNew {
                found: 99,
                supported: 2
            }
        );
        assert_eq!(read_user_version(&conn).unwrap(), 99);
        assert!(!table_exists(&conn, "a"));
    }

    #[test]
    fn migrate_should_roll_back_a_failing_migration_completely() {
        let mut conn = mem();
        migrate(&mut conn, &[V1]).unwrap();
        let bad = Migration {
            version: 2,
            sql: "CREATE TABLE c(z INTEGER); INSERT INTO no_such_table VALUES (1);",
        };
        let err = migrate(&mut conn, &[V1, bad]).unwrap_err();
        assert!(
            matches!(err, StoreError::MigrationFailed { version: 2, .. }),
            "{err:?}"
        );
        assert_eq!(read_user_version(&conn).unwrap(), 1);
        assert!(!table_exists(&conn, "c"));
    }

    #[test]
    fn real_migrations_should_be_contiguous_from_one_with_sql() {
        for (i, m) in MIGRATIONS.iter().enumerate() {
            assert_eq!(m.version as usize, i + 1);
            assert!(!m.sql.trim().is_empty(), "migration {}", m.version);
        }
        assert_eq!(latest_version(MIGRATIONS) as usize, MIGRATIONS.len());
        assert_eq!(latest_version(&[]), 0);
    }
}

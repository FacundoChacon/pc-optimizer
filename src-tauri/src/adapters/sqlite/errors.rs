//! Traduccion de errores de `rusqlite` a `StoreError` sin filtrar SQL.

use rusqlite::ErrorCode;

use crate::domain::ports::StoreError;

/// Mapea un error de SQLite. El detalle es el nombre del codigo de error y, para
/// violaciones de restriccion, el mensaje de la restriccion; nunca el texto de la sentencia.
pub fn map_sqlite_error(error: rusqlite::Error) -> StoreError {
    let rusqlite::Error::SqliteFailure(failure, message) = error else {
        return StoreError::Unavailable("unexpected database error".into());
    };
    let code_name = format!("{:?}", failure.code);
    match failure.code {
        ErrorCode::NotADatabase | ErrorCode::DatabaseCorrupt => StoreError::Corrupt(code_name),
        ErrorCode::ConstraintViolation => {
            StoreError::Constraint(message.filter(|m| !mentions_sql(m)).unwrap_or(code_name))
        }
        _ => StoreError::Unavailable(code_name),
    }
}

/// `true` si el mensaje parece contener SQL (defensa en profundidad).
fn mentions_sql(message: &str) -> bool {
    let upper = message.to_ascii_uppercase();
    ["SELECT", "INSERT", "UPDATE", "DELETE"]
        .iter()
        .any(|keyword| upper.contains(keyword))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::ffi;

    fn failure(code: std::os::raw::c_int, msg: &str) -> rusqlite::Error {
        rusqlite::Error::SqliteFailure(ffi::Error::new(code), Some(msg.to_string()))
    }

    #[test]
    fn map_should_classify_by_error_code() {
        use std::mem::discriminant as kind;
        let corrupt = kind(&StoreError::Corrupt(String::new()));
        let constraint = kind(&StoreError::Constraint(String::new()));
        let unavailable = kind(&StoreError::Unavailable(String::new()));
        let cases = [
            (ffi::SQLITE_NOTADB, corrupt),
            (ffi::SQLITE_CORRUPT, corrupt),
            (ffi::SQLITE_CONSTRAINT, constraint),
            (ffi::SQLITE_BUSY, unavailable),
            (ffi::SQLITE_CANTOPEN, unavailable),
            (ffi::SQLITE_FULL, unavailable),
        ];
        for (code, expected) in cases {
            let mapped = map_sqlite_error(failure(code, "detalle"));
            assert_eq!(kind(&mapped), expected, "code {code} -> {mapped:?}");
        }
    }

    #[test]
    fn map_should_keep_constraint_message_but_never_leak_sql() {
        let mapped = map_sqlite_error(failure(
            ffi::SQLITE_CONSTRAINT,
            "cleanup_log is append-only",
        ));
        assert_eq!(
            mapped,
            StoreError::Constraint("cleanup_log is append-only".into())
        );

        let sql = "near \"SELECT * FROM x\": INSERT UPDATE DELETE";
        for code in [
            ffi::SQLITE_NOTADB,
            ffi::SQLITE_CONSTRAINT,
            ffi::SQLITE_BUSY,
            ffi::SQLITE_ERROR,
        ] {
            let text = map_sqlite_error(failure(code, sql))
                .to_string()
                .to_uppercase();
            for keyword in ["SELECT", "INSERT", "UPDATE", "DELETE"] {
                assert!(!text.contains(keyword), "code {code}: {text}");
            }
        }
    }

    #[test]
    fn map_should_treat_non_sqlite_errors_as_unavailable() {
        let mapped = map_sqlite_error(rusqlite::Error::QueryReturnedNoRows);
        assert!(matches!(mapped, StoreError::Unavailable(_)));
    }
}

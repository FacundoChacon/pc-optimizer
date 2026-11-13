//! Errores de la aplicacion, serializables hacia el frontend como `{ kind, message }`.

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

use crate::domain::ports::StoreError;

/// Error de aplicacion devuelto por los comandos IPC.
///
/// Cada variante tiene un `kind` estable en `snake_case`; el `message` es el `Display`.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// Fallo interno inesperado.
    #[error("internal error: {0}")]
    Internal(String),
    /// La base de datos local esta danada.
    #[error("the local database is corrupt: {0}")]
    StoreCorrupt(String),
    /// La base fue creada por una version mas nueva de la app.
    #[error("the local database schema {found} is newer than supported {supported}")]
    StoreSchemaTooNew { found: u32, supported: u32 },
    /// Fallo al migrar el esquema de la base.
    #[error("the local database migration {version} failed: {detail}")]
    StoreMigrationFailed { version: u32, detail: String },
    /// La base no esta disponible (ruta, permisos, bloqueo).
    #[error("the local database is unavailable: {0}")]
    StoreUnavailable(String),
}

impl From<StoreError> for AppError {
    fn from(error: StoreError) -> Self {
        match error {
            StoreError::Corrupt(detail) => Self::StoreCorrupt(detail),
            StoreError::SchemaTooNew { found, supported } => {
                Self::StoreSchemaTooNew { found, supported }
            }
            StoreError::MigrationFailed { version, detail } => {
                Self::StoreMigrationFailed { version, detail }
            }
            StoreError::Unavailable(detail) => Self::StoreUnavailable(detail),
            // Errores de uso o validacion: C-04 los tipara al exponer comandos.
            other @ (StoreError::NotFound(_)
            | StoreError::InvalidState(_)
            | StoreError::Constraint(_)) => Self::Internal(other.to_string()),
        }
    }
}

impl AppError {
    fn kind(&self) -> &'static str {
        match self {
            Self::Internal(_) => "internal",
            Self::StoreCorrupt(_) => "store_corrupt",
            Self::StoreSchemaTooNew { .. } => "store_schema_too_new",
            Self::StoreMigrationFailed { .. } => "store_migration_failed",
            Self::StoreUnavailable(_) => "store_unavailable",
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct("AppError", 2)?;
        state.serialize_field("kind", self.kind())?;
        state.serialize_field("message", &self.to_string())?;
        state.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn internal_should_serialize_with_stable_kind_and_display_message() {
        let err = AppError::Internal("x".into());
        let value = serde_json::to_value(&err).unwrap();
        assert_eq!(
            value,
            json!({ "kind": "internal", "message": err.to_string() })
        );
        assert!(err.to_string().contains('x'));
    }

    #[test]
    fn store_errors_should_map_to_stable_kinds() {
        let cases = [
            (StoreError::Corrupt("NotADatabase".into()), "store_corrupt"),
            (
                StoreError::SchemaTooNew {
                    found: 9,
                    supported: 1,
                },
                "store_schema_too_new",
            ),
            (
                StoreError::MigrationFailed {
                    version: 2,
                    detail: "x".into(),
                },
                "store_migration_failed",
            ),
            (
                StoreError::Unavailable("DatabaseBusy".into()),
                "store_unavailable",
            ),
            (StoreError::NotFound("scan 1".into()), "internal"),
            (StoreError::InvalidState("done".into()), "internal"),
            (StoreError::Constraint("empty key".into()), "internal"),
        ];
        for (store_error, kind) in cases {
            let value = serde_json::to_value(AppError::from(store_error.clone())).unwrap();
            assert_eq!(value["kind"], kind, "{store_error:?}");
        }
    }

    #[test]
    fn store_errors_should_keep_their_details_and_never_expose_sql() {
        let too_new = serde_json::to_value(AppError::from(StoreError::SchemaTooNew {
            found: 9,
            supported: 1,
        }))
        .unwrap();
        let message = too_new["message"].as_str().unwrap();
        assert!(message.contains('9') && message.contains('1'), "{message}");

        let all = [
            StoreError::Corrupt("NotADatabase".into()),
            StoreError::MigrationFailed {
                version: 2,
                detail: "ConstraintViolation".into(),
            },
            StoreError::Unavailable("DatabaseBusy".into()),
            StoreError::Constraint("cleanup_log is append-only".into()),
        ];
        for store_error in all {
            let value = serde_json::to_value(AppError::from(store_error)).unwrap();
            let upper = value["message"].as_str().unwrap().to_ascii_uppercase();
            for keyword in ["SELECT", "INSERT", "UPDATE", "DELETE"] {
                assert!(!upper.contains(keyword), "{upper}");
            }
        }
    }

    #[test]
    fn different_messages_should_yield_different_messages_and_same_kind() {
        let a = serde_json::to_value(AppError::Internal("uno".into())).unwrap();
        let b = serde_json::to_value(AppError::Internal("dos".into())).unwrap();
        assert_ne!(a["message"], b["message"]);
        assert_eq!(a["kind"], b["kind"]);
        assert_eq!(a["kind"], "internal");
    }
}

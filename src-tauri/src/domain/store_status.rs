//! Estado de la base local visto por el dominio (puro, sin I/O).

use crate::domain::ports::StoreError;

/// Motivo por el que la base no esta disponible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnavailableReason {
    Corrupt,
    SchemaTooNew,
    MigrationFailed,
    Unavailable,
}

impl UnavailableReason {
    /// Cadena estable usada en el contrato IPC.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Corrupt => "corrupt",
            Self::SchemaTooNew => "schema_too_new",
            Self::MigrationFailed => "migration_failed",
            Self::Unavailable => "unavailable",
        }
    }
}

/// Estado de la base: lista, o no disponible (con o sin recuperacion posible).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreStatus {
    Ready,
    Unavailable {
        reason: UnavailableReason,
        recoverable: bool,
    },
}

/// Clasifica el error de apertura. Solo `Corrupt` y `MigrationFailed` con ruta conocida
/// son recuperables (respaldo y recreacion).
pub fn status_of(error: &StoreError, has_path: bool) -> StoreStatus {
    let (reason, can_recover) = match error {
        StoreError::Corrupt(_) => (UnavailableReason::Corrupt, true),
        StoreError::MigrationFailed { .. } => (UnavailableReason::MigrationFailed, true),
        StoreError::SchemaTooNew { .. } => (UnavailableReason::SchemaTooNew, false),
        StoreError::Unavailable(_)
        | StoreError::NotFound(_)
        | StoreError::InvalidState(_)
        | StoreError::Constraint(_) => (UnavailableReason::Unavailable, false),
    };
    StoreStatus::Unavailable {
        reason,
        recoverable: can_recover && has_path,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unavailable(reason: UnavailableReason, recoverable: bool) -> StoreStatus {
        StoreStatus::Unavailable {
            reason,
            recoverable,
        }
    }

    #[test]
    fn status_of_should_classify_every_error_kind() {
        let migration = StoreError::MigrationFailed {
            version: 2,
            detail: "x".into(),
        };
        let too_new = StoreError::SchemaTooNew {
            found: 9,
            supported: 1,
        };
        let cases = [
            (
                StoreError::Corrupt("x".into()),
                true,
                unavailable(UnavailableReason::Corrupt, true),
            ),
            (
                migration.clone(),
                true,
                unavailable(UnavailableReason::MigrationFailed, true),
            ),
            (
                StoreError::Corrupt("x".into()),
                false,
                unavailable(UnavailableReason::Corrupt, false),
            ),
            (
                migration,
                false,
                unavailable(UnavailableReason::MigrationFailed, false),
            ),
            (
                too_new,
                true,
                unavailable(UnavailableReason::SchemaTooNew, false),
            ),
            (
                StoreError::Unavailable("busy".into()),
                true,
                unavailable(UnavailableReason::Unavailable, false),
            ),
            (
                StoreError::NotFound("x".into()),
                true,
                unavailable(UnavailableReason::Unavailable, false),
            ),
            (
                StoreError::InvalidState("x".into()),
                true,
                unavailable(UnavailableReason::Unavailable, false),
            ),
            (
                StoreError::Constraint("x".into()),
                true,
                unavailable(UnavailableReason::Unavailable, false),
            ),
        ];
        for (error, has_path, expected) in cases {
            assert_eq!(
                status_of(&error, has_path),
                expected,
                "{error:?}/{has_path}"
            );
        }
    }

    #[test]
    fn reason_should_expose_the_exact_spec_strings() {
        assert_eq!(UnavailableReason::Corrupt.as_str(), "corrupt");
        assert_eq!(UnavailableReason::SchemaTooNew.as_str(), "schema_too_new");
        assert_eq!(
            UnavailableReason::MigrationFailed.as_str(),
            "migration_failed"
        );
        assert_eq!(UnavailableReason::Unavailable.as_str(), "unavailable");
    }
}

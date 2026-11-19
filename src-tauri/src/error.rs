//! Errores de la aplicacion, serializables hacia el frontend como `{ kind, message }`.

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

use crate::adapters::sqlite::state::RecoveryError;
use crate::domain::disk_usage::model::RevealReason;
use crate::domain::ports::StoreError;
use crate::domain::privileges::MismatchReason;
use crate::domain::settings::{SettingsError, SettingsServiceError};

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
    /// Valor o clave de ajuste invalidos.
    #[error("invalid setting: {0}")]
    SettingInvalid(String),
    /// El ajuste existe pero no es escribible en esta version.
    #[error("setting is read-only: {0}")]
    SettingReadOnly(String),
    /// La base no necesita o no admite recuperacion.
    #[error("the local database cannot be recovered: {0}")]
    StoreNotRecoverable(String),
    /// Ya hay un escaneo en curso (solo se admite uno a la vez).
    #[error("a scan is already in progress")]
    ScanInProgress,
    /// El escaneo pedido no existe, no esta en curso o no es el ultimo terminado.
    #[error("scan not found: {0}")]
    ScanNotFound(String),
    /// El tipo de escaneo pedido no esta soportado en esta version.
    #[error("unsupported scan kind: {0}")]
    ScanKindUnsupported(String),
    /// Argumento fuera de rango o desconocido.
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    /// El preview pedido no existe, ya se uso, fue reemplazado o no esta en ejecucion.
    #[error("cleanup preview not found")]
    PreviewNotFound,
    /// El preview supero su tiempo de vida (10 minutos).
    #[error("cleanup preview expired")]
    PreviewExpired,
    /// Desde el preview termino otro escaneo de basura o su escaneo fue limpiado.
    #[error("cleanup preview is stale")]
    PreviewStale,
    /// El escaneo ya tiene una limpieza registrada: hay que volver a escanear.
    #[error("the scan was already cleaned")]
    ScanAlreadyCleaned,
    /// Ya hay una limpieza en curso.
    #[error("a cleanup is already in progress")]
    CleanupInProgress,
    /// El preview no tiene ningun item para mover.
    #[error("there is nothing to clean")]
    NothingToClean,
    /// La solicitud de vaciado no existe, fue reemplazada, ya se uso o el token no corresponde.
    #[error("recycle bin empty request not found")]
    EmptyRequestNotFound,
    /// La solicitud de vaciado supero su tiempo de vida (2 minutos).
    #[error("recycle bin empty request expired")]
    EmptyRequestExpired,
    /// Falta la primera confirmacion del vaciado.
    #[error("recycle bin empty request not acknowledged")]
    EmptyNotAcknowledged,
    /// La Papelera cambio desde que se midio: no se vacio nada.
    #[error("the recycle bin changed since it was measured")]
    RecycleBinChanged,
    /// No hay nada que vaciar.
    #[error("the recycle bin is already empty")]
    RecycleBinAlreadyEmpty,
    /// La Papelera es por-usuario y la sesion no es la del token (`different_user` / `undetermined`).
    #[error("the recycle bin is blocked: {}", .0.as_str())]
    RecycleBinBlocked(MismatchReason),
    /// No se pudo resolver el volumen del sistema o medir la Papelera.
    #[error("the recycle bin is unavailable")]
    RecycleBinUnavailable,
    /// Ya hay un vaciado en curso.
    #[error("a recycle bin empty is already in progress")]
    RecycleBinEmptyInProgress,
    /// Las carpetas de contenido serian las del perfil de otra cuenta
    /// (`different_user` / `undetermined`): no se busca ni se previsualiza nada (C-11).
    #[error("duplicates are blocked: {}", .0.as_str())]
    DuplicatesBlocked(MismatchReason),
    /// El grupo de duplicados no existe en el escaneo.
    #[error("duplicate group not found")]
    DuplicateGroupNotFound,
    /// El grupo de duplicados ya se resolvio.
    #[error("duplicate group already resolved")]
    DuplicateGroupResolved,
    /// La decision no conserva ninguna copia (eliminaria todas).
    #[error("at least one copy of each group must be kept")]
    DuplicateKeepRequired,
    /// La carpeta elegida no sirve como raiz de analisis; lleva el codigo del motivo (C-07).
    #[error("folder root rejected: {0}")]
    FolderRootRejected(&'static str),
    /// El identificador de seleccion es desconocido o fue reemplazado por otra seleccion.
    #[error("folder pick not found")]
    FolderPickNotFound,
    /// Ya hay un selector de carpeta abierto.
    #[error("a folder picker is already open")]
    FolderPickerBusy,
    /// No se pudo abrir el selector de carpeta del sistema.
    #[error("the folder picker is unavailable")]
    FolderPickerUnavailable,
    /// La carpeta (o el archivo) no pertenece al ultimo analisis.
    #[error("folder not found")]
    FolderNotFound,
    /// No se puede mostrar el elemento en el Explorador; lleva el motivo.
    #[error("reveal unavailable: {}", .0.as_str())]
    RevealUnavailable(RevealReason),
    /// El muestreo del monitor no arranco o fallo: no hay datos en vivo en esta sesion (C-06).
    #[error("the system monitor is unavailable")]
    MonitorUnavailable,
}

impl From<SettingsServiceError> for AppError {
    fn from(error: SettingsServiceError) -> Self {
        match error {
            SettingsServiceError::Settings(SettingsError::Invalid { key, reason }) => {
                Self::SettingInvalid(format!("{key}: {reason}"))
            }
            SettingsServiceError::Settings(SettingsError::ReadOnly(key)) => {
                Self::SettingReadOnly(key)
            }
            SettingsServiceError::Store(store) => store.into(),
        }
    }
}

impl From<RecoveryError> for AppError {
    fn from(error: RecoveryError) -> Self {
        match error {
            RecoveryError::NotRecoverable(reason) => Self::StoreNotRecoverable(reason.to_string()),
            RecoveryError::Failed(store) => store.into(),
        }
    }
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
            Self::SettingInvalid(_) => "setting_invalid",
            Self::SettingReadOnly(_) => "setting_read_only",
            Self::StoreNotRecoverable(_) => "store_not_recoverable",
            Self::ScanInProgress => "scan_in_progress",
            Self::ScanNotFound(_) => "scan_not_found",
            Self::ScanKindUnsupported(_) => "scan_kind_unsupported",
            Self::InvalidArgument(_) => "invalid_argument",
            Self::PreviewNotFound => "preview_not_found",
            Self::PreviewExpired => "preview_expired",
            Self::PreviewStale => "preview_stale",
            Self::ScanAlreadyCleaned => "scan_already_cleaned",
            Self::CleanupInProgress => "cleanup_in_progress",
            Self::NothingToClean => "nothing_to_clean",
            Self::EmptyRequestNotFound => "empty_request_not_found",
            Self::EmptyRequestExpired => "empty_request_expired",
            Self::EmptyNotAcknowledged => "empty_not_acknowledged",
            Self::RecycleBinChanged => "recycle_bin_changed",
            Self::RecycleBinAlreadyEmpty => "recycle_bin_already_empty",
            Self::RecycleBinBlocked(_) => "recycle_bin_blocked",
            Self::RecycleBinUnavailable => "recycle_bin_unavailable",
            Self::RecycleBinEmptyInProgress => "recycle_bin_empty_in_progress",
            Self::DuplicatesBlocked(_) => "duplicates_blocked",
            Self::DuplicateGroupNotFound => "duplicate_group_not_found",
            Self::DuplicateGroupResolved => "duplicate_group_resolved",
            Self::DuplicateKeepRequired => "duplicate_keep_required",
            Self::FolderRootRejected(_) => "folder_root_rejected",
            Self::FolderPickNotFound => "folder_pick_not_found",
            Self::FolderPickerBusy => "folder_picker_busy",
            Self::FolderPickerUnavailable => "folder_picker_unavailable",
            Self::FolderNotFound => "folder_not_found",
            Self::RevealUnavailable(_) => "reveal_unavailable",
            Self::MonitorUnavailable => "monitor_unavailable",
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

    #[test]
    fn settings_errors_should_map_to_typed_kinds() {
        use crate::adapters::sqlite::state::RecoveryError;
        use crate::domain::settings::{SettingsError, SettingsServiceError};
        let invalid = SettingsServiceError::Settings(SettingsError::Invalid {
            key: "ui_mode".into(),
            reason: "unsupported value",
        });
        let read_only = SettingsServiceError::Settings(SettingsError::ReadOnly("language".into()));
        let corrupt = SettingsServiceError::Store(StoreError::Corrupt("x".into()));
        let cases = [
            (AppError::from(invalid), "setting_invalid"),
            (AppError::from(read_only), "setting_read_only"),
            (AppError::from(corrupt), "store_corrupt"),
            (
                AppError::from(RecoveryError::NotRecoverable("already ready")),
                "store_not_recoverable",
            ),
            (
                AppError::from(RecoveryError::Failed(StoreError::Unavailable("x".into()))),
                "store_unavailable",
            ),
        ];
        for (error, kind) in cases {
            let value = serde_json::to_value(&error).unwrap();
            assert_eq!(value["kind"], kind, "{error:?}");
            let upper = value["message"].as_str().unwrap().to_ascii_uppercase();
            assert!(
                !upper.contains("SELECT") && !upper.contains("INSERT"),
                "{upper}"
            );
        }
    }

    #[test]
    fn cleanup_errors_should_serialize_with_stable_kinds_and_a_plain_message() {
        let cases = [
            (AppError::PreviewNotFound, "preview_not_found"),
            (AppError::PreviewExpired, "preview_expired"),
            (AppError::PreviewStale, "preview_stale"),
            (AppError::ScanAlreadyCleaned, "scan_already_cleaned"),
            (AppError::CleanupInProgress, "cleanup_in_progress"),
            (AppError::NothingToClean, "nothing_to_clean"),
        ];
        for (error, kind) in cases {
            let value = serde_json::to_value(&error).unwrap();
            assert_eq!(value["kind"], kind);
            let message = value["message"].as_str().unwrap();
            assert!(!message.is_empty(), "{kind}");
            assert!(!message.contains(char::from(0x5c)), "{kind}: {message}");
        }
    }

    #[test]
    fn recycle_bin_empty_errors_should_serialize_with_stable_kinds_and_a_plain_message() {
        let cases = [
            (AppError::EmptyRequestNotFound, "empty_request_not_found"),
            (AppError::EmptyRequestExpired, "empty_request_expired"),
            (AppError::EmptyNotAcknowledged, "empty_not_acknowledged"),
            (AppError::RecycleBinChanged, "recycle_bin_changed"),
            (
                AppError::RecycleBinAlreadyEmpty,
                "recycle_bin_already_empty",
            ),
            (
                AppError::RecycleBinBlocked(MismatchReason::DifferentUser),
                "recycle_bin_blocked",
            ),
            (AppError::RecycleBinUnavailable, "recycle_bin_unavailable"),
            (
                AppError::RecycleBinEmptyInProgress,
                "recycle_bin_empty_in_progress",
            ),
        ];
        for (error, kind) in cases {
            let value = serde_json::to_value(&error).unwrap();
            assert_eq!(value["kind"], kind);
            let message = value["message"].as_str().unwrap();
            assert!(!message.is_empty(), "{kind}");
            assert!(!message.contains(char::from(0x5c)), "{kind}: {message}");
            assert!(
                !message.contains(':') || kind == "recycle_bin_blocked",
                "{kind}"
            );
        }
    }

    #[test]
    fn recycle_bin_blocked_should_carry_the_mismatch_reason_in_its_message() {
        for (reason, code) in [
            (MismatchReason::DifferentUser, "different_user"),
            (MismatchReason::Undetermined, "undetermined"),
        ] {
            let value = serde_json::to_value(AppError::RecycleBinBlocked(reason)).unwrap();
            assert!(value["message"].as_str().unwrap().ends_with(code));
        }
    }

    #[test]
    fn duplicate_errors_should_serialize_with_stable_kinds_and_a_message_without_paths() {
        let cases = [
            (
                AppError::DuplicatesBlocked(MismatchReason::DifferentUser),
                "duplicates_blocked",
            ),
            (
                AppError::DuplicatesBlocked(MismatchReason::Undetermined),
                "duplicates_blocked",
            ),
            (
                AppError::DuplicateGroupNotFound,
                "duplicate_group_not_found",
            ),
            (AppError::DuplicateGroupResolved, "duplicate_group_resolved"),
            (AppError::DuplicateKeepRequired, "duplicate_keep_required"),
        ];
        for (error, kind) in cases {
            let value = serde_json::to_value(&error).unwrap();
            assert_eq!(value["kind"], kind);
            let message = value["message"].as_str().unwrap();
            assert!(!message.is_empty(), "{kind}");
            assert!(!message.contains(char::from(0x5c)), "{kind}: {message}");
            assert!(!message.contains('/'), "{kind}: {message}");
        }
    }

    #[test]
    fn duplicates_blocked_should_carry_the_mismatch_reason_in_its_message() {
        for (reason, code) in [
            (MismatchReason::DifferentUser, "different_user"),
            (MismatchReason::Undetermined, "undetermined"),
        ] {
            let value = serde_json::to_value(AppError::DuplicatesBlocked(reason)).unwrap();
            assert!(value["message"].as_str().unwrap().ends_with(code));
        }
    }

    #[test]
    fn disk_usage_errors_should_serialize_with_stable_kinds_and_a_message_without_paths() {
        use crate::domain::disk_usage::model::RevealReason;
        let cases = [
            (
                AppError::FolderRootRejected("reparse_point"),
                "folder_root_rejected",
            ),
            (
                AppError::FolderRootRejected("unsupported_volume"),
                "folder_root_rejected",
            ),
            (AppError::FolderPickNotFound, "folder_pick_not_found"),
            (AppError::FolderPickerBusy, "folder_picker_busy"),
            (
                AppError::FolderPickerUnavailable,
                "folder_picker_unavailable",
            ),
            (AppError::FolderNotFound, "folder_not_found"),
            (
                AppError::RevealUnavailable(RevealReason::TargetMissing),
                "reveal_unavailable",
            ),
            (
                AppError::RevealUnavailable(RevealReason::Opaque),
                "reveal_unavailable",
            ),
        ];
        for (error, kind) in cases {
            let value = serde_json::to_value(&error).unwrap();
            assert_eq!(value["kind"], kind);
            let message = value["message"].as_str().unwrap();
            assert!(!message.is_empty(), "{kind}");
            assert!(!message.contains(char::from(0x5c)), "{kind}: {message}");
            assert!(!message.contains('/'), "{kind}: {message}");
        }
    }

    #[test]
    fn monitor_unavailable_should_serialize_with_a_stable_kind_and_no_internal_details() {
        let value = serde_json::to_value(AppError::MonitorUnavailable).unwrap();
        assert_eq!(value["kind"], "monitor_unavailable");
        let message = value["message"].as_str().unwrap();
        assert_eq!(message, "the system monitor is unavailable");
    }

    #[test]
    fn monitor_unavailable_should_serialize_next_to_invalid_argument_with_distinct_kinds() {
        let kinds: Vec<String> = [
            AppError::MonitorUnavailable,
            AppError::InvalidArgument("subscriptionId".into()),
        ]
        .iter()
        .map(|e| {
            serde_json::to_value(e).unwrap()["kind"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
        assert_eq!(kinds, vec!["monitor_unavailable", "invalid_argument"]);
    }
    #[test]
    fn folder_root_rejected_and_reveal_unavailable_should_carry_their_reason_in_the_message() {
        use crate::domain::disk_usage::model::RevealReason;
        for code in ["reparse_point", "other_user_profile", "unsupported_volume"] {
            let value = serde_json::to_value(AppError::FolderRootRejected(code)).unwrap();
            assert!(value["message"].as_str().unwrap().ends_with(code));
        }
        for (reason, code) in [
            (RevealReason::TargetMissing, "target_missing"),
            (RevealReason::Opaque, "opaque"),
            (RevealReason::DifferentUser, "different_user"),
            (RevealReason::Undetermined, "undetermined"),
            (RevealReason::ShellError, "shell_error"),
        ] {
            assert_eq!(reason.as_str(), code);
            let value = serde_json::to_value(AppError::RevealUnavailable(reason)).unwrap();
            assert!(value["message"].as_str().unwrap().ends_with(code));
        }
    }

    #[test]
    fn cleanup_errors_should_have_distinct_kinds_from_the_scan_errors() {
        let kinds: Vec<String> = [
            AppError::ScanInProgress,
            AppError::PreviewNotFound,
            AppError::PreviewExpired,
            AppError::PreviewStale,
            AppError::ScanAlreadyCleaned,
            AppError::CleanupInProgress,
            AppError::NothingToClean,
        ]
        .iter()
        .map(|e| {
            serde_json::to_value(e).unwrap()["kind"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
        let mut unique = kinds.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), kinds.len());
    }

    #[test]
    fn scan_errors_should_serialize_with_stable_kinds_and_context() {
        let cases = [
            (AppError::ScanInProgress, "scan_in_progress", None),
            (
                AppError::ScanNotFound("7".into()),
                "scan_not_found",
                Some("7"),
            ),
            (
                AppError::ScanKindUnsupported("folders".into()),
                "scan_kind_unsupported",
                Some("folders"),
            ),
            (
                AppError::InvalidArgument("limit".into()),
                "invalid_argument",
                Some("limit"),
            ),
        ];
        for (error, kind, detail) in cases {
            let value = serde_json::to_value(&error).unwrap();
            assert_eq!(value["kind"], kind);
            if let Some(detail) = detail {
                assert!(value["message"].as_str().unwrap().contains(detail));
            }
        }
    }
}

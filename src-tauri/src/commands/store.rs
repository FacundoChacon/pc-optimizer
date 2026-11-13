//! Comandos `get_store_status` y `recover_store`.

use serde::Serialize;
use tauri::State;

use super::run_blocking;
use crate::adapters::clock::SystemClock;
use crate::adapters::sqlite::StoreHandle;
use crate::domain::ports::Clock;
use crate::domain::store_status::{StoreStatus, UnavailableReason};
use crate::error::AppError;

/// Estado de la base para el frontend: sin rutas ni detalles internos.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum StoreStatusDto {
    Ready,
    Unavailable {
        reason: &'static str,
        recoverable: bool,
    },
}

impl From<StoreStatus> for StoreStatusDto {
    fn from(status: StoreStatus) -> Self {
        match status {
            StoreStatus::Ready => Self::Ready,
            StoreStatus::Unavailable {
                reason,
                recoverable,
            } => Self::Unavailable {
                reason: UnavailableReason::as_str(reason),
                recoverable,
            },
        }
    }
}

/// Resultado de una recuperacion: solo el nombre del respaldo, sin directorio.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryDto {
    pub backup_file_name: String,
}

/// Estado actual de la base.
pub fn store_status(handle: &StoreHandle) -> StoreStatusDto {
    handle.status().into()
}

/// Respalda y recrea la base; devuelve el nombre del respaldo.
pub fn recover(handle: &StoreHandle, clock: &impl Clock) -> Result<RecoveryDto, AppError> {
    let backup = handle.recover(clock)?;
    let name = backup
        .file_name()
        .ok_or_else(|| AppError::Internal("backup path has no file name".into()))?;
    Ok(RecoveryDto {
        backup_file_name: name.to_string_lossy().into_owned(),
    })
}

/// Comando IPC: estado de la base.
#[tauri::command]
pub async fn get_store_status(handle: State<'_, StoreHandle>) -> Result<StoreStatusDto, AppError> {
    let handle = handle.inner().clone();
    run_blocking(move || Ok(store_status(&handle))).await
}

/// Comando IPC: recuperacion confirmada por el usuario.
#[tauri::command]
pub async fn recover_store(handle: State<'_, StoreHandle>) -> Result<RecoveryDto, AppError> {
    let handle = handle.inner().clone();
    run_blocking(move || recover(&handle, &SystemClock)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::sqlite::open_store_state;
    use crate::domain::model::EpochMillis;
    use crate::domain::ports::fakes::FixedClock;
    use serde_json::json;
    use tempfile::TempDir;

    fn ready_handle() -> (TempDir, StoreHandle) {
        let dir = TempDir::new().unwrap();
        let handle = StoreHandle::new(open_store_state(Ok(dir.path().to_path_buf())));
        (dir, handle)
    }

    fn corrupt_handle() -> (TempDir, StoreHandle) {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("data.db"), b"basura".repeat(200)).unwrap();
        let handle = StoreHandle::new(open_store_state(Ok(dir.path().to_path_buf())));
        (dir, handle)
    }

    #[test]
    fn status_dto_should_serialize_ready_and_unavailable_shapes() {
        assert_eq!(
            serde_json::to_value(StoreStatusDto::from(StoreStatus::Ready)).unwrap(),
            json!({"status":"ready"})
        );
        let corrupt = StoreStatus::Unavailable {
            reason: UnavailableReason::Corrupt,
            recoverable: true,
        };
        assert_eq!(
            serde_json::to_value(StoreStatusDto::from(corrupt)).unwrap(),
            json!({"status":"unavailable","reason":"corrupt","recoverable":true})
        );
        let too_new = StoreStatus::Unavailable {
            reason: UnavailableReason::SchemaTooNew,
            recoverable: false,
        };
        assert_eq!(
            serde_json::to_value(StoreStatusDto::from(too_new)).unwrap(),
            json!({"status":"unavailable","reason":"schema_too_new","recoverable":false})
        );
    }

    #[test]
    fn recovery_dto_should_serialize_only_the_file_name() {
        let dto = RecoveryDto {
            backup_file_name: "data.db.corrupt-1700000000000.bak".into(),
        };
        assert_eq!(
            serde_json::to_value(dto).unwrap(),
            json!({"backupFileName":"data.db.corrupt-1700000000000.bak"})
        );
    }

    #[test]
    fn store_status_should_follow_the_handle() {
        let (_d1, ready) = ready_handle();
        assert_eq!(
            serde_json::to_value(store_status(&ready)).unwrap(),
            json!({"status":"ready"})
        );
        let (_d2, corrupt) = corrupt_handle();
        let value = serde_json::to_value(store_status(&corrupt)).unwrap();
        assert_eq!(value["reason"], "corrupt");
        assert_eq!(value["recoverable"], true);
        assert!(!value.to_string().contains("data.db"));
    }

    #[test]
    fn recover_should_return_the_backup_file_name_without_directory() {
        let (dir, handle) = corrupt_handle();
        let dto = recover(&handle, &FixedClock(EpochMillis(1_700_000_000_000))).unwrap();
        assert_eq!(dto.backup_file_name, "data.db.corrupt-1700000000000.bak");
        assert!(dir.path().join(&dto.backup_file_name).is_file());
        assert_eq!(
            serde_json::to_value(store_status(&handle)).unwrap(),
            json!({"status":"ready"})
        );
    }

    #[test]
    fn recover_should_fail_with_store_not_recoverable_when_ready() {
        let (_dir, handle) = ready_handle();
        let err = recover(&handle, &FixedClock(EpochMillis(1))).unwrap_err();
        assert_eq!(
            serde_json::to_value(err).unwrap()["kind"],
            "store_not_recoverable"
        );
    }
}

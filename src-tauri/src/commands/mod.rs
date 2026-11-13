//! Capa de comandos IPC: funciones finas que delegan en el dominio.

pub mod app_info;
pub mod names;
pub mod privileges;
pub mod settings;
pub mod store;

use crate::error::AppError;

/// Ejecuta trabajo bloqueante (SQLite, renombrados) fuera del hilo principal.
/// Un fallo de la tarea (panic/cancelacion) se informa como `AppError::Internal`.
pub async fn run_blocking<T, F>(work: F) -> Result<T, AppError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, AppError> + Send + 'static,
{
    match tauri::async_runtime::spawn_blocking(work).await {
        Ok(result) => result,
        Err(error) => Err(AppError::Internal(format!("blocking task failed: {error}"))),
    }
}

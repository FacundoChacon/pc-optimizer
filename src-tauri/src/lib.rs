//! Biblioteca de PC Optimizer.

pub mod adapters;
pub mod commands;
pub mod config;
pub mod domain;
pub mod error;

use std::path::Path;

use adapters::sqlite::open_store_state;
use config::{RuntimeConfig, resolve_data_dir, resolve_runtime_config};
use error::AppError;
use tauri::Manager;

/// Configura un `Builder` con el handler IPC y el estado compartido.
///
/// Es generico sobre el runtime para poder usarlo con el runtime mock en tests.
pub fn app_builder<R: tauri::Runtime>(
    builder: tauri::Builder<R>,
    config: RuntimeConfig,
) -> tauri::Builder<R> {
    builder
        .manage(config)
        .invoke_handler(tauri::generate_handler![commands::app_info::get_app_info])
}

/// Abre la base local y la registra como estado gestionado (`StoreState`).
///
/// El directorio sale del override de desarrollo o, si no hay, de `%LOCALAPPDATA%` del perfil
/// (costura para C-05: unico punto que depende del perfil). Nunca falla: una base no
/// disponible queda como `StoreState::Unavailable` y la app arranca igual.
pub fn manage_store<R: tauri::Runtime>(app: &impl Manager<R>, override_dir: Option<&Path>) {
    let local_app_data = app.path().local_data_dir().map_err(|e| e.to_string());
    app.manage(open_store_state(resolve_data_dir(
        override_dir,
        local_app_data,
    )));
}

/// Arranca la aplicacion Tauri.
pub fn run() -> Result<(), AppError> {
    let config = resolve_runtime_config(|key| std::env::var(key).ok(), cfg!(debug_assertions));
    let data_dir_override = config.data_dir_override.clone();
    app_builder(tauri::Builder::default(), config)
        .setup(move |app| {
            manage_store(app, data_dir_override.as_deref());
            Ok(())
        })
        .run(tauri::generate_context!())
        .map_err(|error| AppError::Internal(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri::test::{
        INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder, mock_context, noop_assets,
    };
    use tauri::webview::InvokeRequest;
    use tauri::{WebviewWindowBuilder, ipc::InvokeBody};

    fn mock_app() -> tauri::App<MockRuntime> {
        app_builder(mock_builder(), resolve_runtime_config(|_| None, false))
            .build(mock_context(noop_assets()))
            .unwrap()
    }

    fn request(cmd: &str) -> InvokeRequest {
        InvokeRequest {
            cmd: cmd.into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            url: "http://tauri.localhost".parse().unwrap(),
            body: InvokeBody::default(),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        }
    }

    #[test]
    fn get_app_info_should_answer_over_ipc() {
        let app = mock_app();
        let window = WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        let response = get_ipc_response(&window, request("get_app_info"))
            .map(|body| body.deserialize::<serde_json::Value>().unwrap())
            .unwrap();
        assert_eq!(response["name"], "pc-optimizer");
        assert_eq!(response["version"], env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn manage_store_should_register_a_ready_store_in_the_override_directory() {
        let dir = tempfile::TempDir::new().unwrap();
        let data_dir = dir.path().join("datos");
        let app = mock_app();
        manage_store(&app, Some(&data_dir));
        let state = app.state::<adapters::sqlite::StoreState>();
        assert!(matches!(*state, adapters::sqlite::StoreState::Ready(_)));
        assert!(data_dir.join("data.db").is_file());
    }

    #[test]
    fn manage_store_should_register_unavailable_state_when_the_database_is_corrupt() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("data.db"), b"basura".repeat(200)).unwrap();
        let app = mock_app();
        manage_store(&app, Some(dir.path()));
        let state = app.state::<adapters::sqlite::StoreState>();
        assert!(matches!(
            &*state,
            adapters::sqlite::StoreState::Unavailable {
                error: domain::ports::StoreError::Corrupt(_),
                ..
            }
        ));
    }

    #[test]
    fn unknown_command_should_be_rejected_over_ipc() {
        let app = mock_app();
        let window = WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        assert!(get_ipc_response(&window, request("does_not_exist")).is_err());
    }
}

//! Biblioteca de PC Optimizer.

pub mod adapters;
pub mod commands;
pub mod config;
pub mod domain;
pub mod error;

use config::{RuntimeConfig, resolve_runtime_config};
use error::AppError;

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

/// Arranca la aplicacion Tauri.
pub fn run() -> Result<(), AppError> {
    let config = resolve_runtime_config(|key| std::env::var(key).ok(), cfg!(debug_assertions));
    app_builder(tauri::Builder::default(), config)
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
    fn unknown_command_should_be_rejected_over_ipc() {
        let app = mock_app();
        let window = WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        assert!(get_ipc_response(&window, request("does_not_exist")).is_err());
    }
}

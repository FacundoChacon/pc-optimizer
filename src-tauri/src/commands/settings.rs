//! Comandos `get_settings` y `set_setting`.

use serde::Serialize;
use serde_json::Value;
use tauri::State;

use super::run_blocking;
use crate::adapters::sqlite::StoreHandle;
use crate::domain::settings::{self, Settings};
use crate::error::AppError;

/// Configuracion tal como la ve el frontend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsDto {
    pub ui_mode: &'static str,
    pub autostart_tray: bool,
    pub language: &'static str,
}

impl From<Settings> for SettingsDto {
    fn from(settings: Settings) -> Self {
        Self {
            ui_mode: settings.ui_mode.as_str(),
            autostart_tray: settings.autostart_tray,
            language: settings.language.as_str(),
        }
    }
}

/// Lee la configuracion efectiva.
pub fn read_settings(handle: &StoreHandle) -> Result<SettingsDto, AppError> {
    let store = handle.store()?;
    Ok(settings::read_settings(&*store)?.into())
}

/// Valida y escribe un ajuste; devuelve la configuracion releida.
pub fn write_setting(
    handle: &StoreHandle,
    key: &str,
    value: &Value,
) -> Result<SettingsDto, AppError> {
    let store = handle.store()?;
    Ok(settings::update_setting(&*store, key, value)?.into())
}

/// Comando IPC: configuracion actual.
#[tauri::command]
pub async fn get_settings(handle: State<'_, StoreHandle>) -> Result<SettingsDto, AppError> {
    let handle = handle.inner().clone();
    run_blocking(move || read_settings(&handle)).await
}

/// Comando IPC: escribe un ajuste de la allowlist.
#[tauri::command]
pub async fn set_setting(
    handle: State<'_, StoreHandle>,
    key: String,
    value: Value,
) -> Result<SettingsDto, AppError> {
    let handle = handle.inner().clone();
    run_blocking(move || write_setting(&handle, &key, &value)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::sqlite::open_store_state;
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

    fn kind_of(error: AppError) -> String {
        serde_json::to_value(error).unwrap()["kind"]
            .as_str()
            .unwrap()
            .to_string()
    }

    #[test]
    fn dto_should_serialize_with_camel_case_keys() {
        let simple = SettingsDto::from(Settings::default());
        assert_eq!(
            serde_json::to_value(simple).unwrap(),
            json!({"uiMode":"simple","autostartTray":false,"language":"es"})
        );
        let advanced = SettingsDto::from(Settings {
            ui_mode: crate::domain::settings::UiMode::Advanced,
            ..Settings::default()
        });
        assert_eq!(
            serde_json::to_value(advanced).unwrap()["uiMode"],
            "advanced"
        );
    }

    #[test]
    fn read_settings_should_return_the_seed_on_a_new_database() {
        let (_dir, handle) = ready_handle();
        let dto = read_settings(&handle).unwrap();
        assert_eq!(dto.ui_mode, "simple");
        assert!(!dto.autostart_tray);
    }

    #[test]
    fn write_setting_should_persist_and_return_the_reread_dto() {
        let (_dir, handle) = ready_handle();
        let dto = write_setting(&handle, "ui_mode", &json!("advanced")).unwrap();
        assert_eq!(dto.ui_mode, "advanced");
        assert_eq!(read_settings(&handle).unwrap().ui_mode, "advanced");
        assert_eq!(
            write_setting(&handle, "ui_mode", &json!("simple"))
                .unwrap()
                .ui_mode,
            "simple"
        );
    }

    #[test]
    fn write_setting_should_reject_invalid_and_read_only_input() {
        let (_dir, handle) = ready_handle();
        let invalid = write_setting(&handle, "ui_mode", &json!("turbo")).unwrap_err();
        assert_eq!(kind_of(invalid), "setting_invalid");
        let read_only = write_setting(&handle, "autostart_tray", &json!(true)).unwrap_err();
        assert_eq!(kind_of(read_only), "setting_read_only");
        assert_eq!(read_settings(&handle).unwrap().ui_mode, "simple");
    }

    #[test]
    fn commands_should_report_store_corrupt_on_a_corrupt_database() {
        let (_dir, handle) = corrupt_handle();
        assert_eq!(
            kind_of(read_settings(&handle).unwrap_err()),
            "store_corrupt"
        );
        let err = write_setting(&handle, "ui_mode", &json!("advanced")).unwrap_err();
        assert_eq!(kind_of(err), "store_corrupt");
    }

    #[test]
    fn run_blocking_should_return_the_closure_result_and_map_panics_to_internal() {
        let ok = tauri::async_runtime::block_on(crate::commands::run_blocking(|| Ok(7)));
        assert_eq!(ok.unwrap(), 7);
        let panicked: Result<u8, AppError> =
            tauri::async_runtime::block_on(crate::commands::run_blocking(|| panic!("boom")));
        assert_eq!(kind_of(panicked.unwrap_err()), "internal");
    }
}

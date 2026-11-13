//! Comando `get_app_info`: nombre y version de la aplicacion.

use crate::error::AppError;
use serde::Serialize;

/// Informacion basica de la aplicacion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
}

/// Construye la informacion de la app desde los metadatos del paquete.
pub fn app_info() -> AppInfo {
    AppInfo {
        name: env!("CARGO_PKG_NAME").to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

/// Comando IPC fino: delega en [`app_info`].
#[tauri::command]
pub fn get_app_info() -> Result<AppInfo, AppError> {
    Ok(app_info())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn app_info_should_report_package_name_and_version() {
        let info = app_info();
        assert_eq!(info.name, "pc-optimizer");
        assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn app_info_should_serialize_with_name_and_version_keys() {
        let value = serde_json::to_value(app_info()).unwrap();
        assert_eq!(
            value,
            json!({ "name": "pc-optimizer", "version": env!("CARGO_PKG_VERSION") })
        );
    }

    #[test]
    fn get_app_info_should_return_ok_with_same_data() {
        let info = get_app_info().unwrap();
        assert_eq!(info, app_info());
    }
}

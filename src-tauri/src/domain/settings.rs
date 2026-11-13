//! Ajustes de la aplicacion: modelo, lectura tolerante y escritura validada (allowlist).

use serde_json::Value;

use crate::domain::ports::{SettingsStore, StoreError};

pub const KEY_UI_MODE: &str = "ui_mode";
pub const KEY_AUTOSTART_TRAY: &str = "autostart_tray";
pub const KEY_LANGUAGE: &str = "language";

/// Modo de interfaz.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UiMode {
    #[default]
    Simple,
    Advanced,
}

impl UiMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Simple => "simple",
            Self::Advanced => "advanced",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "simple" => Some(Self::Simple),
            "advanced" => Some(Self::Advanced),
            _ => None,
        }
    }
}

/// Idioma de la interfaz.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Language {
    #[default]
    Es,
}

impl Language {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Es => "es",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "es" => Some(Self::Es),
            _ => None,
        }
    }
}

/// Configuracion efectiva de la app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Settings {
    pub ui_mode: UiMode,
    pub autostart_tray: bool,
    pub language: Language,
}

/// Actualizacion validada y escribible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingUpdate {
    UiMode(UiMode),
}

impl SettingUpdate {
    pub fn key(&self) -> &'static str {
        match self {
            Self::UiMode(_) => KEY_UI_MODE,
        }
    }

    pub fn to_json(&self) -> Value {
        match self {
            Self::UiMode(mode) => Value::String(mode.as_str().to_string()),
        }
    }
}

/// Rechazo de una escritura.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SettingsError {
    #[error("invalid setting {key}: {reason}")]
    Invalid { key: String, reason: &'static str },
    #[error("setting {0} is read-only")]
    ReadOnly(String),
}

/// Error del servicio de ajustes: validacion o persistencia.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SettingsServiceError {
    #[error(transparent)]
    Settings(#[from] SettingsError),
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// Lectura tolerante: ausente, de otro tipo o fuera de rango -> default. Nunca escribe.
pub fn settings_from_entries(entries: &[(String, Value)]) -> Settings {
    let find = |key: &str| entries.iter().find(|(k, _)| k == key).map(|(_, v)| v);
    let defaults = Settings::default();
    Settings {
        ui_mode: find(KEY_UI_MODE)
            .and_then(Value::as_str)
            .and_then(UiMode::parse)
            .unwrap_or(defaults.ui_mode),
        autostart_tray: find(KEY_AUTOSTART_TRAY)
            .and_then(Value::as_bool)
            .unwrap_or(defaults.autostart_tray),
        language: find(KEY_LANGUAGE)
            .and_then(Value::as_str)
            .and_then(Language::parse)
            .unwrap_or(defaults.language),
    }
}

/// Allowlist de escritura: solo `ui_mode` es escribible en esta version.
pub fn parse_update(key: &str, value: &Value) -> Result<SettingUpdate, SettingsError> {
    match key {
        KEY_UI_MODE => value
            .as_str()
            .and_then(UiMode::parse)
            .map(SettingUpdate::UiMode)
            .ok_or_else(|| SettingsError::Invalid {
                key: key.to_string(),
                reason: "unsupported value",
            }),
        KEY_AUTOSTART_TRAY | KEY_LANGUAGE => Err(SettingsError::ReadOnly(key.to_string())),
        _ => Err(SettingsError::Invalid {
            key: key.to_string(),
            reason: "unknown setting",
        }),
    }
}

/// Lee la configuracion efectiva del store.
pub fn read_settings<S: SettingsStore + ?Sized>(store: &S) -> Result<Settings, StoreError> {
    Ok(settings_from_entries(&store.list_settings()?))
}

/// Valida y guarda un ajuste; devuelve la configuracion releida (no la supuesta).
pub fn update_setting<S: SettingsStore + ?Sized>(
    store: &S,
    key: &str,
    value: &Value,
) -> Result<Settings, SettingsServiceError> {
    let update = parse_update(key, value)?;
    store.set_setting(update.key(), &update.to_json())?;
    Ok(read_settings(store)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::model::DEFAULT_SETTINGS;
    use crate::domain::ports::fakes::InMemoryStore;
    use serde_json::{Value, json};

    fn entries(pairs: &[(&str, Value)]) -> Vec<(String, Value)> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), v.clone()))
            .collect()
    }

    #[test]
    fn ui_mode_should_round_trip_and_reject_unknown_values() {
        for mode in [UiMode::Simple, UiMode::Advanced] {
            assert_eq!(UiMode::parse(mode.as_str()), Some(mode));
        }
        assert_eq!(UiMode::Advanced.as_str(), "advanced");
        for bad in ["turbo", "", "SIMPLE"] {
            assert_eq!(UiMode::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn language_should_round_trip_and_reject_unknown_values() {
        assert_eq!(Language::parse("es"), Some(Language::Es));
        assert_eq!(Language::Es.as_str(), "es");
        assert_eq!(Language::parse("en"), None);
    }

    #[test]
    fn default_settings_should_be_simple_without_autostart_in_spanish() {
        let defaults = Settings::default();
        assert_eq!(defaults.ui_mode, UiMode::Simple);
        assert!(!defaults.autostart_tray);
        assert_eq!(defaults.language, Language::Es);
    }

    #[test]
    fn from_entries_should_read_the_seed_as_defaults() {
        let seed: Vec<(String, Value)> = DEFAULT_SETTINGS
            .iter()
            .map(|(k, raw)| ((*k).to_string(), serde_json::from_str(raw).unwrap()))
            .collect();
        assert_eq!(settings_from_entries(&seed), Settings::default());
    }

    #[test]
    fn from_entries_should_read_advanced_mode() {
        let settings = settings_from_entries(&entries(&[("ui_mode", json!("advanced"))]));
        assert_eq!(settings.ui_mode, UiMode::Advanced);
    }

    #[test]
    fn from_entries_should_fall_back_to_simple_on_invalid_ui_mode() {
        for bad in [json!("turbo"), json!(1), Value::Null] {
            let settings = settings_from_entries(&entries(&[("ui_mode", bad.clone())]));
            assert_eq!(settings.ui_mode, UiMode::Simple, "{bad}");
        }
    }

    #[test]
    fn from_entries_should_treat_non_boolean_autostart_as_false_and_ignore_unknown_keys() {
        let settings = settings_from_entries(&entries(&[
            ("autostart_tray", json!("si")),
            ("scan_roots", json!(["C:\\"])),
        ]));
        assert!(!settings.autostart_tray);
        let on = settings_from_entries(&entries(&[("autostart_tray", json!(true))]));
        assert!(on.autostart_tray);
        assert_eq!(settings_from_entries(&[]), Settings::default());
    }

    #[test]
    fn parse_update_should_accept_valid_ui_modes() {
        assert_eq!(
            parse_update("ui_mode", &json!("advanced")),
            Ok(SettingUpdate::UiMode(UiMode::Advanced))
        );
        assert_eq!(
            parse_update("ui_mode", &json!("simple")),
            Ok(SettingUpdate::UiMode(UiMode::Simple))
        );
    }

    #[test]
    fn parse_update_should_reject_invalid_ui_mode_values() {
        for bad in [json!("turbo"), json!(1), Value::Null, json!(true)] {
            assert!(
                matches!(
                    parse_update("ui_mode", &bad),
                    Err(SettingsError::Invalid { .. })
                ),
                "{bad}"
            );
        }
    }

    #[test]
    fn parse_update_should_mark_known_keys_read_only_and_reject_unknown_keys() {
        assert_eq!(
            parse_update("autostart_tray", &json!(true)),
            Err(SettingsError::ReadOnly("autostart_tray".into()))
        );
        assert_eq!(
            parse_update("language", &json!("en")),
            Err(SettingsError::ReadOnly("language".into()))
        );
        for key in ["scan_roots", "", "UI_MODE"] {
            assert!(
                matches!(
                    parse_update(key, &json!("simple")),
                    Err(SettingsError::Invalid { .. })
                ),
                "{key}"
            );
        }
    }

    #[test]
    fn update_setting_should_persist_and_return_the_reread_settings() {
        let store = InMemoryStore::new();
        let advanced = update_setting(&store, "ui_mode", &json!("advanced")).unwrap();
        assert_eq!(advanced.ui_mode, UiMode::Advanced);
        let simple = update_setting(&store, "ui_mode", &json!("simple")).unwrap();
        assert_eq!(simple.ui_mode, UiMode::Simple);
    }

    #[test]
    fn update_setting_should_keep_the_previous_value_when_the_input_is_invalid() {
        let store = InMemoryStore::new();
        update_setting(&store, "ui_mode", &json!("advanced")).unwrap();
        let err = update_setting(&store, "ui_mode", &json!("turbo")).unwrap_err();
        assert!(matches!(
            err,
            SettingsServiceError::Settings(SettingsError::Invalid { .. })
        ));
        assert_eq!(
            store.get_setting("ui_mode").unwrap(),
            Some(json!("advanced"))
        );
    }

    #[test]
    fn update_setting_should_not_write_read_only_or_unknown_keys() {
        let store = InMemoryStore::new();
        let before = store.list_settings().unwrap();
        let err = update_setting(&store, "autostart_tray", &json!(true)).unwrap_err();
        assert!(matches!(
            err,
            SettingsServiceError::Settings(SettingsError::ReadOnly(_))
        ));
        assert_eq!(
            store.get_setting("autostart_tray").unwrap(),
            Some(json!(false))
        );
        assert!(update_setting(&store, "inventada", &json!(1)).is_err());
        assert_eq!(store.list_settings().unwrap(), before);
    }
}

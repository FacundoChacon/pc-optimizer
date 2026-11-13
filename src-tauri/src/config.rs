//! Configuracion de runtime resuelta desde variables de entorno.
//!
//! La resolucion es pura: el entorno se inyecta como funcion de busqueda para poder
//! testearla sin tocar el entorno real del proceso.

use std::path::PathBuf;

/// Variable que redirige el directorio de datos (solo desarrollo y tests).
const ENV_DATA_DIR: &str = "PCOPT_DATA_DIR";
/// Variable que fija la verbosidad del log (respetada tambien en release).
const ENV_LOG: &str = "PCOPT_LOG";

/// Nivel de log resuelto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "error" => Some(Self::Error),
            "warn" => Some(Self::Warn),
            "info" => Some(Self::Info),
            "debug" => Some(Self::Debug),
            "trace" => Some(Self::Trace),
            _ => None,
        }
    }
}

/// Configuracion de ejecucion de la aplicacion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConfig {
    /// Directorio de datos alternativo; `None` usa el directorio por defecto.
    pub data_dir_override: Option<PathBuf>,
    /// Nivel de log.
    pub log_level: LogLevel,
}

/// Resuelve la configuracion a partir de `lookup` (normalmente `std::env::var`).
///
/// `allow_dev_overrides` solo debe ser `true` en builds de desarrollo/tests: habilita
/// `PCOPT_DATA_DIR`, que se ignora si esta vacio o no es una ruta absoluta.
pub fn resolve_runtime_config(
    lookup: impl Fn(&str) -> Option<String>,
    allow_dev_overrides: bool,
) -> RuntimeConfig {
    let data_dir_override = if allow_dev_overrides {
        lookup(ENV_DATA_DIR)
            .map(|raw| raw.trim().to_string())
            .filter(|raw| !raw.is_empty())
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
    } else {
        None
    };
    let log_level = lookup(ENV_LOG)
        .and_then(|raw| LogLevel::parse(&raw))
        .unwrap_or(LogLevel::Info);
    RuntimeConfig {
        data_dir_override,
        log_level,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn lookup_from<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        let map: HashMap<&str, &str> = pairs.iter().copied().collect();
        move |key| map.get(key).map(|v| (*v).to_string())
    }

    #[test]
    fn data_dir_should_be_honored_when_absolute_and_dev_overrides_allowed() {
        let cfg = resolve_runtime_config(
            lookup_from(&[("PCOPT_DATA_DIR", r"C:\tmp\pcopt-dev")]),
            true,
        );
        assert_eq!(
            cfg.data_dir_override,
            Some(PathBuf::from(r"C:\tmp\pcopt-dev"))
        );
    }

    #[test]
    fn data_dir_should_be_trimmed_before_use() {
        let cfg = resolve_runtime_config(lookup_from(&[("PCOPT_DATA_DIR", r"  D:\datos  ")]), true);
        assert_eq!(cfg.data_dir_override, Some(PathBuf::from(r"D:\datos")));
    }

    #[test]
    fn data_dir_should_be_ignored_when_dev_overrides_disallowed() {
        let cfg = resolve_runtime_config(
            lookup_from(&[("PCOPT_DATA_DIR", r"C:\tmp\pcopt-dev")]),
            false,
        );
        assert_eq!(cfg.data_dir_override, None);
    }

    #[test]
    fn data_dir_should_be_ignored_when_empty_or_blank() {
        for raw in ["", "   ", "\t"] {
            let cfg = resolve_runtime_config(lookup_from(&[("PCOPT_DATA_DIR", raw)]), true);
            assert_eq!(cfg.data_dir_override, None, "raw = {raw:?}");
        }
    }

    #[test]
    fn data_dir_should_be_ignored_when_missing() {
        let cfg = resolve_runtime_config(lookup_from(&[]), true);
        assert_eq!(cfg.data_dir_override, None);
    }

    #[test]
    fn data_dir_should_be_ignored_when_relative() {
        for raw in [r"datos\dev", "./datos", "datos"] {
            let cfg = resolve_runtime_config(lookup_from(&[("PCOPT_DATA_DIR", raw)]), true);
            assert_eq!(cfg.data_dir_override, None, "raw = {raw:?}");
        }
    }

    #[test]
    fn log_level_should_parse_case_insensitively() {
        let cases = [
            ("DEBUG", LogLevel::Debug),
            ("debug", LogLevel::Debug),
            ("Error", LogLevel::Error),
            ("warn", LogLevel::Warn),
            ("INFO", LogLevel::Info),
            ("trace", LogLevel::Trace),
            ("  trace ", LogLevel::Trace),
        ];
        for (raw, expected) in cases {
            let cfg = resolve_runtime_config(lookup_from(&[("PCOPT_LOG", raw)]), false);
            assert_eq!(cfg.log_level, expected, "raw = {raw:?}");
        }
    }

    #[test]
    fn log_level_should_default_to_info_when_invalid_or_missing() {
        for pairs in [
            &[("PCOPT_LOG", "verbose")][..],
            &[("PCOPT_LOG", "")][..],
            &[][..],
        ] {
            let cfg = resolve_runtime_config(lookup_from(pairs), false);
            assert_eq!(cfg.log_level, LogLevel::Info, "pairs = {pairs:?}");
        }
    }

    #[test]
    fn log_level_should_be_honored_even_when_dev_overrides_disallowed() {
        let cfg = resolve_runtime_config(lookup_from(&[("PCOPT_LOG", "debug")]), false);
        assert_eq!(cfg.log_level, LogLevel::Debug);
    }
}

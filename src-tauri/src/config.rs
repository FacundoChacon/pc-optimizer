//! Configuracion de runtime resuelta desde variables de entorno.
//!
//! La resolucion es pura: el entorno se inyecta como funcion de busqueda para poder
//! testearla sin tocar el entorno real del proceso.

use std::path::{Path, PathBuf};

/// Variable que redirige el directorio de datos (solo desarrollo y tests).
const ENV_DATA_DIR: &str = "PCOPT_DATA_DIR";
/// Variable que fija la verbosidad del log (respetada tambien en release).
const ENV_LOG: &str = "PCOPT_LOG";

/// Subcarpeta de la app dentro de `%LOCALAPPDATA%`.
pub const APP_DATA_SUBDIR: &str = "pc-optimizer";
/// Nombre del archivo de base de datos dentro del directorio de datos.
pub const DB_FILE_NAME: &str = "data.db";

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

/// Resuelve el directorio de datos: el override (ya filtrado por `resolve_runtime_config`) se
/// usa tal cual; sin override, `<LocalAppData>\pc-optimizer`. Si `local_app_data` es `Err`
/// (perfil no resoluble) falla en vez de usar un directorio alternativo.
pub fn resolve_data_dir(
    override_dir: Option<&Path>,
    local_app_data: Result<PathBuf, String>,
) -> Result<PathBuf, String> {
    match override_dir {
        Some(dir) => Ok(dir.to_path_buf()),
        None => local_app_data.map(|base| base.join(APP_DATA_SUBDIR)),
    }
}

/// Ruta del archivo de base de datos dentro de `data_dir`.
pub fn db_path(data_dir: &Path) -> PathBuf {
    data_dir.join(DB_FILE_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;

    #[test]
    fn resolve_data_dir_should_append_app_subdir_to_local_app_data_without_override() {
        let dir = resolve_data_dir(None, Ok(PathBuf::from(r"C:\Users\ana\AppData\Local")));
        assert_eq!(
            dir,
            Ok(PathBuf::from(r"C:\Users\ana\AppData\Local\pc-optimizer"))
        );
        let other = resolve_data_dir(None, Ok(PathBuf::from(r"D:\Perfiles\bob\Local")));
        assert_eq!(
            other,
            Ok(PathBuf::from(r"D:\Perfiles\bob\Local\pc-optimizer"))
        );
    }

    #[test]
    fn resolve_data_dir_should_use_override_as_is_and_ignore_the_profile() {
        let over = PathBuf::from(r"C:\tmp\pcopt-dev");
        let with_profile = resolve_data_dir(
            Some(&over),
            Ok(PathBuf::from(r"C:\Users\ana\AppData\Local")),
        );
        assert_eq!(with_profile, Ok(over.clone()));
        let without_profile = resolve_data_dir(Some(&over), Err("sin perfil".into()));
        assert_eq!(without_profile, Ok(over));
    }

    #[test]
    fn resolve_data_dir_should_fail_when_profile_unresolvable_and_no_override() {
        assert_eq!(
            resolve_data_dir(None, Err("sin perfil".into())),
            Err("sin perfil".to_string())
        );
    }

    #[test]
    fn db_path_should_append_the_database_file_name() {
        assert_eq!(
            db_path(Path::new(r"C:\tmp\pcopt-dev")),
            PathBuf::from(r"C:\tmp\pcopt-dev\data.db")
        );
        assert_eq!(db_path(Path::new(r"D:\x")), PathBuf::from(r"D:\x\data.db"));
    }

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

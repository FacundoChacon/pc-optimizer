//! Estado de la persistencia gestionado por la app (`app.manage`).

use std::path::PathBuf;
use std::sync::Arc;

use super::SqliteStore;
use crate::config::db_path;
use crate::domain::ports::StoreError;

/// Resultado de abrir la base al arrancar. La app arranca igual si no esta disponible.
pub enum StoreState {
    Ready(Arc<SqliteStore>),
    Unavailable {
        error: StoreError,
        db_path: Option<PathBuf>,
    },
}

/// Crea el directorio de datos si hace falta y abre la base. Nunca falla ni recrea nada:
/// una base corrupta queda intacta y se informa como `Unavailable`.
pub fn open_store_state(data_dir: Result<PathBuf, String>) -> StoreState {
    let data_dir = match data_dir {
        Ok(dir) => dir,
        Err(reason) => {
            return StoreState::Unavailable {
                error: StoreError::Unavailable(reason),
                db_path: None,
            };
        }
    };
    let path = db_path(&data_dir);
    let opened = std::fs::create_dir_all(&data_dir)
        .map_err(|e| StoreError::Unavailable(format!("cannot create data dir: {}", e.kind())))
        .and_then(|()| SqliteStore::open(&path));
    match opened {
        Ok(store) => StoreState::Ready(Arc::new(store)),
        Err(error) => StoreState::Unavailable {
            error,
            db_path: Some(path),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ports::SettingsStore;
    use tempfile::TempDir;

    #[test]
    fn open_should_create_missing_directory_and_return_ready() {
        let dir = TempDir::new().unwrap();
        let data_dir = dir.path().join("nuevo").join("pc-optimizer");
        let state = open_store_state(Ok(data_dir.clone()));
        let StoreState::Ready(store) = state else {
            panic!("expected Ready");
        };
        assert!(data_dir.join("data.db").is_file());
        assert!(store.get_setting("ui_mode").unwrap().is_some());
    }

    #[test]
    fn open_should_be_unavailable_when_the_directory_cannot_be_resolved() {
        match open_store_state(Err("sin perfil".into())) {
            StoreState::Unavailable { error, db_path } => {
                assert_eq!(error, StoreError::Unavailable("sin perfil".into()));
                assert_eq!(db_path, None);
            }
            StoreState::Ready(_) => panic!("expected Unavailable"),
        }
    }

    #[test]
    fn open_should_report_corrupt_and_never_recreate_automatically() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("data.db");
        let garbage = b"no soy sqlite".repeat(100);
        std::fs::write(&path, &garbage).unwrap();
        match open_store_state(Ok(dir.path().to_path_buf())) {
            StoreState::Unavailable { error, db_path } => {
                assert!(matches!(error, StoreError::Corrupt(_)), "{error:?}");
                assert_eq!(db_path, Some(path.clone()));
            }
            StoreState::Ready(_) => panic!("expected Unavailable"),
        }
        assert_eq!(std::fs::read(&path).unwrap(), garbage);
        let names: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names.len(), 1, "{names:?}");
    }

    #[test]
    fn open_should_be_unavailable_when_the_directory_path_is_a_file() {
        let dir = TempDir::new().unwrap();
        let blocker = dir.path().join("archivo");
        std::fs::write(&blocker, b"x").unwrap();
        match open_store_state(Ok(blocker.join("sub"))) {
            StoreState::Unavailable { error, .. } => {
                assert!(matches!(error, StoreError::Unavailable(_)), "{error:?}");
            }
            StoreState::Ready(_) => panic!("expected Unavailable"),
        }
    }
}

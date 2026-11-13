//! Estado de la persistencia gestionado por la app (`app.manage`).

use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use super::SqliteStore;
use super::recovery::backup_and_recreate;
use crate::config::db_path;
use crate::domain::ports::{Clock, StoreError};
use crate::domain::store_status::{StoreStatus, UnavailableReason, status_of};

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

/// Fallo de una recuperacion de la base.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RecoveryError {
    /// No hay nada que recuperar (base lista, sin ruta, esquema nuevo...).
    #[error("the store cannot be recovered: {0}")]
    NotRecoverable(&'static str),
    /// El respaldo o la recreacion fallaron; el estado previo se conserva.
    #[error(transparent)]
    Failed(StoreError),
}

/// Estado de la base compartido y reemplazable: permite recuperar sin reiniciar la app.
#[derive(Clone)]
pub struct StoreHandle(Arc<RwLock<StoreState>>);

impl StoreHandle {
    /// Envuelve el estado inicial de la base.
    pub fn new(state: StoreState) -> Self {
        Self(Arc::new(RwLock::new(state)))
    }

    /// Estado actual, sin rutas ni detalles internos.
    pub fn status(&self) -> StoreStatus {
        match self.0.read() {
            Ok(state) => match &*state {
                StoreState::Ready(_) => StoreStatus::Ready,
                StoreState::Unavailable { error, db_path } => status_of(error, db_path.is_some()),
            },
            Err(_) => StoreStatus::Unavailable {
                reason: UnavailableReason::Unavailable,
                recoverable: false,
            },
        }
    }

    /// El store listo (clona el `Arc`; el lock se suelta al volver) o el error de apertura.
    pub fn store(&self) -> Result<Arc<SqliteStore>, StoreError> {
        let state = self
            .0
            .read()
            .map_err(|_| StoreError::Unavailable("lock poisoned".into()))?;
        match &*state {
            StoreState::Ready(store) => Ok(Arc::clone(store)),
            StoreState::Unavailable { error, .. } => Err(error.clone()),
        }
    }

    /// Respalda y recrea la base si es recuperable. Mantiene el lock de escritura toda la
    /// operacion: llamadas simultaneas se serializan y la segunda ve `Ready`.
    /// Devuelve la ruta del respaldo. Ante un error conserva el estado previo.
    pub fn recover(&self, clock: &impl Clock) -> Result<PathBuf, RecoveryError> {
        let mut state = self
            .0
            .write()
            .map_err(|_| RecoveryError::Failed(StoreError::Unavailable("lock poisoned".into())))?;
        let path = match &*state {
            StoreState::Ready(_) => return Err(RecoveryError::NotRecoverable("store is ready")),
            StoreState::Unavailable { db_path: None, .. } => {
                return Err(RecoveryError::NotRecoverable("unknown database path"));
            }
            StoreState::Unavailable {
                error,
                db_path: Some(path),
            } => {
                let recoverable = matches!(
                    status_of(error, true),
                    StoreStatus::Unavailable {
                        recoverable: true,
                        ..
                    }
                );
                if !recoverable {
                    return Err(RecoveryError::NotRecoverable("not a recoverable failure"));
                }
                path.clone()
            }
        };
        let (backup, store) = backup_and_recreate(&path, clock).map_err(RecoveryError::Failed)?;
        *state = StoreState::Ready(Arc::new(store));
        Ok(backup)
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

    use crate::domain::model::EpochMillis;
    use crate::domain::ports::fakes::FixedClock;
    use std::os::windows::fs::OpenOptionsExt;

    const GARBAGE: &[u8] = b"no soy sqlite, no soy sqlite, no soy sqlite";

    fn handle_for(dir: &TempDir) -> StoreHandle {
        StoreHandle::new(open_store_state(Ok(dir.path().to_path_buf())))
    }

    fn corrupt_handle() -> (TempDir, StoreHandle) {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("data.db"), GARBAGE.repeat(40)).unwrap();
        let handle = handle_for(&dir);
        (dir, handle)
    }

    fn backups(dir: &TempDir) -> Vec<String> {
        std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("data.db.corrupt-"))
            .collect()
    }

    const CORRUPT: StoreStatus = StoreStatus::Unavailable {
        reason: UnavailableReason::Corrupt,
        recoverable: true,
    };

    #[test]
    fn handle_should_be_ready_for_a_new_database() {
        let dir = TempDir::new().unwrap();
        let handle = handle_for(&dir);
        assert_eq!(handle.status(), StoreStatus::Ready);
        assert!(handle.store().is_ok());
    }

    #[test]
    fn handle_should_report_corrupt_as_recoverable_and_fail_store() {
        let (_dir, handle) = corrupt_handle();
        assert_eq!(handle.status(), CORRUPT);
        assert!(matches!(handle.store(), Err(StoreError::Corrupt(_))));
    }

    #[test]
    fn handle_should_report_unavailable_without_path_as_not_recoverable() {
        let handle = StoreHandle::new(open_store_state(Err("sin perfil".into())));
        assert_eq!(
            handle.status(),
            StoreStatus::Unavailable {
                reason: UnavailableReason::Unavailable,
                recoverable: false
            }
        );
    }

    #[test]
    fn handle_should_be_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<StoreHandle>();
    }

    #[test]
    fn recover_should_back_up_and_leave_a_ready_seeded_store() {
        let (dir, handle) = corrupt_handle();
        let backup = handle
            .recover(&FixedClock(EpochMillis(1_700_000_000_000)))
            .unwrap();
        assert_eq!(
            backup.file_name().unwrap().to_str().unwrap(),
            "data.db.corrupt-1700000000000.bak"
        );
        assert_eq!(std::fs::read(&backup).unwrap(), GARBAGE.repeat(40));
        assert_eq!(handle.status(), StoreStatus::Ready);
        let store = handle.store().unwrap();
        assert!(store.get_setting("ui_mode").unwrap().is_some());
        assert_eq!(backups(&dir).len(), 1);
    }

    #[test]
    fn recover_should_handle_a_migration_failed_state_with_a_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("data.db");
        std::fs::write(&path, GARBAGE).unwrap();
        let handle = StoreHandle::new(StoreState::Unavailable {
            error: StoreError::MigrationFailed {
                version: 2,
                detail: "x".into(),
            },
            db_path: Some(path),
        });
        handle.recover(&FixedClock(EpochMillis(5))).unwrap();
        assert_eq!(handle.status(), StoreStatus::Ready);
        assert_eq!(backups(&dir).len(), 1);
    }

    #[test]
    fn recover_should_refuse_a_ready_store_without_creating_backups() {
        let dir = TempDir::new().unwrap();
        let handle = handle_for(&dir);
        let err = handle.recover(&FixedClock(EpochMillis(1))).unwrap_err();
        assert!(matches!(err, RecoveryError::NotRecoverable(_)));
        assert!(backups(&dir).is_empty());
    }

    #[test]
    fn recover_should_refuse_a_newer_schema_and_keep_the_bytes() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("data.db");
        drop(SqliteStore::open(&path).unwrap());
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.pragma_update(None, "user_version", 99).unwrap();
        drop(conn);
        let before = std::fs::read(&path).unwrap();
        let handle = handle_for(&dir);
        assert!(matches!(
            handle.status(),
            StoreStatus::Unavailable {
                reason: UnavailableReason::SchemaTooNew,
                recoverable: false
            }
        ));
        let err = handle.recover(&FixedClock(EpochMillis(1))).unwrap_err();
        assert!(matches!(err, RecoveryError::NotRecoverable(_)));
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert!(backups(&dir).is_empty());
    }

    #[test]
    fn recover_should_refuse_when_the_path_is_unknown() {
        let handle = StoreHandle::new(open_store_state(Err("sin perfil".into())));
        let err = handle.recover(&FixedClock(EpochMillis(1))).unwrap_err();
        assert!(matches!(err, RecoveryError::NotRecoverable(_)));
    }

    #[test]
    fn recover_should_fail_cleanly_and_keep_state_when_the_file_is_locked() {
        let (dir, handle) = corrupt_handle();
        let path = dir.path().join("data.db");
        let _lock = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&path)
            .unwrap();
        let err = handle.recover(&FixedClock(EpochMillis(1))).unwrap_err();
        assert!(matches!(
            err,
            RecoveryError::Failed(StoreError::Unavailable(_))
        ));
        assert!(backups(&dir).is_empty());
        assert_eq!(handle.status(), CORRUPT);
    }

    #[test]
    fn concurrent_recover_should_succeed_exactly_once() {
        let (dir, handle) = corrupt_handle();
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let threads: Vec<_> = (0..2)
            .map(|_| {
                let handle = handle.clone();
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    handle.recover(&FixedClock(EpochMillis(42)))
                })
            })
            .collect();
        let results: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
        let ok = results.iter().filter(|r| r.is_ok()).count();
        let refused = results
            .iter()
            .filter(|r| matches!(r, Err(RecoveryError::NotRecoverable(_))))
            .count();
        assert_eq!((ok, refused), (1, 1), "{results:?}");
        assert_eq!(backups(&dir).len(), 1);
    }
}

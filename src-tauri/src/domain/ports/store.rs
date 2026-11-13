//! Puertos de persistencia: ajustes, escaneos y bitacora de limpieza.

use crate::domain::model::{
    CleanupLogEntry, EpochMillis, NewCleanupLogEntry, NewScan, Scan, ScanId, ScanStatus,
};

/// Fallo de persistencia. No contiene SQL ni tipos de `rusqlite`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StoreError {
    #[error("database is corrupt: {0}")]
    Corrupt(String),
    #[error("database schema {found} is newer than supported {supported}")]
    SchemaTooNew { found: u32, supported: u32 },
    #[error("migration {version} failed: {detail}")]
    MigrationFailed { version: u32, detail: String },
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid state: {0}")]
    InvalidState(String),
    #[error("constraint violated: {0}")]
    Constraint(String),
    #[error("database unavailable: {0}")]
    Unavailable(String),
}

/// Ajustes clave -> JSON.
pub trait SettingsStore {
    fn get_setting(&self, key: &str) -> Result<Option<serde_json::Value>, StoreError>;
    /// Ordenados por clave.
    fn list_settings(&self) -> Result<Vec<(String, serde_json::Value)>, StoreError>;
    /// Inserta o reemplaza. Clave vacia -> `Constraint`.
    fn set_setting(&self, key: &str, value: &serde_json::Value) -> Result<(), StoreError>;
}

/// Escaneos. `create_scan` aplica la retencion (`SCAN_RETENTION`) en la misma operacion.
pub trait ScanStore {
    fn create_scan(&self, scan: &NewScan) -> Result<ScanId, StoreError>;
    /// `status` debe ser terminal; solo un escaneo `Running` puede finalizarse.
    fn finish_scan(
        &self,
        id: ScanId,
        status: ScanStatus,
        finished_at: EpochMillis,
    ) -> Result<(), StoreError>;
    fn get_scan(&self, id: ScanId) -> Result<Option<Scan>, StoreError>;
    /// Mas reciente primero.
    fn list_scans(&self) -> Result<Vec<Scan>, StoreError>;
}

/// Bitacora de limpieza append-only: no hay metodos de modificacion ni borrado.
pub trait CleanupLogStore {
    /// Atomico: si una entrada es invalida no se guarda ninguna.
    fn append_cleanup_log(&self, entries: &[NewCleanupLogEntry]) -> Result<(), StoreError>;
    /// Orden `ts DESC, id DESC`.
    fn recent_cleanup_log(&self, limit: u32) -> Result<Vec<CleanupLogEntry>, StoreError>;
}

/// Persistencia completa.
pub trait Store: SettingsStore + ScanStore + CleanupLogStore {}
impl<T: SettingsStore + ScanStore + CleanupLogStore> Store for T {}

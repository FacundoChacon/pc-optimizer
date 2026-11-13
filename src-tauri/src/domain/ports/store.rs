//! Puertos de persistencia: ajustes, escaneos, bitacora de limpieza y hallazgos de basura.

use crate::domain::junk::category::JunkCategory;
use crate::domain::junk::model::{JunkFindingPage, JunkScanResults, JunkScanSummary};
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

/// Hallazgos de basura (C-08). Solo registros de la base: ninguna operacion actua sobre archivos.
pub trait JunkFindingStore {
    /// Guarda de forma ATOMICA los hallazgos y el resumen por categoria de un escaneo de basura
    /// `Running`, elimina los hallazgos de todo escaneo de basura anterior (se conserva su resumen)
    /// y finaliza el escaneo como `Done`. Si algo falla no queda nada del escaneo nuevo y el
    /// escaneo sigue `Running`.
    ///
    /// Errores: `NotFound` (escaneo inexistente), `InvalidState` (no es de basura o no esta
    /// `Running`), `Constraint` (resumen o hallazgo invalido, p. ej. ruta repetida).
    fn complete_junk_scan(
        &self,
        scan_id: ScanId,
        results: &JunkScanResults,
        finished_at: EpochMillis,
    ) -> Result<(), StoreError>;
    /// Resumen del escaneo de basura `Done` mas reciente; ignora `Cancelled`/`Error` y otros tipos.
    fn last_done_junk_scan(&self) -> Result<Option<JunkScanSummary>, StoreError>;
    /// Hallazgos de una categoria, `size_bytes DESC, id ASC`, con el total. Un escaneo sin
    /// hallazgos (o desconocido) da una pagina vacia.
    fn list_junk_findings(
        &self,
        scan_id: ScanId,
        category: JunkCategory,
        offset: u64,
        limit: u32,
    ) -> Result<JunkFindingPage, StoreError>;
}

/// Persistencia completa.
pub trait Store: SettingsStore + ScanStore + CleanupLogStore + JunkFindingStore {}
impl<T: SettingsStore + ScanStore + CleanupLogStore + JunkFindingStore> Store for T {}

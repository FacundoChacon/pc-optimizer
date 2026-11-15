//! Puertos de persistencia: ajustes, escaneos, bitacora de limpieza y hallazgos de basura.

use crate::domain::disk_usage::model::{
    FolderPage, FolderRow, FolderScanResults, FolderScanSummary, FolderSort, LargeFilePage,
    LargeFileRow, LargeFileSort,
};
use crate::domain::duplicates::model::{
    DuplicateGroupPage, DuplicateScanResults, DuplicateScanSummary, GroupResolution,
    StoredDuplicateGroup,
};
use crate::domain::junk::category::JunkCategory;
use crate::domain::junk::model::{
    JunkFindingPage, JunkScanResults, JunkScanSummary, StoredFinding,
};
use crate::domain::model::{
    CleanupLogEntry, EpochMillis, NewCleanupLogEntry, NewScan, Scan, ScanId, ScanStatus,
};
use crate::domain::protection::context::ContentFolder;

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
    /// Fecha (`ts` maximo) de la ultima entrada del log asociada a `scan_id` con `ts >= since`
    /// (el fin del escaneo), o `None` si no hay ninguna. Asi se deriva que un escaneo ya fue
    /// limpiado sin agregar tablas ni columnas (C-09, D-20).
    fn scan_cleaned_at(
        &self,
        scan_id: ScanId,
        since: EpochMillis,
    ) -> Result<Option<EpochMillis>, StoreError>;
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
    /// Hallazgos ELEGIBLES (no omitidos) de `scan_id` en las `categories` pedidas, con su
    /// identidad: orden de categoria segun `JunkCategory::ALL`, luego `size_bytes DESC` y `id ASC`.
    /// Un escaneo desconocido, sin hallazgos o sin categorias pedidas da una lista vacia.
    fn cleanup_candidates(
        &self,
        scan_id: ScanId,
        categories: &[JunkCategory],
    ) -> Result<Vec<StoredFinding>, StoreError>;
}

/// Duplicados (C-11). Solo registros de la base: ninguna operacion actua sobre archivos y la
/// unica que escribe la marca de conservada es `resolve_duplicate_groups`.
pub trait DuplicateStore {
    /// Guarda de forma ATOMICA el resultado, el estado por carpeta, los grupos y las copias de un
    /// escaneo de duplicados `Running`, elimina los grupos y copias de todo escaneo de duplicados
    /// anterior (se conserva su escaneo y su resultado) y finaliza el escaneo como `Done`. Si algo
    /// falla no queda nada del escaneo nuevo y el escaneo sigue `Running`.
    ///
    /// Errores: `NotFound` (escaneo inexistente), `InvalidState` (no es de duplicados o no esta
    /// `Running`), `Constraint` (resultado invalido: ver `DuplicateScanResults::validate`).
    fn complete_duplicate_scan(
        &self,
        scan_id: ScanId,
        results: &DuplicateScanResults,
        finished_at: EpochMillis,
    ) -> Result<(), StoreError>;
    /// Resumen del escaneo de duplicados `Done` mas reciente; ignora `Cancelled`/`Error` y otros
    /// tipos.
    fn last_done_duplicate_scan(&self) -> Result<Option<DuplicateScanSummary>, StoreError>;
    /// Grupos con sus copias, por bytes recuperables `size * (n - 1)` descendentes y luego por
    /// id, con el total filtrado. `root`: grupos con al menos una copia en esa carpeta;
    /// `min_size`: grupos de `size_bytes >= min_size`. Un escaneo desconocido o una pagina fuera de
    /// rango da una lista vacia.
    fn list_duplicate_groups(
        &self,
        scan_id: ScanId,
        offset: u64,
        limit: u32,
        root: Option<ContentFolder>,
        min_size: Option<u64>,
    ) -> Result<DuplicateGroupPage, StoreError>;
    /// Los grupos `group_ids` que pertenecen a `scan_id`, con sus copias, ordenados por id. Los
    /// ids desconocidos o de otro escaneo no aparecen.
    fn get_duplicate_groups(
        &self,
        scan_id: ScanId,
        group_ids: &[i64],
    ) -> Result<Vec<StoredDuplicateGroup>, StoreError>;
    /// Resuelve los grupos en una sola transaccion: fecha de resolucion y marca de conservada solo
    /// en las copias indicadas. Rechaza sin cambios (`Constraint`/`NotFound`/`InvalidState`) un
    /// grupo ya resuelto, de otro escaneo, sin copias conservadas o con copias ajenas al grupo.
    fn resolve_duplicate_groups(
        &self,
        scan_id: ScanId,
        resolutions: &[GroupResolution],
        now: EpochMillis,
    ) -> Result<(), StoreError>;
}

/// Analisis de espacio en disco (C-07). Solo registros de la base: ninguna operacion actua sobre
/// archivos.
pub trait FolderStore {
    /// Guarda de forma ATOMICA el resultado, las carpetas (el padre antes que la hija), los
    /// archivos grandes y las incidencias de un analisis `Running`, elimina las carpetas, archivos
    /// e incidencias de todo analisis anterior (se conservan su escaneo y su resultado) y
    /// finaliza el escaneo como `Done`. Si algo falla no queda nada y el escaneo sigue `Running`.
    ///
    /// Errores: `NotFound` (escaneo inexistente), `InvalidState` (no es de carpetas o no esta
    /// `Running`), `Constraint` (resultado invalido: ver `FolderScanResults::validate`).
    fn complete_folder_scan(
        &self,
        scan_id: ScanId,
        results: &FolderScanResults,
        finished_at: EpochMillis,
    ) -> Result<(), StoreError>;
    /// Resumen del analisis `Done` mas reciente; ignora `Cancelled`/`Error` y otros tipos.
    fn last_done_folder_scan(&self) -> Result<Option<FolderScanSummary>, StoreError>;
    /// Resumen de un analisis concreto; `None` si no existe o no tiene resultado.
    fn folder_scan_summary(&self, scan_id: ScanId)
    -> Result<Option<FolderScanSummary>, StoreError>;
    /// Hijos de una carpeta (`None` = la raiz del analisis), ordenados, con sus migas de pan y el
    /// resto. `NotFound` si el analisis ya no tiene carpetas o la carpeta es de otro analisis.
    fn list_folder_children(
        &self,
        scan_id: ScanId,
        folder_id: Option<i64>,
        sort: FolderSort,
        offset: u64,
        limit: u32,
    ) -> Result<FolderPage, StoreError>;
    /// Archivos grandes del analisis; un analisis sin archivos (o desconocido) da una pagina vacia.
    fn list_large_files(
        &self,
        scan_id: ScanId,
        sort: LargeFileSort,
        offset: u64,
        limit: u32,
    ) -> Result<LargeFilePage, StoreError>;
    /// Una carpeta del analisis; `None` si no existe o es de otro analisis.
    fn get_folder(&self, scan_id: ScanId, folder_id: i64) -> Result<Option<FolderRow>, StoreError>;
    /// Un archivo grande del analisis; `None` si no existe o es de otro analisis.
    fn get_large_file(
        &self,
        scan_id: ScanId,
        file_id: i64,
    ) -> Result<Option<LargeFileRow>, StoreError>;
}

/// Persistencia completa.
pub trait Store:
    SettingsStore + ScanStore + CleanupLogStore + JunkFindingStore + DuplicateStore + FolderStore
{
}
impl<
    T: SettingsStore + ScanStore + CleanupLogStore + JunkFindingStore + DuplicateStore + FolderStore,
> Store for T
{
}

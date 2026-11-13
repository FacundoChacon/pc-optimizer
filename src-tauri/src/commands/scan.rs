//! Comandos del escaneo de basura (C-08): `start_scan`, `cancel_scan`, `get_last_junk_scan` y
//! `list_junk_findings`.
//!
//! Los comandos son finos: validan argumentos, arman las dependencias reales y delegan en
//! funciones genericas (`run_junk_scan`, `last_junk_scan`, `list_findings`) que se prueban con
//! dobles. La deteccion NUNCA borra ni mueve archivos.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use serde::Serialize;
use tauri::State;
use tauri::ipc::Channel;

use super::run_blocking;
use crate::adapters::clock::SystemClock;
use crate::adapters::sqlite::StoreHandle;
use crate::adapters::windows::{
    UserFolderSource, WindowsFileSystem, WindowsRecycleBin, resolve_protection_context,
};
use crate::domain::junk::category::JunkCategory;
use crate::domain::junk::model::{
    CategoryResult, CategoryStatus, FindingState, JunkFindingPage, JunkScanOutcome, JunkScanSummary,
};
use crate::domain::junk::scanner::{ScanObserver, scan_junk};
use crate::domain::model::{NewScan, ScanId, ScanKind, ScanStatus, ScanTrigger};
use crate::domain::ports::{Clock, FileSystem, RecycleBinInfo, Store};
use crate::domain::privileges::{PrivilegeContext, restrict_protection_context};
use crate::domain::protection::context::{ProtectionContext, RawProtectionContext};
use crate::error::AppError;

/// Tamano maximo de una pagina de hallazgos.
pub const MAX_PAGE_SIZE: u32 = 500;

// ---------------------------------------------------------------------------------------------
// DTOs (camelCase hacia el frontend; sin SID, nombres de cuenta ni rutas fuera de los hallazgos)
// ---------------------------------------------------------------------------------------------

/// Resumen de una categoria.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryDto {
    pub category: &'static str,
    pub scope: &'static str,
    pub status: &'static str,
    /// Solo con `partial` y `blocked`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_reason: Option<&'static str>,
    pub item_count: u64,
    pub size_bytes: u64,
    pub skipped_count: u64,
    pub inaccessible_count: u64,
}

impl From<&CategoryResult> for CategoryDto {
    fn from(result: &CategoryResult) -> Self {
        let reason_applies = matches!(
            result.status,
            CategoryStatus::Partial | CategoryStatus::Blocked
        );
        Self {
            category: result.category.as_str(),
            scope: result.scope.as_str(),
            status: result.status.as_str(),
            block_reason: result
                .block_reason
                .filter(|_| reason_applies)
                .map(|reason| reason.as_str()),
            item_count: result.item_count,
            size_bytes: result.size_bytes,
            skipped_count: result.skipped_count,
            inaccessible_count: result.inaccessible_count,
        }
    }
}

/// Resumen de un escaneo de basura (`done` o `cancelled`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JunkScanSummaryDto {
    pub scan_id: i64,
    pub status: &'static str,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub total_items: u64,
    pub total_bytes: u64,
    pub categories: Vec<CategoryDto>,
}

impl From<&JunkScanSummary> for JunkScanSummaryDto {
    fn from(summary: &JunkScanSummary) -> Self {
        Self {
            scan_id: summary.scan.id.0,
            status: summary.scan.status.as_str(),
            started_at: summary.scan.started_at.0,
            finished_at: summary.scan.finished_at.map(|at| at.0),
            total_items: summary.total_items(),
            total_bytes: summary.total_bytes(),
            categories: summary.categories.iter().map(CategoryDto::from).collect(),
        }
    }
}

/// Evento de progreso enviado por el canal de la invocacion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ScanEventDto {
    #[serde(rename_all = "camelCase")]
    Started { scan_id: i64 },
    #[serde(rename_all = "camelCase")]
    Category { category: &'static str },
    #[serde(rename_all = "camelCase")]
    Progress {
        category: &'static str,
        files_seen: u64,
        eligible_bytes: u64,
    },
}

/// Un hallazgo para listar (unico DTO que lleva rutas).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FindingDto {
    pub id: i64,
    pub path: String,
    pub size_bytes: u64,
    pub modified_at: Option<i64>,
    /// `None` en los elegibles; el codigo estable del motivo en los omitidos.
    pub skip_reason: Option<String>,
}

/// Pagina de hallazgos de una categoria.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FindingPageDto {
    pub total: u64,
    pub items: Vec<FindingDto>,
}

impl From<JunkFindingPage> for FindingPageDto {
    fn from(page: JunkFindingPage) -> Self {
        Self {
            total: page.total,
            items: page
                .items
                .into_iter()
                .map(|stored| FindingDto {
                    id: stored.id,
                    path: stored.finding.path,
                    size_bytes: stored.finding.size_bytes,
                    modified_at: stored.finding.modified_at.map(|at| at.0),
                    skip_reason: match stored.finding.state {
                        FindingState::Eligible(_) => None,
                        FindingState::Skipped(code) => Some(code),
                    },
                })
                .collect(),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Estado: escaneo activo (D-14) y entorno (D-17)
// ---------------------------------------------------------------------------------------------

/// Directorio de datos efectivo de la app (parte de las carpetas protegidas del escaneo).
#[derive(Debug, Default)]
pub struct ScanEnvironment {
    data_dir: Mutex<Option<PathBuf>>,
}

impl ScanEnvironment {
    /// Fija el directorio de datos (lo hace `manage_store` al arrancar).
    pub fn set_data_dir(&self, dir: Option<PathBuf>) {
        *lock(&self.data_dir) = dir;
    }

    /// Directorio de datos, si se pudo resolver.
    pub fn data_dir(&self) -> Option<PathBuf> {
        lock(&self.data_dir).clone()
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // Un panic de otro hilo no invalida estos datos simples: se sigue con su contenido.
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Debug)]
struct ActiveEntry {
    scan_id: Option<ScanId>,
    cancel: Arc<AtomicBool>,
}

/// Escaneo en curso: como maximo uno a la vez.
#[derive(Debug, Clone, Default)]
pub struct ActiveScan(Arc<Mutex<Option<ActiveEntry>>>);

impl ActiveScan {
    /// Toma el lugar del escaneo; falla `scan_in_progress` si ya hay uno. El lugar se libera
    /// al soltar el guard, tambien si la tarea falla.
    pub fn begin(&self) -> Result<ScanGuard, AppError> {
        let mut slot = lock(&self.0);
        if slot.is_some() {
            return Err(AppError::ScanInProgress);
        }
        let cancel = Arc::new(AtomicBool::new(false));
        *slot = Some(ActiveEntry {
            scan_id: None,
            cancel: Arc::clone(&cancel),
        });
        Ok(ScanGuard {
            owner: self.clone(),
            cancel,
        })
    }

    /// Pide cancelar el escaneo `id`; `scan_not_found` si no es el activo.
    pub fn cancel(&self, id: ScanId) -> Result<(), AppError> {
        let slot = lock(&self.0);
        match slot.as_ref() {
            Some(entry) if entry.scan_id == Some(id) => {
                // Solo es una bandera: no ordena otros accesos a memoria.
                entry.cancel.store(true, Ordering::Relaxed);
                Ok(())
            }
            _ => Err(AppError::ScanNotFound(id.0.to_string())),
        }
    }
}

/// Posee el lugar del escaneo activo hasta que se suelta.
#[derive(Debug)]
pub struct ScanGuard {
    owner: ActiveScan,
    cancel: Arc<AtomicBool>,
}

impl ScanGuard {
    /// Asocia el identificador persistido (desde ahi se puede cancelar).
    pub fn bind(&self, id: ScanId) {
        if let Some(entry) = lock(&self.owner.0).as_mut() {
            entry.scan_id = Some(id);
        }
    }

    /// Bandera de cancelacion que consulta el dominio.
    pub fn cancel_flag(&self) -> &AtomicBool {
        &self.cancel
    }
}

impl Drop for ScanGuard {
    fn drop(&mut self) {
        *lock(&self.owner.0) = None;
    }
}

// ---------------------------------------------------------------------------------------------
// Logica de los comandos (generica, sin Tauri)
// ---------------------------------------------------------------------------------------------

/// Adapta el progreso del dominio a eventos del canal.
struct EventObserver<'a> {
    emit: &'a mut dyn FnMut(ScanEventDto),
}

impl ScanObserver for EventObserver<'_> {
    fn category(&mut self, category: JunkCategory) {
        (self.emit)(ScanEventDto::Category {
            category: category.as_str(),
        });
    }

    fn progress(&mut self, category: JunkCategory, files_seen: u64, eligible_bytes: u64) {
        (self.emit)(ScanEventDto::Progress {
            category: category.as_str(),
            files_seen,
            eligible_bytes,
        });
    }
}

/// Dependencias de un escaneo (puertos del SO y de la persistencia).
pub struct ScanPorts<'a, S: ?Sized, F, R, C> {
    pub store: &'a S,
    pub fs: &'a F,
    pub recycle_bin: &'a R,
    pub clock: &'a C,
}

/// Corre un escaneo de basura completo: lo crea, detecta, persiste y devuelve el resumen.
///
/// - El contexto de proteccion se arma por escaneo y se restringe por privilegios (D-11, D-17).
/// - La unidad de la Papelera sale de `windows_dir` del contexto crudo, nunca de una letra fija.
/// - Cancelado: el escaneo queda `cancelled` y no se guarda ningun hallazgo.
/// - Fallo al guardar: el escaneo se cierra como `error` y el fallo se propaga.
pub fn run_junk_scan<S, F, R, C>(
    ports: &ScanPorts<'_, S, F, R, C>,
    privileges: &PrivilegeContext,
    raw: RawProtectionContext,
    guard: &ScanGuard,
    emit: &mut dyn FnMut(ScanEventDto),
) -> Result<JunkScanSummaryDto, AppError>
where
    S: Store + ?Sized,
    F: FileSystem,
    R: RecycleBinInfo,
    C: Clock,
{
    let store = ports.store;
    let started_at = ports.clock.now();
    let scan_id = store.create_scan(&NewScan {
        kind: ScanKind::Junk,
        trigger: ScanTrigger::Manual,
        started_at,
    })?;
    guard.bind(scan_id);
    emit(ScanEventDto::Started { scan_id: scan_id.0 });

    let system_drive = raw.system_drive();
    let protection = ProtectionContext::from_raw(restrict_protection_context(raw, privileges));
    let outcome = scan_junk(
        privileges,
        protection,
        system_drive,
        ports.fs,
        ports.recycle_bin,
        started_at,
        guard.cancel_flag(),
        &mut EventObserver { emit },
    );
    match outcome {
        JunkScanOutcome::Completed(results) => {
            if let Err(error) = store.complete_junk_scan(scan_id, &results, ports.clock.now()) {
                // El escaneo sigue `running`: se cierra como error (si tambien falla, ya no hay
                // nada mas que hacer; D-16 lo cierra al reabrir).
                let _ = store.finish_scan(scan_id, ScanStatus::Error, ports.clock.now());
                return Err(error.into());
            }
            match store.last_done_junk_scan()? {
                Some(summary) if summary.scan.id == scan_id => Ok((&summary).into()),
                _ => Err(AppError::Internal(
                    "the finished scan is missing from the store".to_owned(),
                )),
            }
        }
        JunkScanOutcome::Cancelled => {
            let finished_at = ports.clock.now();
            store.finish_scan(scan_id, ScanStatus::Cancelled, finished_at)?;
            Ok(JunkScanSummaryDto {
                scan_id: scan_id.0,
                status: ScanStatus::Cancelled.as_str(),
                started_at: started_at.0,
                finished_at: Some(finished_at.0),
                total_items: 0,
                total_bytes: 0,
                categories: Vec::new(),
            })
        }
    }
}

/// Resumen del ultimo escaneo de basura terminado, si existe.
pub fn last_junk_scan<S: Store + ?Sized>(
    store: &S,
) -> Result<Option<JunkScanSummaryDto>, AppError> {
    Ok(store.last_done_junk_scan()?.as_ref().map(Into::into))
}

/// Una pagina de hallazgos del ultimo escaneo terminado (valida todos los argumentos).
pub fn list_findings<S: Store + ?Sized>(
    store: &S,
    scan_id: i64,
    category: &str,
    offset: i64,
    limit: i64,
) -> Result<FindingPageDto, AppError> {
    let category = JunkCategory::parse(category)
        .ok_or_else(|| AppError::InvalidArgument("unknown category".to_owned()))?;
    let offset = u64::try_from(offset)
        .map_err(|_| AppError::InvalidArgument("offset must be 0 or more".to_owned()))?;
    let limit = u32::try_from(limit)
        .ok()
        .filter(|limit| (1..=MAX_PAGE_SIZE).contains(limit))
        .ok_or_else(|| {
            AppError::InvalidArgument(format!("limit must be between 1 and {MAX_PAGE_SIZE}"))
        })?;
    match store.last_done_junk_scan()? {
        Some(summary) if summary.scan.id.0 == scan_id => {}
        _ => return Err(AppError::ScanNotFound(scan_id.to_string())),
    }
    Ok(store
        .list_junk_findings(ScanId(scan_id), category, offset, limit)?
        .into())
}

/// Carpetas de la app que la proteccion debe vetar: datos y carpeta del ejecutable.
pub fn app_dirs(data_dir: Option<&Path>, exe: Option<&Path>) -> Vec<String> {
    let exe_dir = exe.and_then(Path::parent);
    [data_dir, exe_dir]
        .into_iter()
        .flatten()
        .filter_map(|dir| dir.to_str().map(str::to_owned))
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Comandos IPC
// ---------------------------------------------------------------------------------------------

/// Comando IPC: inicia un escaneo de basura; el progreso viaja por `on_progress` y la respuesta
/// es el resumen final (`done` o `cancelled`).
#[tauri::command]
pub async fn start_scan(
    kind: String,
    on_progress: Channel<ScanEventDto>,
    handle: State<'_, StoreHandle>,
    active: State<'_, ActiveScan>,
    privileges: State<'_, PrivilegeContext>,
    environment: State<'_, ScanEnvironment>,
) -> Result<JunkScanSummaryDto, AppError> {
    if kind != "junk" {
        return Err(AppError::ScanKindUnsupported(kind));
    }
    let guard = active.begin()?;
    let handle = handle.inner().clone();
    let privileges = *privileges.inner();
    let data_dir = environment.data_dir();
    run_blocking(move || {
        let store = handle.store()?;
        let exe = std::env::current_exe().ok();
        let raw = resolve_protection_context(
            UserFolderSource::ProcessToken,
            app_dirs(data_dir.as_deref(), exe.as_deref()),
        );
        let ports = ScanPorts {
            store: &*store,
            fs: &WindowsFileSystem,
            recycle_bin: &WindowsRecycleBin,
            clock: &SystemClock,
        };
        // Un error de envio se ignora: la UI pudo cerrarse y el escaneo igual se completa.
        run_junk_scan(&ports, &privileges, raw, &guard, &mut |event| {
            let _ = on_progress.send(event);
        })
    })
    .await
}

/// Comando IPC: cancela el escaneo en curso.
#[tauri::command]
pub async fn cancel_scan(scan_id: i64, active: State<'_, ActiveScan>) -> Result<(), AppError> {
    active.cancel(ScanId(scan_id))
}

/// Comando IPC: resumen del ultimo escaneo de basura terminado (`null` si no hay).
#[tauri::command]
pub async fn get_last_junk_scan(
    handle: State<'_, StoreHandle>,
) -> Result<Option<JunkScanSummaryDto>, AppError> {
    let handle = handle.inner().clone();
    run_blocking(move || last_junk_scan(&*handle.store()?)).await
}

/// Comando IPC: pagina de hallazgos de una categoria del ultimo escaneo terminado.
#[tauri::command]
pub async fn list_junk_findings(
    scan_id: i64,
    category: String,
    offset: i64,
    limit: i64,
    handle: State<'_, StoreHandle>,
) -> Result<FindingPageDto, AppError> {
    let handle = handle.inner().clone();
    run_blocking(move || list_findings(&*handle.store()?, scan_id, &category, offset, limit)).await
}

#[cfg(test)]
#[path = "scan_tests.rs"]
mod tests;

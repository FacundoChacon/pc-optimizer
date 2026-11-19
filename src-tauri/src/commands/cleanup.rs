//! Comandos de la limpieza (C-09): `preview_cleanup`, `confirm_cleanup` y `cancel_cleanup`.
//!
//! Los comandos son finos: validan argumentos, arman las dependencias reales y delegan en
//! funciones genericas (`create_preview`, `confirm_cleanup_run`, `cancel_cleanup_run`) que se
//! prueban con dobles. La interfaz solo envia identificadores y nombres de categoria, NUNCA
//! rutas; las respuestas no llevan rutas, SIDs ni nombres de cuenta.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::State;
use tauri::ipc::Channel;

use super::run_blocking;
use super::scan::{ActiveJob, ScanEnvironment, app_dirs, lock};
use crate::adapters::clock::SystemClock;
use crate::adapters::sqlite::StoreHandle;
use crate::adapters::windows::trash::WindowsTrash;
use crate::adapters::windows::{UserFolderSource, WindowsFileSystem, resolve_protection_context};
use crate::domain::cleanup::execute::{CleanupObserver, run_cleanup};
use crate::domain::cleanup::lifecycle::{PreviewSlot, SlotError};
use crate::domain::cleanup::model::{
    CleanupPreview, CleanupStatus, CleanupSummary, PreviewCategoryStatus, PreviewId,
};
use crate::domain::cleanup::preview::{
    PreviewError, PreviewInput, build_preview, validate_categories,
};
use crate::domain::junk::category::JunkCategory;
use crate::domain::model::ScanId;
use crate::domain::ports::{Clock, FileSystem, Store, Trash, TrashError};
use crate::domain::privileges::{PrivilegeContext, restrict_protection_context};
use crate::domain::protection::context::{ProtectionContext, RawProtectionContext};
use crate::error::AppError;

/// Mayor entero que JavaScript representa sin perdida (`Number.MAX_SAFE_INTEGER`).
const MAX_SAFE_ID: i64 = (1 << 53) - 1;

// ---------------------------------------------------------------------------------------------
// DTOs (camelCase hacia el frontend; sin rutas, SIDs ni nombres de cuenta)
// ---------------------------------------------------------------------------------------------

/// Una categoria del preview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewCategoryDto {
    pub category: &'static str,
    /// `included` o `blocked`.
    pub status: &'static str,
    /// Solo con `blocked`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_reason: Option<&'static str>,
    pub item_count: u64,
    pub size_bytes: u64,
    pub recent_count: u64,
    pub recent_bytes: u64,
}

/// Preview de una limpieza: lo que se movera si el usuario confirma.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupPreviewDto {
    pub preview_id: u64,
    pub scan_id: i64,
    pub created_at: i64,
    pub expires_at: i64,
    pub total_items: u64,
    pub total_bytes: u64,
    /// `false` si la Papelera esta deshabilitada o su politica no se pudo leer: la interfaz
    /// advierte y no ofrece confirmar (D-12 B).
    pub recycle_bin_available: bool,
    pub categories: Vec<PreviewCategoryDto>,
}

impl CleanupPreviewDto {
    fn new(preview: &CleanupPreview, recycle_bin_available: bool) -> Self {
        Self {
            preview_id: preview.id().0,
            scan_id: preview.scan_id().0,
            created_at: preview.created_at().0,
            expires_at: preview.expires_at().0,
            total_items: preview.total_items(),
            total_bytes: preview.total_bytes(),
            recycle_bin_available,
            categories: preview
                .categories()
                .iter()
                .map(|category| {
                    let (status, block_reason) = match category.status {
                        PreviewCategoryStatus::Included => ("included", None),
                        PreviewCategoryStatus::Blocked(reason) => {
                            ("blocked", Some(reason.as_str()))
                        }
                    };
                    PreviewCategoryDto {
                        category: category.category.as_str(),
                        status,
                        block_reason,
                        item_count: category.item_count,
                        size_bytes: category.size_bytes,
                        recent_count: category.recent_count,
                        recent_bytes: category.recent_bytes,
                    }
                })
                .collect(),
        }
    }
}

/// Cuantos items terminaron con un resultado y detalle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReasonDto {
    pub outcome: &'static str,
    pub detail: String,
    pub count: u64,
}

/// Totales de una categoria en el resumen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategorySummaryDto {
    pub category: &'static str,
    pub moved_count: u64,
    pub moved_bytes: u64,
    pub skipped_count: u64,
    pub error_count: u64,
}

/// Resumen de una ejecucion (`completed`, `cancelled` o `aborted`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupSummaryDto {
    pub preview_id: u64,
    pub scan_id: i64,
    pub status: &'static str,
    /// `null` salvo con `aborted`.
    pub abort_reason: Option<&'static str>,
    pub started_at: i64,
    pub finished_at: i64,
    pub total_items: u64,
    pub processed: u64,
    pub not_processed: u64,
    pub moved_count: u64,
    pub moved_bytes: u64,
    pub skipped_locked: u64,
    pub skipped_protected: u64,
    pub skipped_cloud: u64,
    pub errors: u64,
    /// Ordenado por cantidad descendente.
    pub reasons: Vec<ReasonDto>,
    pub categories: Vec<CategorySummaryDto>,
}

impl From<&CleanupSummary> for CleanupSummaryDto {
    fn from(summary: &CleanupSummary) -> Self {
        let (status, abort_reason) = match summary.status {
            CleanupStatus::Completed => ("completed", None),
            CleanupStatus::Cancelled => ("cancelled", None),
            CleanupStatus::Aborted(reason) => ("aborted", Some(reason.as_str())),
        };
        Self {
            preview_id: summary.preview_id.0,
            scan_id: summary.scan_id.0,
            status,
            abort_reason,
            started_at: summary.started_at.0,
            finished_at: summary.finished_at.0,
            total_items: summary.total_items,
            processed: summary.processed,
            not_processed: summary.not_processed,
            moved_count: summary.moved_count,
            moved_bytes: summary.moved_bytes,
            skipped_locked: summary.skipped_locked,
            skipped_protected: summary.skipped_protected,
            skipped_cloud: summary.skipped_cloud,
            errors: summary.errors,
            reasons: summary
                .reasons
                .iter()
                .map(|reason| ReasonDto {
                    outcome: reason.outcome.as_str(),
                    detail: reason.detail.clone(),
                    count: reason.count,
                })
                .collect(),
            categories: summary
                .categories
                .iter()
                .map(|category| CategorySummaryDto {
                    category: category.category.as_str(),
                    moved_count: category.moved_count,
                    moved_bytes: category.moved_bytes,
                    skipped_count: category.skipped_count,
                    error_count: category.error_count,
                })
                .collect(),
        }
    }
}

/// Evento de progreso enviado por el canal de la invocacion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CleanupEventDto {
    #[serde(rename_all = "camelCase")]
    Started { preview_id: u64, total: u64 },
    #[serde(rename_all = "camelCase")]
    Progress {
        processed: u64,
        total: u64,
        moved_count: u64,
        moved_bytes: u64,
    },
}

// ---------------------------------------------------------------------------------------------
// Estado: el preview activo (D-01)
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Default)]
struct PreviewsInner {
    slot: Mutex<PreviewSlot>,
    next_id: AtomicU64,
}

/// El unico preview activo y el contador de identificadores del proceso (estado gestionado).
#[derive(Debug, Clone, Default)]
pub struct CleanupPreviews(Arc<PreviewsInner>);

impl CleanupPreviews {
    /// Identificador siguiente: contador monotono desde 1.
    fn next_id(&self) -> PreviewId {
        // Solo es un contador: no ordena otros accesos a memoria.
        PreviewId(self.0.next_id.fetch_add(1, Ordering::Relaxed) + 1)
    }

    fn store(&self, preview: CleanupPreview) {
        lock(&self.0.slot).store(preview);
    }

    fn take(
        &self,
        id: PreviewId,
        now: crate::domain::model::EpochMillis,
    ) -> Result<CleanupPreview, AppError> {
        lock(&self.0.slot)
            .take(id, now)
            .map_err(|error| match error {
                SlotError::PreviewNotFound => AppError::PreviewNotFound,
                SlotError::PreviewExpired => AppError::PreviewExpired,
            })
    }
}

// ---------------------------------------------------------------------------------------------
// Logica de los comandos (generica, sin Tauri)
// ---------------------------------------------------------------------------------------------

/// Dependencias de la limpieza (puertos del SO, la persistencia y el estado de la sesion).
pub struct CleanupPorts<'a, S, F, C> {
    pub store: &'a S,
    pub fs: &'a F,
    pub clock: &'a C,
    pub previews: &'a CleanupPreviews,
    pub active: &'a ActiveJob,
}

/// Identificador que viaja como entero JS: de 0 a 2^53 - 1.
fn safe_id(raw: i64, what: &str) -> Result<u64, AppError> {
    u64::try_from(raw)
        .ok()
        .filter(|_| raw <= MAX_SAFE_ID)
        .ok_or_else(|| AppError::InvalidArgument(format!("{what} is out of range")))
}

fn parse_categories(names: &[String]) -> Result<Vec<JunkCategory>, AppError> {
    names
        .iter()
        .map(|name| {
            JunkCategory::parse(name)
                .ok_or_else(|| AppError::InvalidArgument("unknown category".to_owned()))
        })
        .collect()
}

impl From<PreviewError> for AppError {
    fn from(error: PreviewError) -> Self {
        match error {
            PreviewError::ScanNotFound => Self::ScanNotFound("requested scan".to_owned()),
            PreviewError::ScanAlreadyCleaned => Self::ScanAlreadyCleaned,
            PreviewError::InvalidArgument(reason) => Self::InvalidArgument(reason.to_owned()),
        }
    }
}

/// Arma y guarda un preview (sin tocar el disco; D-03). Consulta el trabajo activo pero no toma
/// su lugar.
///
/// - `recycle_bin_available`: `Trash::preflight` sobre la ruta del primer item (preview vacio =
///   `true`); si el mecanismo no se puede crear o el preflight falla, `false` (D-12 B).
pub fn create_preview<S, F, C, T>(
    ports: &CleanupPorts<'_, S, F, C>,
    privileges: &PrivilegeContext,
    make_trash: impl FnOnce() -> Result<T, TrashError>,
    scan_id: i64,
    categories: &[String],
) -> Result<CleanupPreviewDto, AppError>
where
    S: Store,
    F: FileSystem,
    C: Clock,
    T: Trash,
{
    safe_id(scan_id, "scan id")?;
    let requested = ScanId(scan_id);
    let categories = parse_categories(categories)?;
    validate_categories(&categories)?;
    ports.active.ensure_idle()?;

    let last_done = ports.store.last_done_junk_scan()?;
    let mut cleaned_at = None;
    let mut candidates = Vec::new();
    if let Some(last) = last_done.as_ref().filter(|last| last.scan.id == requested) {
        let since = last.scan.finished_at.unwrap_or(last.scan.started_at);
        cleaned_at = ports.store.scan_cleaned_at(requested, since)?;
        if cleaned_at.is_none() {
            candidates = ports.store.cleanup_candidates(requested, &categories)?;
        }
    }
    let preview = build_preview(PreviewInput {
        requested_scan: requested,
        last_done: last_done.as_ref(),
        cleaned_at,
        categories: &categories,
        candidates,
        privileges,
        now: ports.clock.now(),
        id: ports.previews.next_id(),
    })?;
    let available = recycle_bin_available(&preview, make_trash);
    let dto = CleanupPreviewDto::new(&preview, available);
    ports.previews.store(preview);
    Ok(dto)
}

fn recycle_bin_available<T: Trash>(
    preview: &CleanupPreview,
    make_trash: impl FnOnce() -> Result<T, TrashError>,
) -> bool {
    let Some(first) = preview.items().first() else {
        return true;
    };
    make_trash()
        .and_then(|trash| trash.preflight(std::path::Path::new(&first.path)))
        .is_ok()
}

/// Adapta el progreso del dominio a eventos del canal.
struct EventObserver<'a> {
    preview_id: u64,
    emit: &'a mut dyn FnMut(CleanupEventDto),
}

impl CleanupObserver for EventObserver<'_> {
    fn started(&mut self, total: u64) {
        (self.emit)(CleanupEventDto::Started {
            preview_id: self.preview_id,
            total,
        });
    }

    fn progress(&mut self, processed: u64, total: u64, moved_count: u64, moved_bytes: u64) {
        (self.emit)(CleanupEventDto::Progress {
            processed,
            total,
            moved_count,
            moved_bytes,
        });
    }
}

/// El preview solo vale mientras su escaneo siga siendo el ultimo `done` y no este limpiado.
fn ensure_fresh<S: Store>(store: &S, preview: &CleanupPreview) -> Result<(), AppError> {
    match store.last_done_junk_scan()? {
        Some(last) if last.scan.id == preview.scan_id() => {
            let since = last.scan.finished_at.unwrap_or(last.scan.started_at);
            match store.scan_cleaned_at(preview.scan_id(), since)? {
                None => Ok(()),
                Some(_) => Err(AppError::PreviewStale),
            }
        }
        _ => Err(AppError::PreviewStale),
    }
}

/// Confirma y ejecuta un preview (D-05): toma el lugar del trabajo activo, consume el preview,
/// reevalua privilegios y proteccion con el contexto de AHORA y mueve a la Papelera.
///
/// Un fallo por item nunca es un error: queda en el resumen. El preview se consume aunque la
/// ejecucion falle despues; el lugar se libera siempre.
pub fn confirm_cleanup_run<S, F, C, T>(
    ports: &CleanupPorts<'_, S, F, C>,
    privileges: &PrivilegeContext,
    raw: RawProtectionContext,
    make_trash: impl FnOnce() -> Result<T, TrashError>,
    preview_id: i64,
    emit: &mut dyn FnMut(CleanupEventDto),
) -> Result<CleanupSummaryDto, AppError>
where
    S: Store,
    F: FileSystem,
    C: Clock,
    T: Trash,
{
    let id = PreviewId(safe_id(preview_id, "preview id")?);
    let guard = ports.active.begin_cleanup(id)?;
    let preview = ports.previews.take(id, ports.clock.now())?;
    if preview.items().is_empty() {
        return Err(AppError::NothingToClean);
    }
    ensure_fresh(ports.store, &preview)?;
    let trash = make_trash()
        .map_err(|_| AppError::Internal("the recycle bin worker could not start".to_owned()))?;
    let protection = ProtectionContext::from_raw(restrict_protection_context(raw, privileges));
    let summary = run_cleanup(
        preview,
        privileges,
        protection,
        ports.fs,
        &trash,
        ports.store,
        ports.clock,
        guard.cancel_flag(),
        &mut EventObserver {
            preview_id: id.0,
            emit,
        },
    );
    Ok(CleanupSummaryDto::from(&summary))
}

/// Cancela la limpieza en curso del preview `preview_id`; `preview_not_found` si no corre.
pub fn cancel_cleanup_run(active: &ActiveJob, preview_id: i64) -> Result<(), AppError> {
    active.cancel_cleanup(PreviewId(safe_id(preview_id, "preview id")?))
}

// ---------------------------------------------------------------------------------------------
// Comandos IPC
// ---------------------------------------------------------------------------------------------

/// Comando IPC: arma el preview de las `categories` del escaneo `scan_id`.
#[tauri::command]
pub async fn preview_cleanup(
    scan_id: i64,
    categories: Vec<String>,
    handle: State<'_, StoreHandle>,
    previews: State<'_, CleanupPreviews>,
    active: State<'_, ActiveJob>,
    privileges: State<'_, PrivilegeContext>,
) -> Result<CleanupPreviewDto, AppError> {
    let handle = handle.inner().clone();
    let previews = previews.inner().clone();
    let active = active.inner().clone();
    let privileges = *privileges.inner();
    run_blocking(move || {
        let store = handle.store()?;
        let ports = CleanupPorts {
            store: &*store,
            fs: &WindowsFileSystem,
            clock: &SystemClock,
            previews: &previews,
            active: &active,
        };
        create_preview(&ports, &privileges, WindowsTrash::new, scan_id, &categories)
    })
    .await
}

/// Comando IPC: confirma el preview `preview_id`; el progreso viaja por `on_progress` y la
/// respuesta es el resumen. Mueve SOLO a la Papelera (nunca borrado permanente).
#[tauri::command]
pub async fn confirm_cleanup(
    preview_id: i64,
    on_progress: Channel<CleanupEventDto>,
    handle: State<'_, StoreHandle>,
    previews: State<'_, CleanupPreviews>,
    active: State<'_, ActiveJob>,
    privileges: State<'_, PrivilegeContext>,
    environment: State<'_, ScanEnvironment>,
) -> Result<CleanupSummaryDto, AppError> {
    let handle = handle.inner().clone();
    let previews = previews.inner().clone();
    let active = active.inner().clone();
    let privileges = *privileges.inner();
    let data_dir = environment.data_dir();
    run_blocking(move || {
        let store = handle.store()?;
        let exe = std::env::current_exe().ok();
        // El contexto de proteccion se resuelve de nuevo: nunca el del escaneo ni el del preview.
        let raw = resolve_protection_context(
            UserFolderSource::ProcessToken,
            app_dirs(data_dir.as_deref(), exe.as_deref()),
        );
        let ports = CleanupPorts {
            store: &*store,
            fs: &WindowsFileSystem,
            clock: &SystemClock,
            previews: &previews,
            active: &active,
        };
        // Un error de envio se ignora: la UI pudo cerrarse y la limpieza igual termina.
        confirm_cleanup_run(
            &ports,
            &privileges,
            raw,
            WindowsTrash::new,
            preview_id,
            &mut |event| {
                let _ = on_progress.send(event);
            },
        )
    })
    .await
}

/// Comando IPC: cancela la limpieza en curso del preview `preview_id`.
#[tauri::command]
pub async fn cancel_cleanup(preview_id: i64, active: State<'_, ActiveJob>) -> Result<(), AppError> {
    cancel_cleanup_run(active.inner(), preview_id)
}

#[cfg(test)]
#[path = "cleanup_tests.rs"]
mod tests;

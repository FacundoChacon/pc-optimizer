//! Comandos de duplicados (C-11): `start_duplicate_scan`, `get_last_duplicate_scan`,
//! `list_duplicate_groups`, `preview_duplicate_cleanup` y `confirm_duplicate_cleanup`.
//!
//! Los comandos son finos: validan argumentos, arman las dependencias reales y delegan en
//! funciones genericas que se prueban con dobles. La interfaz solo envia CLAVES de carpeta, un
//! tamano minimo de la lista cerrada e identificadores, NUNCA rutas. Solo `list_duplicate_groups`
//! devuelve rutas; los resumenes de ejecucion no llevan rutas, SIDs ni nombres de cuenta. El
//! escaneo nunca mueve ni borra: el borrado pasa por `run_cleanup` (solo Papelera).

use std::collections::{BTreeSet, HashMap};

use serde::{Deserialize, Serialize};
use tauri::State;
use tauri::ipc::Channel;

use super::cleanup::{
    CleanupEventDto, CleanupPreviewDto, CleanupPreviews, CleanupSummaryDto, recycle_bin_available,
};
use super::run_blocking;
use super::scan::{ActiveJob, JobGuard, ScanEnvironment, app_dirs};
use crate::adapters::clock::SystemClock;
use crate::adapters::sqlite::StoreHandle;
use crate::adapters::windows::trash::WindowsTrash;
use crate::adapters::windows::{
    UserFolderSource, WindowsContentReader, WindowsFileSystem, resolve_protection_context,
};
use crate::domain::cleanup::execute::{CleanupObserver, run_cleanup};
use crate::domain::cleanup::model::PreviewId;
use crate::domain::duplicates::decision::{Decision, MAX_SAFE_ID};
use crate::domain::duplicates::guard::DuplicateContentGuard;
use crate::domain::duplicates::model::{
    DuplicateGroupPage, DuplicateScanOutcome, DuplicateScanRequest, DuplicateScanSummary, MinSize,
    RequestError, RootResult, StoredDuplicateGroup,
};
use crate::domain::duplicates::preview::{
    DuplicatePreviewError, DuplicatePreviewInput, build_duplicate_preview,
};
use crate::domain::duplicates::scanner::{DuplicateScanObserver, scan_duplicates};
use crate::domain::model::{NewScan, ScanId, ScanKind, ScanStatus, ScanTrigger};
use crate::domain::ports::{Clock, ContentReader, FileSystem, Store, Trash, TrashError};
use crate::domain::privileges::{
    PrivilegeContext, ScopeAccess, UserScope, restrict_protection_context, scope_access,
};
use crate::domain::protection::context::{ContentFolder, ProtectionContext, RawProtectionContext};
use crate::domain::protection::path::{NormalizedPath, normalize_lexical};
use crate::error::AppError;

/// Tamano maximo de una pagina de grupos.
pub const MAX_PAGE_SIZE: u32 = 200;

// ---------------------------------------------------------------------------------------------
// DTOs (camelCase hacia el frontend)
// ---------------------------------------------------------------------------------------------

/// Estado de una carpeta de contenido en el escaneo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RootDto {
    pub root: &'static str,
    /// `done`, `not_selected`, `nested` o `unavailable`.
    pub status: &'static str,
    /// Solo con `unavailable`.
    pub reason: Option<&'static str>,
    pub files_seen: u64,
}

impl From<&RootResult> for RootDto {
    fn from(root: &RootResult) -> Self {
        Self {
            root: root.folder.as_str(),
            status: root.status.as_str(),
            reason: root.status.reason().map(|reason| reason.as_str()),
            files_seen: root.files_seen,
        }
    }
}

/// Resumen de un escaneo de duplicados (`done` o `cancelled`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateScanSummaryDto {
    pub scan_id: i64,
    pub status: &'static str,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub min_size_bytes: u64,
    pub roots: Vec<RootDto>,
    pub files_seen: u64,
    pub below_min_size: u64,
    pub protected_count: u64,
    pub cloud_count: u64,
    pub hard_linked_count: u64,
    pub changed_count: u64,
    pub unreadable_count: u64,
    pub inaccessible_dirs: u64,
    pub excluded_dirs: u64,
    pub hashed_bytes: u64,
    pub group_count: u64,
    pub duplicate_file_count: u64,
    /// Bytes que aun se pueden recuperar (grupos sin resolver).
    pub reclaimable_bytes: u64,
    pub open_group_count: u64,
}

impl From<&DuplicateScanSummary> for DuplicateScanSummaryDto {
    fn from(summary: &DuplicateScanSummary) -> Self {
        let c = &summary.counters;
        Self {
            scan_id: summary.scan.id.0,
            status: summary.scan.status.as_str(),
            started_at: summary.scan.started_at.0,
            finished_at: summary.scan.finished_at.map(|at| at.0),
            min_size_bytes: summary.min_size.bytes(),
            roots: summary.roots.iter().map(RootDto::from).collect(),
            files_seen: c.files_seen,
            below_min_size: c.below_min_size,
            protected_count: c.protected_count,
            cloud_count: c.cloud_count,
            hard_linked_count: c.hard_linked_count,
            changed_count: c.changed_count,
            unreadable_count: c.unreadable_count,
            inaccessible_dirs: c.inaccessible_dirs,
            excluded_dirs: c.excluded_dirs,
            hashed_bytes: c.hashed_bytes,
            group_count: summary.group_count,
            duplicate_file_count: summary.duplicate_file_count,
            reclaimable_bytes: summary.reclaimable_bytes,
            open_group_count: summary.open_group_count,
        }
    }
}

/// Evento de progreso del escaneo enviado por el canal de la invocacion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum DuplicateScanEventDto {
    #[serde(rename_all = "camelCase")]
    Started { scan_id: i64 },
    #[serde(rename_all = "camelCase")]
    Walking { files_seen: u64 },
    #[serde(rename_all = "camelCase")]
    Partial { done: u64, total: u64 },
    #[serde(rename_all = "camelCase")]
    Full { done_bytes: u64, total_bytes: u64 },
}

/// Una copia de un grupo (unico DTO de duplicados que lleva rutas).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateFileDto {
    pub file_id: i64,
    pub root: &'static str,
    pub path: String,
    /// Ruta relativa a la carpeta de contenido (la ruta completa si no se puede calcular).
    pub relative_path: String,
    pub modified_at: Option<i64>,
    pub kept: bool,
}

/// Un grupo de copias identicas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateGroupDto {
    pub group_id: i64,
    pub size_bytes: u64,
    pub blake3: String,
    pub reclaimable_bytes: u64,
    pub resolved_at: Option<i64>,
    pub files: Vec<DuplicateFileDto>,
}

/// Pagina de grupos.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateGroupPageDto {
    pub scan_id: i64,
    pub total: u64,
    pub groups: Vec<DuplicateGroupDto>,
}

/// Rutas de las carpetas de contenido segun el contexto de ahora (solo para mostrar la ruta
/// relativa).
pub type FolderPaths = HashMap<ContentFolder, NormalizedPath>;

fn relative_path(path: &str, root: Option<&NormalizedPath>) -> String {
    normalize_lexical(path)
        .ok()
        .and_then(|normalized| {
            let root = root?;
            normalized.strip_prefix(root).map(|components| {
                components
                    .iter()
                    .map(|component| component.original())
                    .collect::<Vec<_>>()
                    .join("\\")
            })
        })
        .unwrap_or_else(|| path.to_owned())
}

fn group_dto(group: StoredDuplicateGroup, folders: &FolderPaths) -> DuplicateGroupDto {
    DuplicateGroupDto {
        group_id: group.id,
        size_bytes: group.size_bytes,
        reclaimable_bytes: group.reclaimable_bytes(),
        blake3: group.blake3,
        resolved_at: group.resolved_at.map(|at| at.0),
        files: group
            .files
            .into_iter()
            .map(|stored| DuplicateFileDto {
                file_id: stored.id,
                root: stored.file.root.as_str(),
                relative_path: relative_path(&stored.file.path, folders.get(&stored.file.root)),
                path: stored.file.path,
                modified_at: stored.file.modified.map(|at| at.0),
                kept: stored.kept,
            })
            .collect(),
    }
}

fn page_dto(
    scan_id: i64,
    page: DuplicateGroupPage,
    folders: &FolderPaths,
) -> DuplicateGroupPageDto {
    DuplicateGroupPageDto {
        scan_id,
        total: page.total,
        groups: page
            .groups
            .into_iter()
            .map(|group| group_dto(group, folders))
            .collect(),
    }
}

/// Preview de duplicados: el de la limpieza estandar mas la cantidad de grupos.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateCleanupPreviewDto {
    #[serde(flatten)]
    pub preview: CleanupPreviewDto,
    pub group_count: u64,
}

/// Resumen de la ejecucion: el de la limpieza estandar mas la resolucion de los grupos.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateCleanupSummaryDto {
    #[serde(flatten)]
    pub summary: CleanupSummaryDto,
    /// `false` si no se pudo guardar la resolucion: los grupos siguen abiertos.
    pub groups_resolved: bool,
    pub resolved_group_ids: Vec<i64>,
}

/// Decision de un grupo tal como llega por IPC: copias a conservar.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionArg {
    pub group_id: i64,
    pub keep_file_ids: Vec<i64>,
}

// ---------------------------------------------------------------------------------------------
// Logica de los comandos (generica, sin Tauri)
// ---------------------------------------------------------------------------------------------

/// Dependencias de los comandos de duplicados.
pub struct DuplicatePorts<'a, S: ?Sized, F, R, C> {
    pub store: &'a S,
    pub fs: &'a F,
    pub reader: &'a R,
    pub clock: &'a C,
    pub previews: &'a CleanupPreviews,
    pub active: &'a ActiveJob,
}

impl From<RequestError> for AppError {
    fn from(error: RequestError) -> Self {
        match error {
            RequestError::InvalidArgument(reason) => Self::InvalidArgument(reason.to_owned()),
        }
    }
}

impl From<DuplicatePreviewError> for AppError {
    fn from(error: DuplicatePreviewError) -> Self {
        match error {
            DuplicatePreviewError::ScanNotFound => Self::ScanNotFound("requested scan".to_owned()),
            DuplicatePreviewError::InvalidArgument(reason) => {
                Self::InvalidArgument(reason.to_owned())
            }
            DuplicatePreviewError::GroupNotFound => Self::DuplicateGroupNotFound,
            DuplicatePreviewError::GroupResolved => Self::DuplicateGroupResolved,
            DuplicatePreviewError::KeepRequired => Self::DuplicateKeepRequired,
        }
    }
}

/// Identificador que viaja como entero JS: de 0 a 2^53 - 1.
fn safe_id(raw: i64, what: &str) -> Result<i64, AppError> {
    if (0..=MAX_SAFE_ID).contains(&raw) {
        Ok(raw)
    } else {
        Err(AppError::InvalidArgument(format!("{what} is out of range")))
    }
}

/// Las carpetas de contenido son las del usuario del token: con otra cuenta (mismatch) ni se
/// escanea ni se previsualiza.
fn ensure_not_blocked(privileges: &PrivilegeContext) -> Result<(), AppError> {
    match scope_access(privileges, UserScope::PerUser) {
        ScopeAccess::Allowed => Ok(()),
        ScopeAccess::Blocked(reason) => Err(AppError::DuplicatesBlocked(reason)),
    }
}

/// Adapta el progreso del escaneo a eventos del canal.
struct ScanEvents<'a> {
    emit: &'a mut dyn FnMut(DuplicateScanEventDto),
}

impl DuplicateScanObserver for ScanEvents<'_> {
    fn walking(&mut self, files_seen: u64) {
        (self.emit)(DuplicateScanEventDto::Walking { files_seen });
    }

    fn partial(&mut self, done: u64, total: u64) {
        (self.emit)(DuplicateScanEventDto::Partial { done, total });
    }

    fn full(&mut self, done_bytes: u64, total_bytes: u64) {
        (self.emit)(DuplicateScanEventDto::Full {
            done_bytes,
            total_bytes,
        });
    }
}

/// Corre un escaneo de duplicados completo: toma el lugar del trabajo activo, lo crea, detecta,
/// persiste y devuelve el resumen.
///
/// - Argumentos invalidos (`invalid_argument`) y mismatch (`duplicates_blocked`) fallan antes de
///   crear el escaneo.
/// - Cancelado: el escaneo queda `cancelled` y no se guarda ningun grupo.
/// - Fallo al guardar: el escaneo se cierra como `error` y el fallo se propaga.
pub fn run_duplicate_scan<S, F, R, C>(
    ports: &DuplicatePorts<'_, S, F, R, C>,
    privileges: &PrivilegeContext,
    raw: RawProtectionContext,
    roots: Option<Vec<String>>,
    min_size_bytes: Option<u64>,
    emit: &mut dyn FnMut(DuplicateScanEventDto),
) -> Result<DuplicateScanSummaryDto, AppError>
where
    S: Store + ?Sized,
    F: FileSystem,
    R: ContentReader,
    C: Clock,
{
    let guard = ports.active.begin_scan()?;
    let request = DuplicateScanRequest::new(roots, min_size_bytes)?;
    ensure_not_blocked(privileges)?;
    run_scan_with_guard(ports, privileges, raw, &request, &guard, emit)
}

fn run_scan_with_guard<S, F, R, C>(
    ports: &DuplicatePorts<'_, S, F, R, C>,
    privileges: &PrivilegeContext,
    raw: RawProtectionContext,
    request: &DuplicateScanRequest,
    guard: &JobGuard,
    emit: &mut dyn FnMut(DuplicateScanEventDto),
) -> Result<DuplicateScanSummaryDto, AppError>
where
    S: Store + ?Sized,
    F: FileSystem,
    R: ContentReader,
    C: Clock,
{
    let store = ports.store;
    let started_at = ports.clock.now();
    let scan_id = store.create_scan(&NewScan {
        kind: ScanKind::Duplicates,
        trigger: ScanTrigger::Manual,
        started_at,
    })?;
    guard.bind(scan_id);
    emit(DuplicateScanEventDto::Started { scan_id: scan_id.0 });

    let protection = ProtectionContext::from_raw(restrict_protection_context(raw, privileges));
    let outcome = scan_duplicates(
        request,
        protection,
        ports.fs,
        ports.reader,
        guard.cancel_flag(),
        &mut ScanEvents { emit },
    );
    match outcome {
        DuplicateScanOutcome::Completed(results) => {
            if let Err(error) = store.complete_duplicate_scan(scan_id, &results, ports.clock.now())
            {
                // El escaneo sigue `running`: se cierra como error (si tambien falla, la
                // apertura de la base lo cierra).
                let _ = store.finish_scan(scan_id, ScanStatus::Error, ports.clock.now());
                return Err(error.into());
            }
            match store.last_done_duplicate_scan()? {
                Some(summary) if summary.scan.id == scan_id => Ok((&summary).into()),
                _ => Err(AppError::Internal(
                    "the finished scan is missing from the store".to_owned(),
                )),
            }
        }
        DuplicateScanOutcome::Cancelled => {
            let finished_at = ports.clock.now();
            store.finish_scan(scan_id, ScanStatus::Cancelled, finished_at)?;
            Ok(DuplicateScanSummaryDto {
                scan_id: scan_id.0,
                status: ScanStatus::Cancelled.as_str(),
                started_at: started_at.0,
                finished_at: Some(finished_at.0),
                min_size_bytes: request.min_size().bytes(),
                roots: Vec::new(),
                files_seen: 0,
                below_min_size: 0,
                protected_count: 0,
                cloud_count: 0,
                hard_linked_count: 0,
                changed_count: 0,
                unreadable_count: 0,
                inaccessible_dirs: 0,
                excluded_dirs: 0,
                hashed_bytes: 0,
                group_count: 0,
                duplicate_file_count: 0,
                reclaimable_bytes: 0,
                open_group_count: 0,
            })
        }
    }
}

/// Resumen del ultimo escaneo de duplicados terminado, si existe.
pub fn last_duplicate_scan<S: Store + ?Sized>(
    store: &S,
) -> Result<Option<DuplicateScanSummaryDto>, AppError> {
    Ok(store
        .last_done_duplicate_scan()?
        .as_ref()
        .map(DuplicateScanSummaryDto::from))
}

/// Una pagina de grupos del ultimo escaneo terminado (valida todos los argumentos).
pub fn list_groups<S: Store + ?Sized>(
    store: &S,
    scan_id: i64,
    offset: i64,
    limit: i64,
    root: Option<&str>,
    min_size_bytes: Option<u64>,
    folders: &FolderPaths,
) -> Result<DuplicateGroupPageDto, AppError> {
    safe_id(scan_id, "scan id")?;
    let offset = u64::try_from(offset)
        .map_err(|_| AppError::InvalidArgument("offset must be 0 or more".to_owned()))?;
    let limit = u32::try_from(limit)
        .ok()
        .filter(|limit| (1..=MAX_PAGE_SIZE).contains(limit))
        .ok_or_else(|| {
            AppError::InvalidArgument(format!("limit must be between 1 and {MAX_PAGE_SIZE}"))
        })?;
    let root = root
        .map(|key| {
            ContentFolder::parse(key)
                .ok_or_else(|| AppError::InvalidArgument("unknown folder key".to_owned()))
        })
        .transpose()?;
    let min_size = min_size_bytes
        .map(|bytes| {
            MinSize::new(bytes).map(MinSize::bytes).ok_or_else(|| {
                AppError::InvalidArgument("min size is not an allowed option".to_owned())
            })
        })
        .transpose()?;
    match store.last_done_duplicate_scan()? {
        Some(summary) if summary.scan.id.0 == scan_id => {}
        _ => return Err(AppError::ScanNotFound(scan_id.to_string())),
    }
    let page = store.list_duplicate_groups(ScanId(scan_id), offset, limit, root, min_size)?;
    Ok(page_dto(scan_id, page, folders))
}

/// Arma y guarda un preview de duplicados (sin tocar el disco ni abrir contenido). Consulta el
/// trabajo activo pero no toma su lugar.
pub fn create_duplicate_preview<S, F, R, C, T>(
    ports: &DuplicatePorts<'_, S, F, R, C>,
    privileges: &PrivilegeContext,
    make_trash: impl FnOnce() -> Result<T, TrashError>,
    scan_id: i64,
    decisions: &[DecisionArg],
) -> Result<DuplicateCleanupPreviewDto, AppError>
where
    S: Store + ?Sized,
    F: FileSystem,
    R: ContentReader,
    C: Clock,
    T: Trash,
{
    safe_id(scan_id, "scan id")?;
    for decision in decisions {
        safe_id(decision.group_id, "group id")?;
        for id in &decision.keep_file_ids {
            safe_id(*id, "file id")?;
        }
    }
    ports.active.ensure_idle()?;
    ensure_not_blocked(privileges)?;

    let requested = ScanId(scan_id);
    let last_done = ports.store.last_done_duplicate_scan()?;
    let group_ids: Vec<i64> = decisions.iter().map(|d| d.group_id).collect();
    let groups = match last_done.as_ref().filter(|last| last.scan.id == requested) {
        Some(_) => ports.store.get_duplicate_groups(requested, &group_ids)?,
        None => Vec::new(),
    };
    let decisions: Vec<Decision> = decisions
        .iter()
        .map(|d| Decision {
            group_id: d.group_id,
            keep_file_ids: d.keep_file_ids.clone(),
        })
        .collect();
    let preview = build_duplicate_preview(DuplicatePreviewInput {
        requested_scan: requested,
        last_done: last_done.as_ref(),
        decisions: &decisions,
        groups: &groups,
        now: ports.clock.now(),
        id: ports.previews.next_id(),
    })?;
    let available = recycle_bin_available(preview.preview(), make_trash);
    let dto = DuplicateCleanupPreviewDto {
        preview: CleanupPreviewDto::new(preview.preview(), available),
        group_count: preview.group_count(),
    };
    ports.previews.store_duplicates(preview);
    Ok(dto)
}

/// Adapta el progreso de la ejecucion a eventos del canal y recuerda que items se procesaron.
struct RunObserver<'a> {
    preview_id: u64,
    emit: &'a mut dyn FnMut(CleanupEventDto),
    processed: BTreeSet<i64>,
}

impl CleanupObserver for RunObserver<'_> {
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

    fn item_processed(&mut self, finding_id: i64) {
        self.processed.insert(finding_id);
    }
}

/// Cada cuantos items se avisa el progreso de la ejecucion de duplicados: pocos items grandes.
const PROGRESS_EVERY_COPIES: u64 = 1;

/// Confirma y ejecuta un preview de duplicados: toma el lugar del trabajo activo, consume el
/// preview, reevalua privilegios y proteccion con el contexto de AHORA, mueve a la Papelera con la
/// re-verificacion de contenido y resuelve los grupos procesados (D-17).
pub fn confirm_duplicate_cleanup_run<S, F, R, C, T>(
    ports: &DuplicatePorts<'_, S, F, R, C>,
    privileges: &PrivilegeContext,
    raw: RawProtectionContext,
    make_trash: impl FnOnce() -> Result<T, TrashError>,
    preview_id: i64,
    emit: &mut dyn FnMut(CleanupEventDto),
) -> Result<DuplicateCleanupSummaryDto, AppError>
where
    S: Store,
    F: FileSystem,
    R: ContentReader,
    C: Clock,
    T: Trash,
{
    let id = PreviewId(u64::try_from(safe_id(preview_id, "preview id")?).unwrap_or(0));
    let job = ports.active.begin_cleanup(id)?;
    let preview = ports.previews.take_duplicates(id, ports.clock.now())?;
    if preview.preview().items().is_empty() {
        return Err(AppError::NothingToClean);
    }
    match ports.store.last_done_duplicate_scan()? {
        Some(last) if last.scan.id == preview.preview().scan_id() => {}
        _ => return Err(AppError::PreviewStale),
    }
    let trash = make_trash()
        .map_err(|_| AppError::Internal("the recycle bin worker could not start".to_owned()))?;
    let protection = ProtectionContext::from_raw(restrict_protection_context(raw, privileges));
    let scan_id = preview.preview().scan_id();
    let (core, plan) = preview.into_parts();
    let mut content_guard = DuplicateContentGuard::new(ports.reader, &plan);
    let mut observer = RunObserver {
        preview_id: id.0,
        emit,
        processed: BTreeSet::new(),
    };
    let summary = run_cleanup(
        core,
        privileges,
        protection,
        ports.fs,
        &trash,
        ports.store,
        ports.clock,
        job.cancel_flag(),
        &mut observer,
        &mut content_guard,
        PROGRESS_EVERY_COPIES,
    );
    let (resolutions, resolved_ids) = plan.resolutions_for(&observer.processed);
    let (groups_resolved, resolved_group_ids) = if resolutions.is_empty() {
        (true, Vec::new())
    } else {
        match ports
            .store
            .resolve_duplicate_groups(scan_id, &resolutions, ports.clock.now())
        {
            Ok(()) => (true, resolved_ids),
            Err(_) => (false, Vec::new()),
        }
    };
    Ok(DuplicateCleanupSummaryDto {
        summary: CleanupSummaryDto::from(&summary),
        groups_resolved,
        resolved_group_ids,
    })
}

// ---------------------------------------------------------------------------------------------
// Comandos IPC
// ---------------------------------------------------------------------------------------------

/// Rutas de las carpetas de contenido del contexto actual (para mostrar rutas relativas).
fn folder_paths(raw: RawProtectionContext) -> FolderPaths {
    let mut folders = FolderPaths::new();
    if let Ok(context) = ProtectionContext::from_raw(raw)
        && let Some(user) = context.user()
    {
        for folder in ContentFolder::ALL {
            if let Some(path) = user.content_folder(folder) {
                folders.insert(folder, path.clone());
            }
        }
    }
    folders
}

/// Comando IPC: inicia un escaneo de duplicados; el progreso viaja por `on_progress` y la
/// respuesta es el resumen final (`done` o `cancelled`).
#[tauri::command]
#[expect(
    clippy::too_many_arguments,
    reason = "los argumentos de un comando Tauri son el payload mas los State que inyecta el framework"
)]
pub async fn start_duplicate_scan(
    roots: Option<Vec<String>>,
    min_size_bytes: Option<u64>,
    on_progress: Channel<DuplicateScanEventDto>,
    handle: State<'_, StoreHandle>,
    previews: State<'_, CleanupPreviews>,
    active: State<'_, ActiveJob>,
    privileges: State<'_, PrivilegeContext>,
    environment: State<'_, ScanEnvironment>,
) -> Result<DuplicateScanSummaryDto, AppError> {
    let handle = handle.inner().clone();
    let previews = previews.inner().clone();
    let active = active.inner().clone();
    let privileges = *privileges.inner();
    let data_dir = environment.data_dir();
    run_blocking(move || {
        let store = handle.store()?;
        let exe = std::env::current_exe().ok();
        let raw = resolve_protection_context(
            UserFolderSource::ProcessToken,
            app_dirs(data_dir.as_deref(), exe.as_deref()),
        );
        let ports = DuplicatePorts {
            store: &*store,
            fs: &WindowsFileSystem,
            reader: &WindowsContentReader,
            clock: &SystemClock,
            previews: &previews,
            active: &active,
        };
        // Un error de envio se ignora: la UI pudo cerrarse y el escaneo igual se completa.
        run_duplicate_scan(
            &ports,
            &privileges,
            raw,
            roots,
            min_size_bytes,
            &mut |event| {
                let _ = on_progress.send(event);
            },
        )
    })
    .await
}

/// Comando IPC: resumen del ultimo escaneo de duplicados terminado (`null` si no hay).
#[tauri::command]
pub async fn get_last_duplicate_scan(
    handle: State<'_, StoreHandle>,
) -> Result<Option<DuplicateScanSummaryDto>, AppError> {
    let handle = handle.inner().clone();
    run_blocking(move || last_duplicate_scan(&*handle.store()?)).await
}

/// Comando IPC: pagina de grupos del ultimo escaneo terminado, con filtros opcionales.
#[tauri::command]
pub async fn list_duplicate_groups(
    scan_id: i64,
    offset: i64,
    limit: i64,
    root: Option<String>,
    min_size_bytes: Option<u64>,
    handle: State<'_, StoreHandle>,
    environment: State<'_, ScanEnvironment>,
) -> Result<DuplicateGroupPageDto, AppError> {
    let handle = handle.inner().clone();
    let data_dir = environment.data_dir();
    run_blocking(move || {
        let raw = resolve_protection_context(
            UserFolderSource::ProcessToken,
            app_dirs(data_dir.as_deref(), None),
        );
        list_groups(
            &*handle.store()?,
            scan_id,
            offset,
            limit,
            root.as_deref(),
            min_size_bytes,
            &folder_paths(raw),
        )
    })
    .await
}

/// Comando IPC: arma el preview de las copias a eliminar segun las `decisions`.
#[tauri::command]
pub async fn preview_duplicate_cleanup(
    scan_id: i64,
    decisions: Vec<DecisionArg>,
    handle: State<'_, StoreHandle>,
    previews: State<'_, CleanupPreviews>,
    active: State<'_, ActiveJob>,
    privileges: State<'_, PrivilegeContext>,
) -> Result<DuplicateCleanupPreviewDto, AppError> {
    let handle = handle.inner().clone();
    let previews = previews.inner().clone();
    let active = active.inner().clone();
    let privileges = *privileges.inner();
    run_blocking(move || {
        let store = handle.store()?;
        let ports = DuplicatePorts {
            store: &*store,
            fs: &WindowsFileSystem,
            reader: &WindowsContentReader,
            clock: &SystemClock,
            previews: &previews,
            active: &active,
        };
        create_duplicate_preview(&ports, &privileges, WindowsTrash::new, scan_id, &decisions)
    })
    .await
}

/// Comando IPC: confirma el preview `preview_id`; el progreso viaja por `on_progress` y la
/// respuesta es el resumen. Mueve SOLO a la Papelera (nunca borrado permanente).
#[tauri::command]
pub async fn confirm_duplicate_cleanup(
    preview_id: i64,
    on_progress: Channel<CleanupEventDto>,
    handle: State<'_, StoreHandle>,
    previews: State<'_, CleanupPreviews>,
    active: State<'_, ActiveJob>,
    privileges: State<'_, PrivilegeContext>,
    environment: State<'_, ScanEnvironment>,
) -> Result<DuplicateCleanupSummaryDto, AppError> {
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
        let ports = DuplicatePorts {
            store: &*store,
            fs: &WindowsFileSystem,
            reader: &WindowsContentReader,
            clock: &SystemClock,
            previews: &previews,
            active: &active,
        };
        confirm_duplicate_cleanup_run(
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

#[cfg(test)]
#[path = "duplicates_tests.rs"]
mod tests;

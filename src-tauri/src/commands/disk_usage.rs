//! Comandos del analisis de espacio en disco (C-07): `start_folder_scan`, `get_last_folder_scan`,
//! `list_folder_children`, `list_large_files`, `pick_folder_root` y `reveal_in_explorer`.
//!
//! Los comandos son finos: validan argumentos, arman las dependencias reales y delegan en
//! funciones genericas que se prueban con dobles. NINGUNA ruta viaja de la interfaz al backend:
//! la raiz llega por tipo o por el identificador de la ultima seleccion, y cada elemento por su
//! id del ultimo analisis. El analisis nunca abre, mueve ni borra archivos; "Mostrar en el
//! Explorador" selecciona el elemento sin ejecutarlo.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::State;
use tauri::ipc::Channel;

use super::run_blocking;
use super::scan::{ActiveJob, JobGuard, ScanEnvironment, app_dirs, lock};
use crate::adapters::clock::SystemClock;
use crate::adapters::sqlite::StoreHandle;
use crate::adapters::windows::{
    UserFolderSource, WindowsExplorer, WindowsFileSystem, WindowsFolderPicker, WindowsVolumeSpace,
    resolve_protection_context,
};
use crate::domain::disk_usage::model::{
    FolderPage, FolderRow, FolderScanSummary, FolderSort, LargeFilePage, LargeFileRow,
    LargeFileSort, PickId, RevealReason, RootKind, RootRequest, unaccounted,
};
use crate::domain::disk_usage::root::{FolderScanError, RootChoice, rejection_code, validate_root};
use crate::domain::disk_usage::scanner::{FolderScanObserver, FolderScanOutcome, scan_folders};
use crate::domain::model::{NewScan, ScanId, ScanKind, ScanStatus, ScanTrigger};
use crate::domain::ports::{
    Clock, Explorer, FileSystem, FolderPicker, Store, StoreError, VolumeSpace,
};
use crate::domain::privileges::{
    MismatchReason, PrivilegeContext, ScopeAccess, UserScope, restrict_protection_context,
    scope_access,
};
use crate::domain::protection::context::{ProtectionContext, RawProtectionContext};
use crate::domain::protection::path::{NormalizedPath, normalize_lexical};
use crate::error::AppError;

/// Tamano maximo de una pagina.
pub const MAX_PAGE_SIZE: u32 = 200;
/// Mayor entero que JavaScript representa sin perdida.
const MAX_SAFE_ID: i64 = (1 << 53) - 1;

// ---------------------------------------------------------------------------------------------
// Argumentos y DTOs (camelCase hacia el frontend)
// ---------------------------------------------------------------------------------------------

/// Raiz pedida tal como llega por IPC: `{ kind, pickId? }`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RootArg {
    pub kind: String,
    pub pick_id: Option<i64>,
}

/// Elemento a mostrar en el Explorador: `{ kind: "folder" | "file", id }`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RevealTargetArg {
    pub kind: String,
    pub id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderRootDto {
    pub kind: &'static str,
    pub path: String,
    pub folder_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeDto {
    pub total_bytes: u64,
    pub free_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueDto {
    pub path: String,
    pub reason: &'static str,
}

/// Resumen de un analisis (`done` o `cancelled`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderScanSummaryDto {
    pub scan_id: i64,
    pub status: &'static str,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub root: FolderRootDto,
    pub volume: Option<VolumeDto>,
    pub unaccounted_bytes: Option<u64>,
    pub over_counted: bool,
    pub total_bytes: u64,
    pub file_count: u64,
    pub dir_count: u64,
    pub inaccessible_dirs: u64,
    pub reparse_skipped: u64,
    pub cloud_files: u64,
    pub opaque_dirs: u64,
    pub folders_stored: u64,
    pub folders_truncated: bool,
    pub issues: Vec<IssueDto>,
}

impl From<&FolderScanSummary> for FolderScanSummaryDto {
    fn from(summary: &FolderScanSummary) -> Self {
        let c = &summary.counters;
        let missing = unaccounted(summary.root_kind, summary.volume, c.total_bytes);
        Self {
            scan_id: summary.scan.id.0,
            status: summary.scan.status.as_str(),
            started_at: summary.scan.started_at.0,
            finished_at: summary.scan.finished_at.map(|at| at.0),
            root: FolderRootDto {
                kind: summary.root_kind.as_str(),
                path: summary.root_path.clone(),
                folder_id: summary.root_folder_id,
            },
            volume: summary.volume.map(|volume| VolumeDto {
                total_bytes: volume.total_bytes,
                free_bytes: volume.free_bytes,
            }),
            unaccounted_bytes: missing.bytes,
            over_counted: missing.over_counted,
            total_bytes: c.total_bytes,
            file_count: c.file_count,
            dir_count: c.dir_count,
            inaccessible_dirs: c.inaccessible_dirs,
            reparse_skipped: c.reparse_skipped,
            cloud_files: c.cloud_files,
            opaque_dirs: c.opaque_dirs,
            folders_stored: c.folders_stored,
            folders_truncated: summary.folders_truncated,
            issues: summary
                .issues
                .iter()
                .map(|issue| IssueDto {
                    path: issue.path.clone(),
                    reason: issue.reason.as_str(),
                })
                .collect(),
        }
    }
}

/// Evento de progreso: ninguno lleva rutas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum FolderScanEventDto {
    #[serde(rename_all = "camelCase")]
    Started {
        scan_id: i64,
        root_kind: &'static str,
    },
    #[serde(rename_all = "camelCase")]
    Walking {
        files_seen: u64,
        dirs_seen: u64,
        bytes_seen: u64,
    },
    Saving,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderDto {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
    pub direct_bytes: u64,
    pub file_count: u64,
    pub dir_count: u64,
    pub access: &'static str,
    pub opaque: Option<&'static str>,
    pub system: bool,
    pub junk_hint: bool,
    pub has_children: bool,
}

impl From<&FolderRow> for FolderDto {
    fn from(row: &FolderRow) -> Self {
        Self {
            id: row.id,
            name: row.name(),
            path: row.path.clone(),
            size_bytes: row.size_bytes,
            direct_bytes: row.direct_bytes,
            file_count: row.file_count,
            dir_count: row.dir_count,
            access: row.access.as_str(),
            opaque: row.opaque.map(|reason| reason.as_str()),
            system: row.system,
            junk_hint: row.junk_hint,
            has_children: row.has_children,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CrumbDto {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderPageDto {
    pub scan_id: i64,
    pub folder: FolderDto,
    pub breadcrumb: Vec<CrumbDto>,
    pub total: u64,
    pub items: Vec<FolderDto>,
    pub remainder_bytes: u64,
}

impl FolderPageDto {
    fn new(scan_id: i64, page: &FolderPage) -> Self {
        Self {
            scan_id,
            folder: FolderDto::from(&page.folder),
            breadcrumb: page
                .breadcrumb
                .iter()
                .map(|crumb| CrumbDto {
                    id: crumb.id,
                    name: crumb.name.clone(),
                })
                .collect(),
            total: page.total,
            items: page.items.iter().map(FolderDto::from).collect(),
            remainder_bytes: page.remainder_bytes,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LargeFileDto {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub folder_id: i64,
    pub size_bytes: u64,
    pub modified_at: Option<i64>,
}

impl From<&LargeFileRow> for LargeFileDto {
    fn from(row: &LargeFileRow) -> Self {
        Self {
            id: row.id,
            name: row
                .path
                .rsplit('\\')
                .next()
                .unwrap_or(&row.path)
                .to_string(),
            path: row.path.clone(),
            folder_id: row.folder_id,
            size_bytes: row.size_bytes,
            modified_at: row.modified_at.map(|at| at.0),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LargeFilePageDto {
    pub scan_id: i64,
    pub total: u64,
    pub items: Vec<LargeFileDto>,
}

impl LargeFilePageDto {
    fn new(scan_id: i64, page: &LargeFilePage) -> Self {
        Self {
            scan_id,
            total: page.total,
            items: page.items.iter().map(LargeFileDto::from).collect(),
        }
    }
}

/// Respuesta de `pick_folder_root`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum PickResultDto {
    #[serde(rename_all = "camelCase")]
    Picked {
        pick_id: i64,
        path: String,
    },
    Cancelled,
}

// ---------------------------------------------------------------------------------------------
// Estado: ultima seleccion y compuerta del selector
// ---------------------------------------------------------------------------------------------

/// La ultima carpeta elegida (una sola viva): elegir otra reemplaza a la anterior. La interfaz
/// solo conoce su identificador.
#[derive(Debug, Clone, Default)]
pub struct PickedRootSlot(Arc<Mutex<PickedState>>);

#[derive(Debug, Default)]
struct PickedState {
    last_id: u64,
    current: Option<(PickId, NormalizedPath)>,
}

impl PickedRootSlot {
    /// Guarda la carpeta ya validada y devuelve su identificador nuevo.
    pub fn put(&self, path: NormalizedPath) -> PickId {
        let mut state = lock(&self.0);
        state.last_id += 1;
        let id = PickId::from_counter(state.last_id);
        state.current = Some((id, path));
        id
    }

    /// La carpeta de `id`, si es la seleccion vigente.
    pub fn get(&self, id: PickId) -> Option<NormalizedPath> {
        lock(&self.0)
            .current
            .as_ref()
            .filter(|(current, _)| *current == id)
            .map(|(_, path)| path.clone())
    }
}

/// Solo un dialogo de seleccion a la vez.
#[derive(Debug, Clone, Default)]
pub struct PickerGate(Arc<AtomicBool>);

/// Mientras vive, el selector esta ocupado.
#[derive(Debug)]
pub struct PickerLease(Arc<AtomicBool>);

impl PickerGate {
    pub fn try_acquire(&self) -> Option<PickerLease> {
        if self.0.swap(true, Ordering::AcqRel) {
            None
        } else {
            Some(PickerLease(Arc::clone(&self.0)))
        }
    }
}

impl Drop for PickerLease {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

// ---------------------------------------------------------------------------------------------
// Logica de los comandos (generica, sin Tauri)
// ---------------------------------------------------------------------------------------------

impl From<FolderScanError> for AppError {
    fn from(error: FolderScanError) -> Self {
        match error {
            FolderScanError::RootRejected(reason) => {
                Self::FolderRootRejected(rejection_code(reason))
            }
        }
    }
}

/// Dependencias de `start_folder_scan`.
pub struct FolderPorts<'a, S: ?Sized, F, V, C> {
    pub store: &'a S,
    pub fs: &'a F,
    pub volume: &'a V,
    pub clock: &'a C,
    pub active: &'a ActiveJob,
}

fn safe_id(raw: i64, what: &str) -> Result<i64, AppError> {
    if (0..=MAX_SAFE_ID).contains(&raw) {
        Ok(raw)
    } else {
        Err(AppError::InvalidArgument(format!("{what} is out of range")))
    }
}

fn page_args(offset: i64, limit: i64) -> Result<(u64, u32), AppError> {
    let offset = u64::try_from(offset)
        .map_err(|_| AppError::InvalidArgument("offset must be 0 or more".to_owned()))?;
    let limit = u32::try_from(limit)
        .ok()
        .filter(|limit| (1..=MAX_PAGE_SIZE).contains(limit))
        .ok_or_else(|| {
            AppError::InvalidArgument(format!("limit must be between 1 and {MAX_PAGE_SIZE}"))
        })?;
    Ok((offset, limit))
}

/// Adapta el progreso del dominio a eventos del canal.
struct ScanEvents<'a> {
    emit: &'a mut dyn FnMut(FolderScanEventDto),
}

impl FolderScanObserver for ScanEvents<'_> {
    fn walking(&mut self, files_seen: u64, dirs_seen: u64, bytes_seen: u64) {
        (self.emit)(FolderScanEventDto::Walking {
            files_seen,
            dirs_seen,
            bytes_seen,
        });
    }

    fn saving(&mut self) {
        (self.emit)(FolderScanEventDto::Saving);
    }
}

/// Corre un analisis completo: toma el lugar del trabajo activo, valida la raiz, crea el escaneo,
/// recorre, persiste y devuelve el resumen.
///
/// - Argumentos invalidos, `pickId` desconocido o raiz que dejo de ser valida fallan ANTES de
///   crear el escaneo.
/// - Cancelado: el escaneo queda `cancelled` y no se guarda nada; el ultimo `done` sigue vigente.
/// - Fallo al guardar: el escaneo se cierra como `error` y el fallo se propaga.
pub fn run_folder_scan<S, F, V, C>(
    ports: &FolderPorts<'_, S, F, V, C>,
    privileges: &PrivilegeContext,
    raw: RawProtectionContext,
    root: &RootArg,
    picked: &PickedRootSlot,
    emit: &mut dyn FnMut(FolderScanEventDto),
) -> Result<FolderScanSummaryDto, AppError>
where
    S: Store + ?Sized,
    F: FileSystem,
    V: VolumeSpace,
    C: Clock,
{
    let guard = ports.active.begin_scan()?;
    let request = RootRequest::parse(&root.kind, root.pick_id)?;
    let choice = match request {
        RootRequest::SystemDrive => RootChoice::SystemDrive,
        RootRequest::Picked(id) => {
            RootChoice::Picked(picked.get(id).ok_or(AppError::FolderPickNotFound)?)
        }
    };
    let protection = ProtectionContext::from_raw(restrict_protection_context(raw, privileges));
    let validated = validate_root(ports.fs, &protection, &choice)?;
    run_scan_with_guard(ports, &guard, validated.kind(), protection, choice, emit)
}

fn run_scan_with_guard<S, F, V, C>(
    ports: &FolderPorts<'_, S, F, V, C>,
    guard: &JobGuard,
    kind: RootKind,
    protection: Result<ProtectionContext, crate::domain::protection::context::ContextError>,
    choice: RootChoice,
    emit: &mut dyn FnMut(FolderScanEventDto),
) -> Result<FolderScanSummaryDto, AppError>
where
    S: Store + ?Sized,
    F: FileSystem,
    V: VolumeSpace,
    C: Clock,
{
    let store = ports.store;
    let started_at = ports.clock.now();
    let scan_id = store.create_scan(&NewScan {
        kind: ScanKind::Folders,
        trigger: ScanTrigger::Manual,
        started_at,
    })?;
    guard.bind(scan_id);
    emit(FolderScanEventDto::Started {
        scan_id: scan_id.0,
        root_kind: kind.as_str(),
    });
    let outcome = scan_folders(
        choice,
        protection,
        ports.fs,
        ports.volume,
        guard.cancel_flag(),
        &mut ScanEvents { emit },
    );
    match outcome {
        Ok(FolderScanOutcome::Completed(results)) => {
            if let Err(error) = store.complete_folder_scan(scan_id, &results, ports.clock.now()) {
                // El escaneo sigue `running`: se cierra como error (si tambien falla, la
                // apertura de la base lo cierra).
                let _ = store.finish_scan(scan_id, ScanStatus::Error, ports.clock.now());
                return Err(error.into());
            }
            match store.last_done_folder_scan()? {
                Some(summary) if summary.scan.id == scan_id => Ok((&summary).into()),
                _ => Err(AppError::Internal(
                    "the finished scan is missing from the store".to_owned(),
                )),
            }
        }
        Ok(FolderScanOutcome::Cancelled) => {
            let finished_at = ports.clock.now();
            store.finish_scan(scan_id, ScanStatus::Cancelled, finished_at)?;
            Ok(cancelled_dto(scan_id, kind, started_at.0, finished_at.0))
        }
        Err(error) => {
            // La raiz se validaba justo antes: solo una carrera con el disco llega aca.
            let _ = store.finish_scan(scan_id, ScanStatus::Error, ports.clock.now());
            Err(error.into())
        }
    }
}

fn cancelled_dto(
    scan_id: ScanId,
    kind: RootKind,
    started_at: i64,
    finished_at: i64,
) -> FolderScanSummaryDto {
    FolderScanSummaryDto {
        scan_id: scan_id.0,
        status: ScanStatus::Cancelled.as_str(),
        started_at,
        finished_at: Some(finished_at),
        root: FolderRootDto {
            kind: kind.as_str(),
            path: String::new(),
            folder_id: None,
        },
        volume: None,
        unaccounted_bytes: None,
        over_counted: false,
        total_bytes: 0,
        file_count: 0,
        dir_count: 0,
        inaccessible_dirs: 0,
        reparse_skipped: 0,
        cloud_files: 0,
        opaque_dirs: 0,
        folders_stored: 0,
        folders_truncated: false,
        issues: Vec::new(),
    }
}

/// Resumen del ultimo analisis terminado, si existe.
pub fn last_folder_scan<S: Store + ?Sized>(
    store: &S,
) -> Result<Option<FolderScanSummaryDto>, AppError> {
    Ok(store
        .last_done_folder_scan()?
        .as_ref()
        .map(FolderScanSummaryDto::from))
}

/// Falla `scan_not_found` salvo que `scan_id` sea el ultimo analisis terminado.
fn ensure_last_scan<S: Store + ?Sized>(store: &S, scan_id: i64) -> Result<ScanId, AppError> {
    match store.last_done_folder_scan()? {
        Some(summary) if summary.scan.id.0 == scan_id => Ok(summary.scan.id),
        _ => Err(AppError::ScanNotFound(scan_id.to_string())),
    }
}

fn folder_not_found(error: StoreError) -> AppError {
    match error {
        StoreError::NotFound(_) => AppError::FolderNotFound,
        other => other.into(),
    }
}

/// Una pagina de los hijos de una carpeta del ultimo analisis (`folder_id = None` = la raiz).
pub fn list_children<S: Store + ?Sized>(
    store: &S,
    scan_id: i64,
    folder_id: Option<i64>,
    sort: &str,
    offset: i64,
    limit: i64,
) -> Result<FolderPageDto, AppError> {
    safe_id(scan_id, "scan id")?;
    let folder_id = folder_id.map(|id| safe_id(id, "folder id")).transpose()?;
    let sort = FolderSort::parse(sort)
        .ok_or_else(|| AppError::InvalidArgument("unknown sort order".to_owned()))?;
    let (offset, limit) = page_args(offset, limit)?;
    let scan = ensure_last_scan(store, scan_id)?;
    let page = store
        .list_folder_children(scan, folder_id, sort, offset, limit)
        .map_err(folder_not_found)?;
    Ok(FolderPageDto::new(scan_id, &page))
}

/// Una pagina del top de archivos grandes del ultimo analisis.
pub fn list_files<S: Store + ?Sized>(
    store: &S,
    scan_id: i64,
    sort: &str,
    offset: i64,
    limit: i64,
) -> Result<LargeFilePageDto, AppError> {
    safe_id(scan_id, "scan id")?;
    let sort = LargeFileSort::parse(sort)
        .ok_or_else(|| AppError::InvalidArgument("unknown sort order".to_owned()))?;
    let (offset, limit) = page_args(offset, limit)?;
    let scan = ensure_last_scan(store, scan_id)?;
    let page = store.list_large_files(scan, sort, offset, limit)?;
    Ok(LargeFilePageDto::new(scan_id, &page))
}

/// Abre el selector, valida la carpeta elegida como raiz de analisis y la guarda como la
/// seleccion vigente. Un solo dialogo a la vez; cancelar conserva la seleccion anterior.
pub fn pick_folder_run<P, F>(
    picker: &P,
    gate: &PickerGate,
    slot: &PickedRootSlot,
    fs: &F,
    privileges: &PrivilegeContext,
    raw: RawProtectionContext,
) -> Result<PickResultDto, AppError>
where
    P: FolderPicker,
    F: FileSystem,
{
    let _lease = gate.try_acquire().ok_or(AppError::FolderPickerBusy)?;
    let chosen = picker
        .pick()
        .map_err(|_| AppError::FolderPickerUnavailable)?;
    let Some(path) = chosen else {
        return Ok(PickResultDto::Cancelled);
    };
    let normalized = path
        .to_str()
        .and_then(|text| normalize_lexical(text).ok())
        .ok_or(AppError::FolderRootRejected("unparseable_path"))?;
    let protection = ProtectionContext::from_raw(restrict_protection_context(raw, privileges));
    let validated = validate_root(fs, &protection, &RootChoice::Picked(normalized))?;
    let id = slot.put(validated.path().clone());
    Ok(PickResultDto::Picked {
        pick_id: i64::try_from(id.get()).unwrap_or(MAX_SAFE_ID),
        path: validated.to_string(),
    })
}

fn reveal_reason(reason: MismatchReason) -> RevealReason {
    match reason {
        MismatchReason::DifferentUser => RevealReason::DifferentUser,
        MismatchReason::Undetermined => RevealReason::Undetermined,
    }
}

/// Muestra una carpeta o un archivo del ultimo analisis en el Explorador, por su id. Nunca
/// recibe una ruta, nunca abre ni ejecuta el elemento.
pub fn reveal_run<S, F, E>(
    store: &S,
    fs: &F,
    explorer: &E,
    privileges: &PrivilegeContext,
    scan_id: i64,
    target: &RevealTargetArg,
) -> Result<(), AppError>
where
    S: Store + ?Sized,
    F: FileSystem,
    E: Explorer,
{
    safe_id(scan_id, "scan id")?;
    safe_id(target.id, "target id")?;
    let is_folder = match target.kind.as_str() {
        "folder" => true,
        "file" => false,
        _ => return Err(AppError::InvalidArgument("unknown target kind".to_owned())),
    };
    let scan = ensure_last_scan(store, scan_id)?;
    // El Explorador se abriria con la cuenta elevada, no con la del usuario sentado.
    if let ScopeAccess::Blocked(reason) = scope_access(privileges, UserScope::PerUser) {
        return Err(AppError::RevealUnavailable(reveal_reason(reason)));
    }
    let path = if is_folder {
        let folder = store
            .get_folder(scan, target.id)?
            .ok_or(AppError::FolderNotFound)?;
        if folder.opaque.is_some() {
            return Err(AppError::RevealUnavailable(RevealReason::Opaque));
        }
        folder.path
    } else {
        store
            .get_large_file(scan, target.id)?
            .ok_or(AppError::FolderNotFound)?
            .path
    };
    let path = PathBuf::from(path);
    // La ruta guardada se verifica de nuevo, sin seguir enlaces.
    if fs.metadata(&path).is_err() {
        return Err(AppError::RevealUnavailable(RevealReason::TargetMissing));
    }
    explorer
        .reveal(&path)
        .map_err(|_| AppError::RevealUnavailable(RevealReason::ShellError))
}

// ---------------------------------------------------------------------------------------------
// Comandos IPC
// ---------------------------------------------------------------------------------------------

/// Comando IPC: inicia un analisis; el progreso viaja por `on_progress` y la respuesta es el
/// resumen final (`done` o `cancelled`).
#[tauri::command]
pub async fn start_folder_scan(
    root: RootArg,
    on_progress: Channel<FolderScanEventDto>,
    handle: State<'_, StoreHandle>,
    active: State<'_, ActiveJob>,
    privileges: State<'_, PrivilegeContext>,
    environment: State<'_, ScanEnvironment>,
    picked: State<'_, PickedRootSlot>,
) -> Result<FolderScanSummaryDto, AppError> {
    let handle = handle.inner().clone();
    let active = active.inner().clone();
    let privileges = *privileges.inner();
    let picked = picked.inner().clone();
    let data_dir = environment.data_dir();
    run_blocking(move || {
        let store = handle.store()?;
        let exe = std::env::current_exe().ok();
        let raw = resolve_protection_context(
            UserFolderSource::ProcessToken,
            app_dirs(data_dir.as_deref(), exe.as_deref()),
        );
        let ports = FolderPorts {
            store: &*store,
            fs: &WindowsFileSystem,
            volume: &WindowsVolumeSpace,
            clock: &SystemClock,
            active: &active,
        };
        // Un error de envio se ignora: la UI pudo cerrarse y el analisis igual se completa.
        run_folder_scan(&ports, &privileges, raw, &root, &picked, &mut |event| {
            let _ = on_progress.send(event);
        })
    })
    .await
}

/// Comando IPC: resumen del ultimo analisis terminado (`null` si no hay).
#[tauri::command]
pub async fn get_last_folder_scan(
    handle: State<'_, StoreHandle>,
) -> Result<Option<FolderScanSummaryDto>, AppError> {
    let handle = handle.inner().clone();
    run_blocking(move || last_folder_scan(&*handle.store()?)).await
}

/// Comando IPC: hijos de una carpeta del ultimo analisis.
#[tauri::command]
pub async fn list_folder_children(
    scan_id: i64,
    folder_id: Option<i64>,
    sort: String,
    offset: i64,
    limit: i64,
    handle: State<'_, StoreHandle>,
) -> Result<FolderPageDto, AppError> {
    let handle = handle.inner().clone();
    run_blocking(move || list_children(&*handle.store()?, scan_id, folder_id, &sort, offset, limit))
        .await
}

/// Comando IPC: archivos mas grandes del ultimo analisis.
#[tauri::command]
pub async fn list_large_files(
    scan_id: i64,
    sort: String,
    offset: i64,
    limit: i64,
    handle: State<'_, StoreHandle>,
) -> Result<LargeFilePageDto, AppError> {
    let handle = handle.inner().clone();
    run_blocking(move || list_files(&*handle.store()?, scan_id, &sort, offset, limit)).await
}

/// Comando IPC: abre el selector de carpeta nativo, valida la eleccion y devuelve su `pickId`.
#[tauri::command]
pub async fn pick_folder_root<R: tauri::Runtime>(
    window: tauri::WebviewWindow<R>,
    privileges: State<'_, PrivilegeContext>,
    environment: State<'_, ScanEnvironment>,
    picked: State<'_, PickedRootSlot>,
    gate: State<'_, PickerGate>,
) -> Result<PickResultDto, AppError> {
    let privileges = *privileges.inner();
    let picked = picked.inner().clone();
    let gate = gate.inner().clone();
    let data_dir = environment.data_dir();
    // La ventana principal es la duena del dialogo (modal respecto de ella).
    let owner = window.hwnd().ok().map(|hwnd| hwnd.0 as isize);
    run_blocking(move || {
        let exe = std::env::current_exe().ok();
        let raw = resolve_protection_context(
            UserFolderSource::ProcessToken,
            app_dirs(data_dir.as_deref(), exe.as_deref()),
        );
        pick_folder_run(
            &WindowsFolderPicker::new(owner),
            &gate,
            &picked,
            &WindowsFileSystem,
            &privileges,
            raw,
        )
    })
    .await
}

/// Comando IPC: muestra una carpeta o un archivo del ultimo analisis en el Explorador.
#[tauri::command]
pub async fn reveal_in_explorer(
    scan_id: i64,
    target: RevealTargetArg,
    handle: State<'_, StoreHandle>,
    privileges: State<'_, PrivilegeContext>,
) -> Result<(), AppError> {
    let handle = handle.inner().clone();
    let privileges = *privileges.inner();
    run_blocking(move || {
        reveal_run(
            &*handle.store()?,
            &WindowsFileSystem,
            &WindowsExplorer::new(),
            &privileges,
            scan_id,
            &target,
        )
    })
    .await
}

#[cfg(test)]
#[path = "disk_usage_tests.rs"]
mod tests;

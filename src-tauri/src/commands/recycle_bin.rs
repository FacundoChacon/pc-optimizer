//! Comandos del vaciado de la Papelera (C-10): `prepare_empty_recycle_bin`,
//! `acknowledge_empty_recycle_bin` y `empty_recycle_bin`.
//!
//! Es la UNICA accion permanente del producto (RN-LIM-06). La doble confirmacion la aplica el
//! backend con tres comandos que el dominio ordena (`EmptySlot`): preparar (mide) -> reconocer
//! (primera confirmacion, entrega un token) -> vaciar (segunda confirmacion con el token). Los
//! comandos son finos y delegan en funciones genericas que se prueban con dobles. Ningun
//! argumento acepta una raiz ni una ruta: el volumen lo decide el backend. Las respuestas no
//! llevan rutas, SIDs ni nombres de cuenta.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::State;

use super::run_blocking;
use super::scan::{ActiveJob, ScanEnvironment, app_dirs, lock};
use crate::adapters::clock::SystemClock;
use crate::adapters::sqlite::StoreHandle;
use crate::adapters::windows::recycle_bin_empty::WindowsRecycleBinEmptier;
use crate::adapters::windows::{UserFolderSource, WindowsRecycleBin, resolve_protection_context};
use crate::domain::ports::{CleanupLogStore, Clock, RecycleBinEmptier, RecycleBinInfo};
use crate::domain::privileges::PrivilegeContext;
use crate::domain::recycle_bin::execute::{
    EmptyFlowError, ensure_not_blocked, prepare_empty, run_empty_recycle_bin,
};
use crate::domain::recycle_bin::outcome::EmptyOutcome;
use crate::domain::recycle_bin::request::{EmptyRequestError, EmptySlot};
use crate::domain::recycle_bin::root::VolumeRoot;
use crate::error::AppError;

/// Mayor entero que JavaScript representa sin perdida (`Number.MAX_SAFE_INTEGER`).
const MAX_SAFE_ID: i64 = (1 << 53) - 1;

/// Primer valor de los tokens: el espacio de tokens no se solapa con el de los identificadores
/// (contador desde 1), asi un token nunca es igual al id de su propia solicitud.
const TOKEN_BASE: u64 = 1 << 32;

// ---------------------------------------------------------------------------------------------
// DTOs (camelCase hacia el frontend; sin rutas, SIDs ni cuentas)
// ---------------------------------------------------------------------------------------------

/// Solicitud medida (paso 1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmptyRequestDto {
    pub request_id: u64,
    pub item_count: u64,
    pub size_bytes: u64,
    pub created_at: i64,
    pub expires_at: i64,
}

/// Primera confirmacion hecha (paso 2): el token autoriza la segunda.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmptyAckDto {
    pub request_id: u64,
    pub confirm_token: u64,
    pub item_count: u64,
    pub size_bytes: u64,
    pub expires_at: i64,
}

/// Resultado del vaciado (paso 3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmptyResultDto {
    /// `emptied`, `partial`, `failed` o `unverified`.
    pub status: &'static str,
    pub items_before: u64,
    pub bytes_before: u64,
    /// `null` si la medicion posterior fallo.
    pub items_after: Option<u64>,
    pub bytes_after: Option<u64>,
    pub freed_bytes: u64,
    pub detail: Option<String>,
    /// `false` si no se pudo guardar el registro del vaciado (que igual ocurrio).
    pub logged: bool,
    pub finished_at: i64,
}

impl EmptyResultDto {
    fn new(outcome: &EmptyOutcome, logged: bool) -> Self {
        Self {
            status: outcome.result.status.as_str(),
            items_before: outcome.before.items,
            bytes_before: outcome.before.bytes,
            items_after: outcome.result.items_after,
            bytes_after: outcome.result.bytes_after,
            freed_bytes: outcome.result.freed_bytes,
            detail: outcome.result.detail.clone(),
            logged,
            finished_at: outcome.finished_at.0,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Estado: la solicitud vigente (D-02)
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Default)]
struct RequestsInner {
    slot: Mutex<EmptySlot>,
    next_id: AtomicU64,
    next_token: AtomicU64,
}

/// La unica solicitud de vaciado vigente y los contadores del proceso (estado gestionado).
#[derive(Debug, Clone, Default)]
pub struct EmptyRequests(Arc<RequestsInner>);

impl EmptyRequests {
    /// Identificador siguiente: contador monotono desde 1.
    fn next_id(&self) -> u64 {
        // Solo es un contador: no ordena otros accesos a memoria.
        self.0.next_id.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// Token siguiente: segundo contador independiente (prueba el orden de los pasos; no es un
    /// secreto, D-01).
    fn next_token(&self) -> u64 {
        TOKEN_BASE + self.0.next_token.fetch_add(1, Ordering::Relaxed)
    }
}

// ---------------------------------------------------------------------------------------------
// Logica de los comandos (generica, sin Tauri)
// ---------------------------------------------------------------------------------------------

/// Dependencias del vaciado (puertos del SO, la persistencia y el estado de la sesion).
pub struct EmptyPorts<'a, S: ?Sized, B, E, C> {
    pub store: &'a S,
    pub bin: &'a B,
    pub emptier: &'a E,
    pub clock: &'a C,
    pub requests: &'a EmptyRequests,
    pub active: &'a ActiveJob,
}

impl From<EmptyFlowError> for AppError {
    fn from(error: EmptyFlowError) -> Self {
        match error {
            EmptyFlowError::Blocked(reason) => Self::RecycleBinBlocked(reason),
            EmptyFlowError::Unavailable => Self::RecycleBinUnavailable,
            EmptyFlowError::AlreadyEmpty => Self::RecycleBinAlreadyEmpty,
            EmptyFlowError::NotFound => Self::EmptyRequestNotFound,
            EmptyFlowError::Expired => Self::EmptyRequestExpired,
            EmptyFlowError::Changed => Self::RecycleBinChanged,
        }
    }
}

impl From<EmptyRequestError> for AppError {
    fn from(error: EmptyRequestError) -> Self {
        match error {
            EmptyRequestError::NotFound => Self::EmptyRequestNotFound,
            EmptyRequestError::Expired => Self::EmptyRequestExpired,
            EmptyRequestError::NotAcknowledged => Self::EmptyNotAcknowledged,
        }
    }
}

/// Identificador o token que viaja como entero JS: de 0 a 2^53 - 1.
fn safe_id(raw: i64, what: &str) -> Result<u64, AppError> {
    u64::try_from(raw)
        .ok()
        .filter(|_| raw <= MAX_SAFE_ID)
        .ok_or_else(|| AppError::InvalidArgument(format!("{what} is out of range")))
}

/// Paso 1: mide la Papelera del volumen del sistema y guarda la solicitud (reemplaza la anterior).
/// No vacia nada y no toma el lugar del trabajo activo, pero respeta al que corra.
pub fn prepare_empty_run<S, B, E, C>(
    ports: &EmptyPorts<'_, S, B, E, C>,
    privileges: &PrivilegeContext,
    root: Option<VolumeRoot>,
) -> Result<EmptyRequestDto, AppError>
where
    S: ?Sized,
    B: RecycleBinInfo,
    C: Clock,
{
    ensure_not_blocked(privileges)?;
    ports.active.ensure_idle()?;
    let request = prepare_empty(
        privileges,
        root,
        ports.bin,
        ports.clock.now(),
        ports.requests.next_id(),
    )?;
    let dto = EmptyRequestDto {
        request_id: request.id(),
        item_count: request.usage().items,
        size_bytes: request.usage().bytes,
        created_at: request.created_at().0,
        expires_at: request.expires_at().0,
    };
    lock(&ports.requests.0.slot).store(request);
    Ok(dto)
}

/// Paso 2: primera confirmacion; entrega el token que autoriza la segunda.
pub fn acknowledge_empty_run<S, B, E, C>(
    ports: &EmptyPorts<'_, S, B, E, C>,
    privileges: &PrivilegeContext,
    request_id: i64,
) -> Result<EmptyAckDto, AppError>
where
    S: ?Sized,
    C: Clock,
{
    ensure_not_blocked(privileges)?;
    ports.active.ensure_idle()?;
    let id = safe_id(request_id, "request id")?;
    let receipt = lock(&ports.requests.0.slot).acknowledge(
        id,
        ports.clock.now(),
        ports.requests.next_token(),
    )?;
    Ok(EmptyAckDto {
        request_id: receipt.request_id,
        confirm_token: receipt.token,
        item_count: receipt.usage.items,
        size_bytes: receipt.usage.bytes,
        expires_at: receipt.expires_at.0,
    })
}

/// Paso 3: segunda confirmacion y ejecucion (D-02, D-04, D-06, D-09).
///
/// Orden: mismatch -> argumentos -> lugar del trabajo activo (ninguno consume la solicitud) ->
/// se TOMA la solicitud (queda consumida pase lo que pase) -> re-medicion -> vaciado -> medicion
/// posterior -> una fila en `cleanup_log`. Si la fila falla despues de vaciar, el resultado real
/// se devuelve con `logged: false`.
pub fn empty_run<S, B, E, C>(
    ports: &EmptyPorts<'_, S, B, E, C>,
    privileges: &PrivilegeContext,
    request_id: i64,
    confirm_token: i64,
) -> Result<EmptyResultDto, AppError>
where
    S: CleanupLogStore + ?Sized,
    B: RecycleBinInfo,
    E: RecycleBinEmptier,
    C: Clock,
{
    ensure_not_blocked(privileges)?;
    let id = safe_id(request_id, "request id")?;
    let token = safe_id(confirm_token, "confirm token")?;
    let _guard = ports.active.begin_empty()?;
    let request = lock(&ports.requests.0.slot).take_for_empty(id)?;
    let outcome = run_empty_recycle_bin(
        request,
        token,
        privileges,
        ports.bin,
        ports.emptier,
        ports.clock,
    )?;
    let logged = ports
        .store
        .append_cleanup_log(&[outcome.log_entry()])
        .is_ok();
    Ok(EmptyResultDto::new(&outcome, logged))
}

/// Volumen del sistema (unidad de `windows_dir` del contexto crudo); `None` si no se pudo
/// resolver: nunca se adivina una letra.
fn system_root(
    data_dir: Option<&std::path::Path>,
    exe: Option<&std::path::Path>,
) -> Option<VolumeRoot> {
    resolve_protection_context(UserFolderSource::ProcessToken, app_dirs(data_dir, exe))
        .system_drive()
        .and_then(VolumeRoot::from_drive_letter)
}

// ---------------------------------------------------------------------------------------------
// Comandos IPC
// ---------------------------------------------------------------------------------------------

/// Comando IPC, paso 1: mide la Papelera del volumen del sistema y prepara la solicitud.
#[tauri::command]
pub async fn prepare_empty_recycle_bin(
    handle: State<'_, StoreHandle>,
    requests: State<'_, EmptyRequests>,
    active: State<'_, ActiveJob>,
    privileges: State<'_, PrivilegeContext>,
    environment: State<'_, ScanEnvironment>,
) -> Result<EmptyRequestDto, AppError> {
    let handle = handle.inner().clone();
    let requests = requests.inner().clone();
    let active = active.inner().clone();
    let privileges = *privileges.inner();
    let data_dir = environment.data_dir();
    run_blocking(move || {
        // Sin base no hay auditoria y sin auditoria no se llega al paso 3: se falla de entrada.
        let store = handle.store()?;
        let exe = std::env::current_exe().ok();
        let ports = EmptyPorts {
            store: &*store,
            bin: &WindowsRecycleBin,
            emptier: &WindowsRecycleBinEmptier::new(),
            clock: &SystemClock,
            requests: &requests,
            active: &active,
        };
        ensure_not_blocked(&privileges)?;
        let root = system_root(data_dir.as_deref(), exe.as_deref());
        prepare_empty_run(&ports, &privileges, root)
    })
    .await
}

/// Comando IPC, paso 2: primera confirmacion de la solicitud `request_id`.
#[tauri::command]
pub async fn acknowledge_empty_recycle_bin(
    request_id: i64,
    handle: State<'_, StoreHandle>,
    requests: State<'_, EmptyRequests>,
    active: State<'_, ActiveJob>,
    privileges: State<'_, PrivilegeContext>,
) -> Result<EmptyAckDto, AppError> {
    let handle = handle.inner().clone();
    let requests = requests.inner().clone();
    let active = active.inner().clone();
    let privileges = *privileges.inner();
    run_blocking(move || {
        let store = handle.store()?;
        let ports = EmptyPorts {
            store: &*store,
            bin: &WindowsRecycleBin,
            emptier: &WindowsRecycleBinEmptier::new(),
            clock: &SystemClock,
            requests: &requests,
            active: &active,
        };
        acknowledge_empty_run(&ports, &privileges, request_id)
    })
    .await
}

/// Comando IPC, paso 3: segunda confirmacion con el token y vaciado. IRREVERSIBLE.
#[tauri::command]
pub async fn empty_recycle_bin(
    request_id: i64,
    confirm_token: i64,
    handle: State<'_, StoreHandle>,
    requests: State<'_, EmptyRequests>,
    active: State<'_, ActiveJob>,
    privileges: State<'_, PrivilegeContext>,
) -> Result<EmptyResultDto, AppError> {
    let handle = handle.inner().clone();
    let requests = requests.inner().clone();
    let active = active.inner().clone();
    let privileges = *privileges.inner();
    run_blocking(move || {
        // El Store primero: sin auditoria no hay accion permanente.
        let store = handle.store()?;
        let ports = EmptyPorts {
            store: &*store,
            bin: &WindowsRecycleBin,
            emptier: &WindowsRecycleBinEmptier::new(),
            clock: &SystemClock,
            requests: &requests,
            active: &active,
        };
        empty_run(&ports, &privileges, request_id, confirm_token)
    })
    .await
}

#[cfg(test)]
#[path = "recycle_bin_tests.rs"]
mod tests;

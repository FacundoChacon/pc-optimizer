//! Comandos del historial de limpieza (C-12): `list_cleanup_runs` y `list_cleanup_entries`.
//!
//! Solo leen el log append-only: no reciben rutas, no escriben y no toman el lugar de ningun
//! trabajo (`ActiveJob`), por lo que responden mientras hay un escaneo o una limpieza en curso.
//! Las rutas se ocultan en el backend cuando el contexto de privilegios no es `Match` (D-11).

use serde::{Deserialize, Serialize};
use tauri::State;

use super::run_blocking;
use crate::adapters::sqlite::StoreHandle;

use crate::domain::cleanup::history::{HistoryEntry, RunKind, redact_paths};
use crate::domain::model::{EpochMillis, ScanId};
use crate::domain::ports::{CleanupLogStore, CleanupRunKey, CleanupRunRow, RunCursor};
use crate::domain::privileges::PrivilegeContext;
use crate::domain::recycle_bin::outcome::{DETAIL_PARTIAL, DETAIL_UNVERIFIED};
use crate::error::AppError;

/// Maximo de limpiezas por pagina.
pub const MAX_RUNS_PAGE: i64 = 50;
/// Maximo de items por pagina.
pub const MAX_ENTRIES_PAGE: i64 = 200;
/// Mayor entero que JavaScript representa sin perdida (`Number.MAX_SAFE_INTEGER`).
const MAX_SAFE_ID: i64 = (1 << 53) - 1;

// ---------------------------------------------------------------------------------------------
// Argumentos (tal como llegan por IPC) y DTOs (camelCase hacia el frontend)
// ---------------------------------------------------------------------------------------------

/// Clave de una limpieza pedida: `{ kind: "scan", scanId }` o `{ kind: "entry", entryId }`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunKeyArg {
    pub kind: String,
    pub scan_id: Option<i64>,
    pub entry_id: Option<i64>,
}

/// Cursor pedido: la fecha y la clave de la ultima limpieza vista.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunCursorArg {
    pub finished_at: i64,
    pub key: RunKeyArg,
}

/// Clave de una limpieza hacia el frontend, etiquetada por `kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RunKeyDto {
    #[serde(rename_all = "camelCase")]
    Scan { scan_id: i64 },
    #[serde(rename_all = "camelCase")]
    Entry { entry_id: i64 },
}

impl From<CleanupRunKey> for RunKeyDto {
    fn from(key: CleanupRunKey) -> Self {
        match key {
            CleanupRunKey::Scan(id) => Self::Scan { scan_id: id.0 },
            CleanupRunKey::Entry(id) => Self::Entry { entry_id: id },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunCursorDto {
    pub finished_at: i64,
    pub key: RunKeyDto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutcomesDto {
    pub moved_to_trash: u64,
    pub skipped_locked: u64,
    pub skipped_protected: u64,
    pub skipped_cloud: u64,
    pub error: u64,
    pub recycle_bin_emptied: u64,
}

/// Una limpieza del historial, sin rutas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupRunDto {
    pub key: RunKeyDto,
    pub kind: &'static str,
    pub started_at: i64,
    pub finished_at: i64,
    pub entry_count: u64,
    pub outcomes: OutcomesDto,
    pub moved_bytes: u64,
    pub emptied_bytes: u64,
    /// `partial` o `unverified` en un vaciado de la Papelera; `null` en el resto.
    pub empty_detail: Option<&'static str>,
    pub categories: Vec<String>,
}

impl From<&CleanupRunRow> for CleanupRunDto {
    fn from(run: &CleanupRunRow) -> Self {
        let empty_detail = match run.empty_detail.as_deref() {
            Some(DETAIL_PARTIAL) => Some(DETAIL_PARTIAL),
            Some(DETAIL_UNVERIFIED) => Some(DETAIL_UNVERIFIED),
            _ => None,
        };
        Self {
            key: run.key.into(),
            kind: RunKind::classify(&run.categories, run.key).as_str(),
            started_at: run.started_at.0,
            finished_at: run.finished_at.0,
            entry_count: run.entry_count,
            outcomes: OutcomesDto {
                moved_to_trash: run.outcomes.moved_to_trash,
                skipped_locked: run.outcomes.skipped_locked,
                skipped_protected: run.outcomes.skipped_protected,
                skipped_cloud: run.outcomes.skipped_cloud,
                error: run.outcomes.error,
                recycle_bin_emptied: run.outcomes.recycle_bin_emptied,
            },
            moved_bytes: run.moved_bytes,
            emptied_bytes: run.emptied_bytes,
            empty_detail,
            categories: run.categories.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupRunPageDto {
    pub items: Vec<CleanupRunDto>,
    pub next_cursor: Option<RunCursorDto>,
}

/// Un item de una limpieza; la ruta es `null` cuando esta oculta.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupEntryDto {
    pub id: i64,
    pub ts: i64,
    pub category: String,
    pub path: Option<String>,
    pub size_bytes: u64,
    pub outcome: &'static str,
    pub error_detail: Option<String>,
}

impl From<HistoryEntry> for CleanupEntryDto {
    fn from(entry: HistoryEntry) -> Self {
        Self {
            id: entry.id,
            ts: entry.ts.0,
            category: entry.category,
            path: entry.path,
            size_bytes: entry.size_bytes,
            outcome: entry.outcome.as_str(),
            error_detail: entry.error_detail,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupEntryPageDto {
    pub total: u64,
    pub paths_hidden: bool,
    pub items: Vec<CleanupEntryDto>,
}

// ---------------------------------------------------------------------------------------------
// Validacion y logica (genericas sobre el store y el contexto)
// ---------------------------------------------------------------------------------------------

fn invalid(message: &str) -> AppError {
    AppError::InvalidArgument(message.to_owned())
}

fn page_limit(raw: i64, max: i64) -> Result<u32, AppError> {
    if (1..=max).contains(&raw) {
        u32::try_from(raw).map_err(|_| invalid("limit is out of range"))
    } else {
        Err(AppError::InvalidArgument(format!(
            "limit must be between 1 and {max}"
        )))
    }
}

fn safe_id(raw: Option<i64>, what: &str) -> Result<i64, AppError> {
    raw.filter(|id| (0..=MAX_SAFE_ID).contains(id))
        .ok_or_else(|| AppError::InvalidArgument(format!("{what} must be a safe integer")))
}

fn parse_key(arg: &RunKeyArg) -> Result<CleanupRunKey, AppError> {
    match arg.kind.as_str() {
        "scan" => Ok(CleanupRunKey::Scan(ScanId(safe_id(arg.scan_id, "scanId")?))),
        "entry" => Ok(CleanupRunKey::Entry(safe_id(arg.entry_id, "entryId")?)),
        _ => Err(invalid("unknown run key kind")),
    }
}

fn parse_cursor(arg: &RunCursorArg) -> Result<RunCursor, AppError> {
    if !(1..=MAX_SAFE_ID).contains(&arg.finished_at) {
        return Err(invalid("cursor finishedAt must be a positive safe integer"));
    }
    Ok(RunCursor {
        finished_at: EpochMillis(arg.finished_at),
        key: parse_key(&arg.key)?,
    })
}

/// Una pagina de limpiezas, de la mas reciente a la mas antigua (valida los argumentos antes de
/// leer el log).
pub fn list_runs<S: CleanupLogStore + ?Sized>(
    store: &S,
    before: Option<RunCursorArg>,
    limit: i64,
) -> Result<CleanupRunPageDto, AppError> {
    let limit = page_limit(limit, MAX_RUNS_PAGE)?;
    let before = before.as_ref().map(parse_cursor).transpose()?;
    // Se pide una de mas: si llega, hay otra pagina y el cursor es la ultima de esta.
    let mut runs = store.cleanup_runs(before, limit.saturating_add(1))?;
    let page_len = usize::try_from(limit).unwrap_or(usize::MAX);
    let has_more = runs.len() > page_len;
    runs.truncate(page_len);
    let next_cursor = runs.last().filter(|_| has_more).map(|last| RunCursorDto {
        finished_at: last.finished_at.0,
        key: last.key.into(),
    });
    Ok(CleanupRunPageDto {
        items: runs.iter().map(CleanupRunDto::from).collect(),
        next_cursor,
    })
}

/// Una pagina de items de una limpieza, en orden de insercion; con contexto `mismatch` las rutas
/// salen nulas (D-11).
pub fn list_entries<S: CleanupLogStore + ?Sized>(
    store: &S,
    context: &PrivilegeContext,
    key: &RunKeyArg,
    offset: i64,
    limit: i64,
) -> Result<CleanupEntryPageDto, AppError> {
    let limit = page_limit(limit, MAX_ENTRIES_PAGE)?;
    let key = parse_key(key)?;
    let offset = u64::try_from(offset)
        .ok()
        .filter(|offset| *offset <= MAX_SAFE_ID.unsigned_abs())
        .ok_or_else(|| invalid("offset must be a safe integer of 0 or more"))?;
    let (total, entries) = store.cleanup_run_entries(key, offset, limit)?;
    let (paths_hidden, items) = redact_paths(context, entries);
    Ok(CleanupEntryPageDto {
        total,
        paths_hidden,
        items: items.into_iter().map(CleanupEntryDto::from).collect(),
    })
}

// ---------------------------------------------------------------------------------------------
// Comandos IPC
// ---------------------------------------------------------------------------------------------

/// Comando IPC: una pagina de limpiezas del historial (sin rutas).
#[tauri::command]
pub async fn list_cleanup_runs(
    before: Option<RunCursorArg>,
    limit: i64,
    handle: State<'_, StoreHandle>,
) -> Result<CleanupRunPageDto, AppError> {
    let handle = handle.inner().clone();
    run_blocking(move || list_runs(&*handle.store()?, before, limit)).await
}

/// Comando IPC: una pagina de items de una limpieza (rutas ocultas en `mismatch`).
#[tauri::command]
pub async fn list_cleanup_entries(
    key: RunKeyArg,
    offset: i64,
    limit: i64,
    handle: State<'_, StoreHandle>,
    privileges: State<'_, PrivilegeContext>,
) -> Result<CleanupEntryPageDto, AppError> {
    let handle = handle.inner().clone();
    let privileges = *privileges.inner();
    run_blocking(move || list_entries(&*handle.store()?, &privileges, &key, offset, limit)).await
}

#[cfg(test)]
#[path = "history_tests.rs"]
mod tests;

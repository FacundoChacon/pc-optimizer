//! Resultado de un vaciado decidido por MEDICION (design D-08) y su entrada de log (D-09).
//!
//! El `Result` de la llamada al shell no decide: `SHEmptyRecycleBinW` devuelve errores aunque
//! haya vaciado (p. ej. `E_UNEXPECTED` con la Papelera vacia) y puede vaciar a medias. Lo que
//! manda es lo que mide el sistema despues.

use crate::domain::model::{CleanupOutcome, EpochMillis, NewCleanupLogEntry};
use crate::domain::ports::{EmptyError, PortError, RecycleBinUsage};

use super::root::VolumeRoot;

/// Categoria del registro de un vaciado en `cleanup_log`.
pub const LOG_CATEGORY: &str = "recycle_bin";
/// Detalle: se liberaron algunos elementos pero quedaron otros.
pub const DETAIL_PARTIAL: &str = "partial";
/// Detalle: el shell informo exito pero la medicion no bajo.
pub const DETAIL_NOT_EMPTIED: &str = "not_emptied";
/// Detalle: el shell informo exito pero no se pudo medir despues.
pub const DETAIL_UNVERIFIED: &str = "unverified";

/// Estado final de un vaciado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmptyStatus {
    /// La Papelera quedo en 0 elementos.
    Emptied,
    /// Se libero algo y quedaron elementos (probablemente en uso).
    Partial,
    /// No se libero nada.
    Failed,
    /// El shell informo exito pero no se pudo comprobar.
    Unverified,
}

impl EmptyStatus {
    /// Codigo estable que viaja por IPC.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Emptied => "emptied",
            Self::Partial => "partial",
            Self::Failed => "failed",
            Self::Unverified => "unverified",
        }
    }
}

/// Resultado clasificado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmptyResult {
    pub status: EmptyStatus,
    /// `None` si la medicion posterior fallo.
    pub items_after: Option<u64>,
    pub bytes_after: Option<u64>,
    pub freed_bytes: u64,
    /// Codigo estable (`partial`, `not_emptied`, `unverified`, `permission_denied`,
    /// `empty_unavailable`, `io_0x...`); `None` con `Emptied`.
    pub detail: Option<String>,
}

/// Decide el resultado segun la tabla D-08.
pub fn classify_empty(
    before: RecycleBinUsage,
    call: Result<(), EmptyError>,
    after: Result<RecycleBinUsage, PortError>,
) -> EmptyResult {
    let Ok(after) = after else {
        return match call {
            Ok(()) => EmptyResult {
                status: EmptyStatus::Unverified,
                items_after: None,
                bytes_after: None,
                freed_bytes: before.bytes,
                detail: Some(DETAIL_UNVERIFIED.to_owned()),
            },
            Err(error) => failed(None, error.code()),
        };
    };
    if after.items == 0 {
        return EmptyResult {
            status: EmptyStatus::Emptied,
            items_after: Some(0),
            bytes_after: Some(after.bytes),
            freed_bytes: before.bytes,
            detail: None,
        };
    }
    if after.items < before.items || after.bytes < before.bytes {
        return EmptyResult {
            status: EmptyStatus::Partial,
            items_after: Some(after.items),
            bytes_after: Some(after.bytes),
            freed_bytes: before.bytes.saturating_sub(after.bytes),
            detail: Some(DETAIL_PARTIAL.to_owned()),
        };
    }
    match call {
        Ok(()) => failed(Some(after), DETAIL_NOT_EMPTIED),
        Err(error) => failed(Some(after), error.code()),
    }
}

fn failed(after: Option<RecycleBinUsage>, detail: &str) -> EmptyResult {
    EmptyResult {
        status: EmptyStatus::Failed,
        items_after: after.map(|usage| usage.items),
        bytes_after: after.map(|usage| usage.bytes),
        freed_bytes: 0,
        detail: Some(detail.to_owned()),
    }
}

/// Un vaciado pedido al shell y su resultado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmptyOutcome {
    pub root: VolumeRoot,
    /// Lo medido justo antes de vaciar (igual a lo que vio el usuario).
    pub before: RecycleBinUsage,
    pub result: EmptyResult,
    pub finished_at: EpochMillis,
}

impl EmptyOutcome {
    /// Entrada de `cleanup_log` (D-09): una sola fila, `scan_id` nulo para no marcar ningun
    /// escaneo de basura como limpiado.
    pub fn log_entry(&self) -> NewCleanupLogEntry {
        let (outcome, size_bytes) = match self.result.status {
            EmptyStatus::Emptied | EmptyStatus::Partial | EmptyStatus::Unverified => {
                (CleanupOutcome::RecycleBinEmptied, self.result.freed_bytes)
            }
            EmptyStatus::Failed => (CleanupOutcome::Error, 0),
        };
        NewCleanupLogEntry {
            ts: self.finished_at,
            category: LOG_CATEGORY.to_owned(),
            path: self.root.to_string(),
            size_bytes,
            outcome,
            scan_id: None,
            error_detail: self.result.detail.clone(),
        }
    }
}

#[cfg(test)]
#[path = "outcome_tests.rs"]
mod tests;

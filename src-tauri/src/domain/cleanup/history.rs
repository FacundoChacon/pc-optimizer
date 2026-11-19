//! Historial de limpiezas (C-12): clasificacion y ocultamiento de rutas, puros.

use crate::domain::junk::category::JunkCategory;
use crate::domain::model::{CleanupLogEntry, CleanupOutcome, EpochMillis};
use crate::domain::ports::CleanupRunKey;
use crate::domain::privileges::{PrivilegeContext, UserMatch};

/// Categoria del log de los duplicados movidos a la Papelera (C-11).
const DUPLICATES: &str = "duplicates";

/// Tipo de una limpieza del historial (D-09).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunKind {
    /// Todas sus filas son de categorias de la whitelist de basura.
    Junk,
    /// Todas sus filas son de la categoria `duplicates`.
    Duplicates,
    /// Una fila sin escaneo de la categoria `recycle_bin`.
    RecycleBinEmpty,
    /// Cualquier otra combinacion (reservado para las reglas propias de C-21).
    Other,
}

impl RunKind {
    /// Codigo estable que viaja por IPC.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Junk => "junk",
            Self::Duplicates => "duplicates",
            Self::RecycleBinEmpty => "recycle_bin_empty",
            Self::Other => "other",
        }
    }

    /// Clasifica una limpieza por las categorias de sus filas y por su clave.
    pub fn classify(categories: &[String], key: CleanupRunKey) -> Self {
        let all = |predicate: &dyn Fn(&str) -> bool| {
            !categories.is_empty() && categories.iter().all(|name| predicate(name))
        };
        if matches!(key, CleanupRunKey::Entry(_))
            && all(&|name| name == JunkCategory::RecycleBin.as_str())
        {
            Self::RecycleBinEmpty
        } else if all(&|name| name == DUPLICATES) {
            Self::Duplicates
        } else if all(&|name| JunkCategory::parse(name).is_some()) {
            Self::Junk
        } else {
            Self::Other
        }
    }
}

/// Fila del log tal como se muestra: la ruta puede estar oculta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    pub id: i64,
    pub ts: EpochMillis,
    pub category: String,
    pub path: Option<String>,
    pub size_bytes: u64,
    pub outcome: CleanupOutcome,
    pub error_detail: Option<String>,
}

/// Oculta las rutas cuando el contexto no es `Match` (cualquier motivo, incluido sin determinar):
/// el log vive en el perfil del token y quien mira puede ser otra cuenta (D-11). Devuelve si las
/// rutas quedaron ocultas. Conteos, tamanos, categorias y codigos no cambian.
pub fn redact_paths(
    context: &PrivilegeContext,
    entries: Vec<CleanupLogEntry>,
) -> (bool, Vec<HistoryEntry>) {
    let hidden = !matches!(context.user_match(), UserMatch::Match);
    let items = entries
        .into_iter()
        .map(|entry| HistoryEntry {
            id: entry.id,
            ts: entry.ts,
            category: entry.category,
            path: (!hidden).then_some(entry.path),
            size_bytes: entry.size_bytes,
            outcome: entry.outcome,
            error_detail: entry.error_detail,
        })
        .collect();
    (hidden, items)
}

#[cfg(test)]
#[path = "history_tests.rs"]
mod tests;

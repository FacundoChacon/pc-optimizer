//! Armado del preview (design D-02, D-03, D-04): funcion pura sobre filas persistidas.
//!
//! No recibe puertos de E/S: por construccion no puede tocar el disco ni la Papelera.

use crate::domain::junk::category::{CategoryScope, JunkCategory};
use crate::domain::junk::model::{FindingState, JunkScanSummary, StoredFinding};
use crate::domain::model::{EpochMillis, ScanId};
use crate::domain::privileges::{PrivilegeContext, ScopeAccess, UserScope, scope_access};

use super::model::{
    CleanupItem, CleanupPreview, PreviewCategory, PreviewCategoryStatus, PreviewId, is_recent,
};

/// Por que no se pudo armar el preview.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PreviewError {
    /// El escaneo pedido no es el ultimo escaneo de basura `done`.
    #[error("scan not found")]
    ScanNotFound,
    /// El escaneo ya tiene una limpieza registrada.
    #[error("scan already cleaned")]
    ScanAlreadyCleaned,
    /// Lista de categorias vacia, repetida o con una categoria no limpiable.
    #[error("invalid argument: {0}")]
    InvalidArgument(&'static str),
}

/// Todo lo que necesita `build_preview` (sin puertos de E/S).
pub struct PreviewInput<'a> {
    /// Escaneo que pidio el usuario.
    pub requested_scan: ScanId,
    /// Ultimo escaneo de basura terminado, segun la persistencia.
    pub last_done: Option<&'a JunkScanSummary>,
    /// Fecha de limpieza del escaneo derivada del log (`None` si no se limpio).
    pub cleaned_at: Option<EpochMillis>,
    pub categories: &'a [JunkCategory],
    /// Hallazgos elegibles de las categorias pedidas (`cleanup_candidates`).
    pub candidates: Vec<StoredFinding>,
    pub privileges: &'a PrivilegeContext,
    pub now: EpochMillis,
    pub id: PreviewId,
}

/// Arma un preview con las filas PROPIAS de una fuente distinta de la basura (copias duplicadas en
/// C-11, el asistente en C-21): la fuente es responsable de haber validado las filas y de los
/// totales de sus categorias. No toca el disco; `run_cleanup` aplica igualmente toda la
/// re-verificacion a cada item.
pub fn assemble_preview(
    id: PreviewId,
    scan_id: ScanId,
    created_at: EpochMillis,
    categories: Vec<PreviewCategory>,
    items: Vec<CleanupItem>,
) -> CleanupPreview {
    CleanupPreview {
        id,
        scan_id,
        created_at,
        categories,
        items,
    }
}

/// `recycle_bin` (solo se mide) y `old_logs` (sin ubicaciones) no son limpiables (D-02).
fn is_cleanable(category: JunkCategory) -> bool {
    !matches!(category, JunkCategory::RecycleBin | JunkCategory::OldLogs)
}

/// Lista no vacia, sin repetidas y solo con categorias limpiables.
pub fn validate_categories(categories: &[JunkCategory]) -> Result<(), PreviewError> {
    if categories.is_empty() {
        return Err(PreviewError::InvalidArgument("empty category list"));
    }
    if categories.iter().any(|c| !is_cleanable(*c)) {
        return Err(PreviewError::InvalidArgument("category is not cleanable"));
    }
    let mut seen = Vec::with_capacity(categories.len());
    for category in categories {
        if seen.contains(category) {
            return Err(PreviewError::InvalidArgument("repeated category"));
        }
        seen.push(*category);
    }
    Ok(())
}

fn position(category: JunkCategory) -> usize {
    JunkCategory::ALL
        .iter()
        .position(|known| *known == category)
        .unwrap_or(usize::MAX)
}

/// Arma el preview (D-03): categorias bloqueadas, guarda de 24 h con la fecha persistida y
/// totales. No hay E/S: toda verificacion contra el disco ocurre al confirmar.
pub fn build_preview(input: PreviewInput<'_>) -> Result<CleanupPreview, PreviewError> {
    validate_categories(input.categories)?;
    match input.last_done {
        Some(last) if last.scan.id == input.requested_scan => {}
        _ => return Err(PreviewError::ScanNotFound),
    }
    if input.cleaned_at.is_some() {
        return Err(PreviewError::ScanAlreadyCleaned);
    }

    let blocked = match scope_access(input.privileges, UserScope::PerUser) {
        ScopeAccess::Allowed => None,
        ScopeAccess::Blocked(reason) => Some(reason),
    };
    let mut requested: Vec<JunkCategory> = input.categories.to_vec();
    requested.sort_by_key(|category| position(*category));

    let mut candidates = input.candidates;
    candidates.sort_by_key(|stored| position(stored.finding.category));

    let mut categories = Vec::with_capacity(requested.len());
    let mut items = Vec::new();
    for category in requested {
        let mut result = PreviewCategory {
            category: category.into(),
            status: PreviewCategoryStatus::Included,
            item_count: 0,
            size_bytes: 0,
            recent_count: 0,
            recent_bytes: 0,
        };
        // Barrera 1: solo `PerUser` puro; la mixta (`crash_dumps`) sigue y cae en la barrera 2.
        if let (CategoryScope::PerUser, Some(reason)) = (category.scope(), blocked) {
            result.status = PreviewCategoryStatus::Blocked(reason);
            categories.push(result);
            continue;
        }
        for stored in candidates
            .iter()
            .filter(|stored| stored.finding.category == category)
        {
            let FindingState::Eligible(identity) = &stored.finding.state else {
                continue;
            };
            let finding = &stored.finding;
            if is_recent(finding.modified_at, input.now) {
                result.recent_count += 1;
                result.recent_bytes += finding.size_bytes;
                continue;
            }
            result.item_count += 1;
            result.size_bytes += finding.size_bytes;
            items.push(CleanupItem {
                finding_id: stored.id,
                category: category.into(),
                path: finding.path.clone(),
                size_bytes: finding.size_bytes,
                modified_at: finding.modified_at,
                identity: *identity,
            });
        }
        categories.push(result);
    }
    Ok(CleanupPreview {
        id: input.id,
        scan_id: input.requested_scan,
        created_at: input.now,
        categories,
        items,
    })
}

#[cfg(test)]
#[path = "preview_tests.rs"]
mod tests;

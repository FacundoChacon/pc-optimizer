//! Preview de las copias a eliminar (design D-12): funcion pura sobre filas persistidas.
//!
//! No recibe puertos de E/S: por construccion no puede tocar el disco ni abrir el contenido. La
//! verificacion contra el disco ocurre al confirmar (`guard`).

use std::collections::BTreeSet;

use crate::domain::cleanup::model::{
    CleanupCategory, CleanupItem, CleanupPreview, PreviewCategory, PreviewCategoryStatus,
    PreviewId, is_recent,
};
use crate::domain::cleanup::preview::assemble_preview;
use crate::domain::duplicates::decision::{
    Decision, DecisionError, ValidatedDecision, validate_decisions,
};
use crate::domain::duplicates::model::{
    DuplicateScanSummary, GroupResolution, StoredDuplicateGroup,
};
use crate::domain::model::{EpochMillis, ScanId};
use crate::domain::ports::FileIdentity;

/// Por que no se pudo armar el preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DuplicatePreviewError {
    /// El escaneo pedido no es el ultimo escaneo de duplicados `done`.
    #[error("scan not found")]
    ScanNotFound,
    #[error("invalid argument: {0}")]
    InvalidArgument(&'static str),
    #[error("duplicate group not found")]
    GroupNotFound,
    #[error("duplicate group already resolved")]
    GroupResolved,
    #[error("at least one copy must be kept")]
    KeepRequired,
}

impl From<DecisionError> for DuplicatePreviewError {
    fn from(error: DecisionError) -> Self {
        match error {
            DecisionError::InvalidArgument(reason) => Self::InvalidArgument(reason),
            DecisionError::GroupNotFound => Self::GroupNotFound,
            DecisionError::GroupResolved => Self::GroupResolved,
            DecisionError::KeepRequired => Self::KeepRequired,
        }
    }
}

/// Una copia que el usuario eligio conservar, tal como la persistio el escaneo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeptCopy {
    pub path: String,
    pub identity: FileIdentity,
    pub size_bytes: u64,
    pub modified: Option<EpochMillis>,
}

/// Plan de verificacion de un grupo (D-12 paso 7): lo que `DuplicateContentGuard` comprueba.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupPlan {
    pub group_id: i64,
    /// Hash completo del grupo (hex).
    pub blake3: String,
    pub size_bytes: u64,
    pub kept: Vec<KeptCopy>,
    pub kept_file_ids: Vec<i64>,
    /// Copias del preview (`CleanupItem::finding_id`) de este grupo, sin las excluidas por
    /// escritura reciente.
    pub item_file_ids: Vec<i64>,
}

/// Plan de verificacion de todo el preview.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DuplicatePlan {
    pub groups: Vec<GroupPlan>,
}

impl DuplicatePlan {
    /// Las resoluciones de los grupos con al menos un item procesado (D-17): los demas siguen
    /// abiertos y sin marcas.
    pub fn resolutions_for(&self, processed: &BTreeSet<i64>) -> (Vec<GroupResolution>, Vec<i64>) {
        let resolved: Vec<&GroupPlan> = self
            .groups
            .iter()
            .filter(|group| group.item_file_ids.iter().any(|id| processed.contains(id)))
            .collect();
        (
            resolved
                .iter()
                .map(|group| GroupResolution {
                    group_id: group.group_id,
                    kept_file_ids: group.kept_file_ids.clone(),
                })
                .collect(),
            resolved.iter().map(|group| group.group_id).collect(),
        )
    }
}

/// Preview de duplicados: el preview de limpieza estandar mas el plan de verificacion. Solo lo
/// crea [`build_duplicate_preview`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateCleanupPreview {
    preview: CleanupPreview,
    plan: DuplicatePlan,
}

impl DuplicateCleanupPreview {
    pub fn preview(&self) -> &CleanupPreview {
        &self.preview
    }

    pub fn plan(&self) -> &DuplicatePlan {
        &self.plan
    }

    /// Grupos incluidos en la decision (aunque alguno quede sin items por ser todos recientes).
    pub fn group_count(&self) -> u64 {
        self.plan.groups.len() as u64
    }

    /// Separa el preview (que `run_cleanup` consume por valor) del plan del guard.
    pub fn into_parts(self) -> (CleanupPreview, DuplicatePlan) {
        (self.preview, self.plan)
    }
}

/// Todo lo que necesita `build_duplicate_preview` (sin puertos de E/S).
pub struct DuplicatePreviewInput<'a> {
    /// Escaneo que pidio el usuario.
    pub requested_scan: ScanId,
    /// Ultimo escaneo de duplicados terminado, segun la persistencia.
    pub last_done: Option<&'a DuplicateScanSummary>,
    pub decisions: &'a [Decision],
    /// Los grupos de `requested_scan` que nombran las decisiones, con sus copias.
    pub groups: &'a [StoredDuplicateGroup],
    pub now: EpochMillis,
    pub id: PreviewId,
}

/// Arma el preview (D-12): valida las decisiones, deriva las copias a eliminar, aplica la guarda
/// de 24 h con la fecha persistida (por handle) y arma el plan de verificacion.
pub fn build_duplicate_preview(
    input: DuplicatePreviewInput<'_>,
) -> Result<DuplicateCleanupPreview, DuplicatePreviewError> {
    match input.last_done {
        Some(last) if last.scan.id == input.requested_scan => {}
        _ => return Err(DuplicatePreviewError::ScanNotFound),
    }
    let validated = validate_decisions(input.decisions, input.groups)?;

    let mut category = PreviewCategory {
        category: CleanupCategory::Duplicates,
        status: PreviewCategoryStatus::Included,
        item_count: 0,
        size_bytes: 0,
        recent_count: 0,
        recent_bytes: 0,
    };
    let mut items = Vec::new();
    let mut plan = DuplicatePlan::default();
    for ValidatedDecision {
        group,
        kept,
        delete,
    } in validated
    {
        let mut item_file_ids = Vec::new();
        for file in delete {
            if is_recent(file.file.modified, input.now) {
                category.recent_count += 1;
                category.recent_bytes += group.size_bytes;
                continue;
            }
            category.item_count += 1;
            category.size_bytes += group.size_bytes;
            item_file_ids.push(file.id);
            items.push(CleanupItem {
                finding_id: file.id,
                category: CleanupCategory::Duplicates,
                path: file.file.path.clone(),
                size_bytes: group.size_bytes,
                modified_at: file.file.modified,
                identity: file.file.identity,
            });
        }
        plan.groups.push(GroupPlan {
            group_id: group.id,
            blake3: group.blake3.clone(),
            size_bytes: group.size_bytes,
            kept_file_ids: kept.iter().map(|file| file.id).collect(),
            kept: kept
                .iter()
                .map(|file| KeptCopy {
                    path: file.file.path.clone(),
                    identity: file.file.identity,
                    size_bytes: group.size_bytes,
                    modified: file.file.modified,
                })
                .collect(),
            item_file_ids,
        });
    }
    let preview = assemble_preview(
        input.id,
        input.requested_scan,
        input.now,
        vec![category],
        items,
    );
    Ok(DuplicateCleanupPreview { preview, plan })
}

#[cfg(test)]
#[path = "preview_tests.rs"]
mod tests;

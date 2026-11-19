//! Modelo de la limpieza: preview, items y resumen (design D-28).
//!
//! Type-state: un [`CleanupPreview`] solo lo crea `build_preview` (sus campos son privados al
//! modulo `cleanup`) y `run_cleanup` lo consume por valor.

use crate::domain::junk::category::JunkCategory;
use crate::domain::model::{CleanupOutcome, EpochMillis, ScanId};
use crate::domain::ports::FileIdentity;
use crate::domain::privileges::MismatchReason;

/// Un archivo escrito hace menos de este tiempo no se mueve (D-04): 24 horas.
pub const RECENT_WRITE_GUARD_MS: i64 = 24 * 60 * 60 * 1000;

/// Un preview vence a los 10 minutos de creado (D-01).
pub const PREVIEW_TTL_MS: i64 = 10 * 60 * 1000;

/// Identificador de un preview: contador monotono del proceso, desde 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PreviewId(pub u64);

/// Un archivo escrito hace menos de 24 h, sin fecha o con fecha futura cuenta como reciente
/// (fail-closed, D-04).
pub fn is_recent(modified: Option<EpochMillis>, now: EpochMillis) -> bool {
    match modified {
        None => true,
        Some(at) => now.0.saturating_sub(at.0) < RECENT_WRITE_GUARD_MS,
    }
}

/// Fila a mover, tal como se mostro en el preview (copia de lo persistido).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanupItem {
    pub finding_id: i64,
    pub category: JunkCategory,
    /// Ruta real persistida por el escaneo.
    pub path: String,
    pub size_bytes: u64,
    /// Fecha persistida (la del listado de carpeta al escanear).
    pub modified_at: Option<EpochMillis>,
    pub identity: FileIdentity,
}

/// Estado de una categoria en el preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewCategoryStatus {
    Included,
    /// Categoria por-usuario con la sesion de otro usuario o sin determinar: sin items.
    Blocked(MismatchReason),
}

/// Una categoria del preview con sus totales.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewCategory {
    pub category: JunkCategory,
    pub status: PreviewCategoryStatus,
    pub item_count: u64,
    pub size_bytes: u64,
    /// Excluidos por escritura reciente (se informan, no se mueven).
    pub recent_count: u64,
    pub recent_bytes: u64,
}

/// Preview de una limpieza: exactamente lo que se movera si el usuario confirma.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanupPreview {
    pub(super) id: PreviewId,
    pub(super) scan_id: ScanId,
    pub(super) created_at: EpochMillis,
    pub(super) categories: Vec<PreviewCategory>,
    pub(super) items: Vec<CleanupItem>,
}

impl CleanupPreview {
    pub fn id(&self) -> PreviewId {
        self.id
    }

    pub fn scan_id(&self) -> ScanId {
        self.scan_id
    }

    pub fn created_at(&self) -> EpochMillis {
        self.created_at
    }

    pub fn expires_at(&self) -> EpochMillis {
        EpochMillis(self.created_at.0.saturating_add(PREVIEW_TTL_MS))
    }

    /// En el orden de `JunkCategory::ALL`.
    pub fn categories(&self) -> &[PreviewCategory] {
        &self.categories
    }

    /// Los items a mover, en el orden en que se procesan.
    pub fn items(&self) -> &[CleanupItem] {
        &self.items
    }

    pub fn total_items(&self) -> u64 {
        self.categories.iter().map(|c| c.item_count).sum()
    }

    pub fn total_bytes(&self) -> u64 {
        self.categories.iter().map(|c| c.size_bytes).sum()
    }
}

#[cfg(test)]
impl CleanupPreview {
    /// Preview armado a mano para probar la ejecucion (incluidas filas que `build_preview`
    /// excluiria, como una fecha reciente): las categorias salen de los items, todas incluidas.
    pub(crate) fn for_tests(
        id: PreviewId,
        scan_id: ScanId,
        created_at: EpochMillis,
        items: Vec<CleanupItem>,
    ) -> Self {
        let categories = JunkCategory::ALL
            .into_iter()
            .filter(|category| items.iter().any(|item| item.category == *category))
            .map(|category| {
                let mine = || items.iter().filter(|item| item.category == category);
                PreviewCategory {
                    category,
                    status: PreviewCategoryStatus::Included,
                    item_count: mine().count() as u64,
                    size_bytes: mine().map(|item| item.size_bytes).sum(),
                    recent_count: 0,
                    recent_bytes: 0,
                }
            })
            .collect();
        Self {
            id,
            scan_id,
            created_at,
            categories,
            items,
        }
    }
}

/// Por que se detuvo la ejecucion antes de procesar todos los items (D-17).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbortReason {
    /// Fallo `append_cleanup_log`: no se sigue moviendo sin auditoria.
    LogUnavailable,
    /// La Papelera no esta disponible en el volumen.
    RecycleBinUnavailable,
    /// El mecanismo de Papelera no puede usarse (COM, panico).
    TrashUnavailable,
}

impl AbortReason {
    /// Codigo estable que viaja por IPC.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LogUnavailable => "log_unavailable",
            Self::RecycleBinUnavailable => "recycle_bin_unavailable",
            Self::TrashUnavailable => "trash_unavailable",
        }
    }
}

/// Como termino la ejecucion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupStatus {
    Completed,
    /// El usuario cancelo: el item en curso termino y se registro.
    Cancelled,
    Aborted(AbortReason),
}

/// Cuantos items terminaron con un resultado y detalle (solo los que no se movieron).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReasonCount {
    pub outcome: CleanupOutcome,
    pub detail: String,
    pub count: u64,
}

/// Totales de una categoria en el resumen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CategorySummary {
    pub category: JunkCategory,
    pub moved_count: u64,
    pub moved_bytes: u64,
    /// `skipped_locked` + `skipped_protected` + `skipped_cloud`.
    pub skipped_count: u64,
    pub error_count: u64,
}

/// Resumen de una ejecucion: sin rutas, solo conteos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanupSummary {
    pub preview_id: PreviewId,
    pub scan_id: ScanId,
    pub status: CleanupStatus,
    pub started_at: EpochMillis,
    pub finished_at: EpochMillis,
    pub total_items: u64,
    /// Items con resultado (los que tienen entrada de log o se intento registrar).
    pub processed: u64,
    pub not_processed: u64,
    pub moved_count: u64,
    pub moved_bytes: u64,
    pub skipped_locked: u64,
    pub skipped_protected: u64,
    pub skipped_cloud: u64,
    pub errors: u64,
    /// Ordenado por cantidad descendente, luego por resultado y detalle.
    pub reasons: Vec<ReasonCount>,
    /// Una entrada por categoria del preview, en el orden del preview.
    pub categories: Vec<CategorySummary>,
}

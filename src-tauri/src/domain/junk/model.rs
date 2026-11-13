//! Resultado de la deteccion: hallazgos (archivos) y resumen por categoria (D-02, D-10).

use crate::domain::junk::category::{CategoryScope, JunkCategory};
use crate::domain::model::{EpochMillis, Scan};
use crate::domain::ports::FileIdentity;
use crate::domain::privileges::MismatchReason;

/// Estado de un hallazgo tras la verificacion de proteccion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FindingState {
    /// Elegible para limpiar; la identidad permite a C-09 detectar un archivo reemplazado.
    Eligible(FileIdentity),
    /// Omitido, con un codigo estable (`ProtectedReason::code()` o `skipped_cloud`).
    Skipped(String),
}

/// Un archivo hallado en una ubicacion de su categoria. Las carpetas nunca son hallazgos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JunkFinding {
    pub category: JunkCategory,
    /// Ruta real verificada si es elegible; la ruta listada si fue omitido.
    pub path: String,
    pub size_bytes: u64,
    pub modified_at: Option<EpochMillis>,
    pub state: FindingState,
}

impl JunkFinding {
    pub fn is_eligible(&self) -> bool {
        matches!(self.state, FindingState::Eligible(_))
    }
}

/// Estado de una categoria al terminar el escaneo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CategoryStatus {
    Done,
    /// Parte de sus ubicaciones estaba bloqueada por privilegios.
    Partial,
    /// Todas sus ubicaciones estaban bloqueadas por privilegios.
    Blocked,
    /// No se pudo evaluar (contexto no resoluble, consulta fallida o sin ubicaciones).
    Unavailable,
}

impl CategoryStatus {
    /// Cadena persistida (coincide con el `CHECK` de la migracion 002).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Done => "done",
            Self::Partial => "partial",
            Self::Blocked => "blocked",
            Self::Unavailable => "unavailable",
        }
    }

    /// Inverso de `as_str`.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "done" => Some(Self::Done),
            "partial" => Some(Self::Partial),
            "blocked" => Some(Self::Blocked),
            "unavailable" => Some(Self::Unavailable),
            _ => None,
        }
    }
}

/// Resumen de una categoria.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CategoryResult {
    pub category: JunkCategory,
    pub scope: CategoryScope,
    pub status: CategoryStatus,
    /// Solo con `Partial` y `Blocked`.
    pub block_reason: Option<MismatchReason>,
    /// Elegibles (en `RecycleBin`, los elementos de la Papelera).
    pub item_count: u64,
    /// Bytes elegibles (en `RecycleBin`, el tamano de la Papelera).
    pub size_bytes: u64,
    pub skipped_count: u64,
    pub inaccessible_count: u64,
}

impl CategoryResult {
    /// Categoria sin hallazgos: bloqueada, no disponible o sin nada que informar.
    pub fn empty(
        category: JunkCategory,
        status: CategoryStatus,
        block_reason: Option<MismatchReason>,
    ) -> Self {
        Self {
            category,
            scope: category.scope(),
            status,
            block_reason,
            item_count: 0,
            size_bytes: 0,
            skipped_count: 0,
            inaccessible_count: 0,
        }
    }
}

/// Resultado de un escaneo completo (sin cancelar).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JunkScanResults {
    pub findings: Vec<JunkFinding>,
    /// Una entrada por categoria de la whitelist, en el orden de `JunkCategory::ALL`.
    pub categories: Vec<CategoryResult>,
}

/// Elementos elegibles de un conjunto de categorias; la Papelera solo se mide y no suma.
pub fn total_items(categories: &[CategoryResult]) -> u64 {
    countable(categories).map(|c| c.item_count).sum()
}

/// Bytes elegibles de un conjunto de categorias; la Papelera no suma.
pub fn total_bytes(categories: &[CategoryResult]) -> u64 {
    countable(categories).map(|c| c.size_bytes).sum()
}

fn countable(categories: &[CategoryResult]) -> impl Iterator<Item = &CategoryResult> {
    categories
        .iter()
        .filter(|c| c.category != JunkCategory::RecycleBin)
}

impl JunkScanResults {
    pub fn total_items(&self) -> u64 {
        total_items(&self.categories)
    }

    pub fn total_bytes(&self) -> u64 {
        total_bytes(&self.categories)
    }
}

/// Hallazgo ya guardado, con su identificador de fila.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredFinding {
    pub id: i64,
    pub finding: JunkFinding,
}

/// Pagina de hallazgos de una categoria, ordenados por tamano descendente.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JunkFindingPage {
    /// Cantidad total de hallazgos de la categoria (no solo de la pagina).
    pub total: u64,
    pub items: Vec<StoredFinding>,
}

/// Resumen del ultimo escaneo de basura terminado (`done`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JunkScanSummary {
    pub scan: Scan,
    /// Una entrada por categoria, en el orden de `JunkCategory::ALL`.
    pub categories: Vec<CategoryResult>,
}

impl JunkScanSummary {
    pub fn total_items(&self) -> u64 {
        total_items(&self.categories)
    }

    pub fn total_bytes(&self) -> u64 {
        total_bytes(&self.categories)
    }
}

/// Como termina un escaneo: solo `Completed` puede persistirse como `done` (D-18).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JunkScanOutcome {
    Completed(JunkScanResults),
    /// Cancelado por el usuario: los hallazgos parciales se descartan.
    Cancelled,
}

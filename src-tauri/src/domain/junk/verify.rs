//! Verificacion de cada candidato con la proteccion de rutas (D-10).

use std::path::Path;

use crate::domain::junk::category::JunkCategory;
use crate::domain::junk::model::{FindingState, JunkFinding};
use crate::domain::ports::{DirEntry, EntryKind, FileSystem};
use crate::domain::protection::decision::ProtectionDecision;
use crate::domain::protection::service::CheckSession;

/// Codigo del motivo de omision de un archivo en la nube.
pub const SKIPPED_CLOUD: &str = "skipped_cloud";
/// Codigo del motivo de omision de un punto de reanalisis visto al listar.
pub const REPARSE_POINT: &str = "reparse_point";
/// Codigo cuando la verificacion devuelve algo que no es un archivo (la misma razon que C-03).
pub const RESOLVED_PATH_MISMATCH: &str = "resolved_path_mismatch";

/// Decide un archivo candidato. Nunca falla: todo problema es un hallazgo omitido con su motivo.
pub fn verify_candidate<F: FileSystem>(
    session: &CheckSession<'_, '_, F>,
    category: JunkCategory,
    entry: &DirEntry,
) -> JunkFinding {
    let skipped = |code: &str| JunkFinding {
        category,
        path: entry.path.display().to_string(),
        size_bytes: entry.meta.size_bytes,
        modified_at: entry.meta.modified,
        state: FindingState::Skipped(code.to_string()),
    };
    match session.check(&entry.path) {
        // Solo un archivo puede ser hallazgo: si entre el listado y la verificacion cambio de
        // tipo (carrera), se omite con el codigo de C-03 en lugar de inventar uno nuevo.
        ProtectionDecision::Allowed(checked) if checked.kind != EntryKind::File => {
            skipped(RESOLVED_PATH_MISMATCH)
        }
        ProtectionDecision::Allowed(checked) => JunkFinding {
            category,
            path: checked.path.to_string(),
            size_bytes: checked.size_bytes,
            modified_at: entry.meta.modified,
            state: FindingState::Eligible(checked.identity),
        },
        ProtectionDecision::SkippedCloud { .. } => skipped(SKIPPED_CLOUD),
        ProtectionDecision::Protected(reason) => skipped(reason.code()),
    }
}

/// Hallazgo omitido por ser un punto de reanalisis (no se resuelve ni se sigue).
pub fn reparse_point_finding(category: JunkCategory, path: &Path) -> JunkFinding {
    JunkFinding {
        category,
        path: path.display().to_string(),
        size_bytes: 0,
        modified_at: None,
        state: FindingState::Skipped(REPARSE_POINT.to_string()),
    }
}

#[cfg(test)]
#[path = "verify_tests.rs"]
mod tests;

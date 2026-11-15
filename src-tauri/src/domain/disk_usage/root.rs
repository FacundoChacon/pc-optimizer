//! Resolucion de la raiz de un analisis (C-07, design D-01 y D-03): el volumen del sistema sale
//! del contexto del backend y una carpeta elegida se valida con `check_analysis_root`. Quien
//! recibe un `AnalysisRoot` sabe que paso por esa validacion: solo se construye aca.

use std::path::PathBuf;

use crate::domain::disk_usage::model::RootKind;
use crate::domain::ports::FileSystem;
use crate::domain::protection::context::{ContextError, ProtectionContext};
use crate::domain::protection::decision::{ProtectedReason, ResolveFailure};
use crate::domain::protection::path::{NormalizedPath, normalize_lexical};
use crate::domain::protection::service::PathProtection;

/// Que analizar, ya traducido por el comando.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RootChoice {
    SystemDrive,
    /// Ruta elegida con el selector; se vuelve a validar al iniciar.
    Picked(NormalizedPath),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FolderScanError {
    #[error("analysis root rejected: {}", rejection_code(*.0))]
    RootRejected(ProtectedReason),
}

/// Raiz de analisis validada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalysisRoot {
    kind: RootKind,
    path: NormalizedPath,
}

impl AnalysisRoot {
    pub fn kind(&self) -> RootKind {
        self.kind
    }

    pub fn path(&self) -> &NormalizedPath {
        &self.path
    }

    pub fn to_path_buf(&self) -> PathBuf {
        self.path.to_path_buf()
    }
}

impl std::fmt::Display for AnalysisRoot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.path.fmt(f)
    }
}

/// Codigo estable del motivo de rechazo de una raiz: los motivos comunes de una carpeta que no
/// sirve se simplifican (`not_a_directory`, `not_found`); el resto es el codigo de proteccion.
pub fn rejection_code(reason: ProtectedReason) -> &'static str {
    match reason {
        ProtectedReason::Unresolvable(ResolveFailure::NotADirectory) => "not_a_directory",
        ProtectedReason::Unresolvable(ResolveFailure::NotFound) => "not_found",
        other => other.code(),
    }
}

/// Valida la raiz pedida contra el contexto de proteccion (ya restringido por privilegios).
///
/// `system_drive` es la raiz del volumen del sistema del contexto, nunca una ruta de la interfaz;
/// una carpeta elegida pasa por `check_analysis_root` (existe, es carpeta, sin enlaces, del
/// volumen del sistema y no de otro usuario).
pub fn validate_root<F: FileSystem>(
    fs: &F,
    protection: &Result<ProtectionContext, ContextError>,
    choice: &RootChoice,
) -> Result<AnalysisRoot, FolderScanError> {
    let ctx = protection
        .as_ref()
        .map_err(|_| FolderScanError::RootRejected(ProtectedReason::ContextUnavailable))?;
    let (kind, requested) = match choice {
        RootChoice::SystemDrive => {
            let drive = format!(r"{}:\", ctx.system_drive());
            let path = normalize_lexical(&drive)
                .map_err(|_| FolderScanError::RootRejected(ProtectedReason::ContextUnavailable))?;
            (RootKind::SystemDrive, path)
        }
        RootChoice::Picked(path) => (RootKind::Picked, path.clone()),
    };
    let service = PathProtection::new(fs, Ok(ctx.clone()));
    let path = service
        .check_analysis_root(&requested.to_path_buf())
        .map_err(FolderScanError::RootRejected)?;
    Ok(AnalysisRoot { kind, path })
}

#[cfg(test)]
#[path = "root_tests.rs"]
mod tests;

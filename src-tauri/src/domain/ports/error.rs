//! Error comun de los puertos de SO.

/// Fallo al usar un puerto (sistema de archivos, Papelera, sensores).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PortError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("in use: {0}")]
    InUse(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("i/o error: {0}")]
    Io(String),
}

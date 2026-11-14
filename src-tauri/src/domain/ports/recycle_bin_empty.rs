//! Puerto de vaciado de la Papelera (C-10): la unica accion permanente del producto.
//!
//! Vive aparte de [`RecycleBinInfo`](super::RecycleBinInfo) (solo lectura) a proposito: quien solo
//! necesita medir no puede vaciar. El contrato solo lo usa el flujo de vaciado (lista cerrada en
//! `tests/architecture.rs`).

use crate::domain::recycle_bin::root::VolumeRoot;

/// Por que el shell no pudo vaciar la Papelera.
///
/// `Other` lleva un codigo ASCII estable (`io_0x8000ffff`), nunca la descripcion del sistema.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EmptyError {
    #[error("permission denied")]
    PermissionDenied,
    #[error("recycle bin emptying mechanism unusable")]
    BackendUnavailable,
    #[error("other failure: {0}")]
    Other(String),
}

impl EmptyError {
    /// Codigo estable `snake_case` que se registra como `error_detail` (D-08).
    pub fn code(&self) -> &str {
        match self {
            Self::PermissionDenied => "permission_denied",
            Self::BackendUnavailable => "empty_unavailable",
            Self::Other(code) => code,
        }
    }
}

/// Vacia PARA SIEMPRE la Papelera de un volumen. Irreversible: solo se llama con un
/// `AcknowledgedEmpty` (dos confirmaciones) desde `run_empty_recycle_bin`.
pub trait RecycleBinEmptier {
    /// Una sola llamada bloqueante; vacia lo que haya en la Papelera de `root` en ese instante.
    /// El resultado real se decide midiendo despues (`classify_empty`), no por este `Result`.
    fn empty(&self, root: &VolumeRoot) -> Result<(), EmptyError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_should_be_a_stable_ascii_snake_case_string_per_variant() {
        assert_eq!(EmptyError::PermissionDenied.code(), "permission_denied");
        assert_eq!(EmptyError::BackendUnavailable.code(), "empty_unavailable");
        assert_eq!(
            EmptyError::Other("io_0x8000ffff".to_owned()).code(),
            "io_0x8000ffff"
        );
    }
}

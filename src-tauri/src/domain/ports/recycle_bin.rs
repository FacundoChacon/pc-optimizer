//! Puerto de consulta de la Papelera de reciclaje (solo lectura).

use std::path::Path;

use crate::domain::ports::error::PortError;

/// Cantidad de elementos y tamano total de una Papelera.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecycleBinUsage {
    pub items: u64,
    pub bytes: u64,
}

/// Consulta de la Papelera del usuario del proceso. Su unica operacion mide: no lista,
/// restaura, borra ni vacia elementos (el vaciado es C-10 y vive en otro contrato).
pub trait RecycleBinInfo {
    /// Elementos y bytes de la Papelera del volumen cuya raiz es `volume_root` (`C:\`).
    fn query(&self, volume_root: &Path) -> Result<RecycleBinUsage, PortError>;
}

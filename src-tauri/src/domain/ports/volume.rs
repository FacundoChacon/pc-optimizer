//! Puerto de espacio del volumen (C-07, D-06): total y libre de la unidad de una ruta.

use std::path::Path;

use crate::domain::ports::error::PortError;

/// Espacio de un volumen en bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VolumeSpaceInfo {
    pub total_bytes: u64,
    pub free_bytes: u64,
}

/// Consulta de espacio del volumen que contiene `root`. Solo lectura.
pub trait VolumeSpace {
    fn space(&self, root: &Path) -> Result<VolumeSpaceInfo, PortError>;
}

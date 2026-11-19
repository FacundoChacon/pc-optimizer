//! Puerto de lectura de contenido de archivos (C-11, design D-06).
//!
//! Separado de `FileSystem` (metadatos) y de `Trash`: solo permite ABRIR y LEER. Un archivo se
//! abre sin seguir puntos de reanalisis, sin provocar la recuperacion de contenido de la nube y
//! sin impedir que otros procesos lo lean, escriban o borren. No hay operaciones de escritura,
//! renombre ni borrado.

use std::path::Path;

use crate::domain::ports::error::PortError;
use crate::domain::ports::fs::FileMeta;

/// Abre archivos para leer su contenido.
pub trait ContentReader {
    /// Abre `path` sin seguir el ultimo componente (un enlace se abre como enlace) y con
    /// comparticion total. `InUse` si otro proceso lo tiene abierto sin compartir lectura.
    fn open(&self, path: &Path) -> Result<Box<dyn ContentHandle>, PortError>;
}

/// Un archivo abierto para lectura.
pub trait ContentHandle {
    /// Metadatos leidos del archivo ABIERTO (identidad, tamano, ultima escritura, nube, tipo).
    fn meta(&self) -> Result<FileMeta, PortError>;
    /// Lee hasta `buf.len()` bytes desde `offset`. Devuelve 0 si `offset` esta en o mas alla del
    /// final. Puede devolver menos que `buf.len()` antes del final.
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<usize, PortError>;
}

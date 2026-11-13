//! Puerto de lectura del sistema de archivos.

use std::path::{Path, PathBuf};

use crate::domain::model::EpochMillis;
use crate::domain::ports::error::PortError;

/// Tipo de entrada. `ReparsePoint` agrupa junctions, symlinks y similares; no se siguen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Directory,
    ReparsePoint,
}

/// Metadatos de una entrada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMeta {
    pub kind: EntryKind,
    pub size_bytes: u64,
    pub modified: Option<EpochMillis>,
    /// Atributos RECALL_ON_DATA_ACCESS / RECALL_ON_OPEN / OFFLINE (archivo solo en la nube).
    pub cloud_placeholder: bool,
}

/// Hijo directo de un directorio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub path: PathBuf,
    pub meta: FileMeta,
}

/// Lectura del sistema de archivos.
pub trait FileSystem {
    /// Ruta canonica; falla si no existe.
    fn canonicalize(&self, path: &Path) -> Result<PathBuf, PortError>;
    /// Metadatos sin seguir reparse points.
    fn metadata(&self, path: &Path) -> Result<FileMeta, PortError>;
    /// Hijos directos, sin seguir reparse points.
    fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>, PortError>;
}

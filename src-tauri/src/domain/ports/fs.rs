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

/// Identidad estable de un archivo: volumen + id de archivo (128 bits, como `FILE_ID_INFO`).
///
/// Dos nombres del mismo archivo (hard links) comparten identidad; un archivo recreado con el
/// mismo nombre tiene otra.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileIdentity {
    pub volume_serial: u64,
    pub file_id: u128,
}

/// Metadatos de una entrada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMeta {
    pub kind: EntryKind,
    pub size_bytes: u64,
    pub modified: Option<EpochMillis>,
    /// `RECALL_ON_DATA_ACCESS` | `RECALL_ON_OPEN` | `OFFLINE`, o tag de reparse de un proveedor
    /// de nube (`IO_REPARSE_TAG_CLOUD*`), aunque el contenido este hidratado. Un reparse de nube se
    /// informa ademas con `kind = ReparsePoint`; el dominio da prioridad a la nube.
    pub cloud_placeholder: bool,
    /// `Some` en `metadata()`; `read_dir()` puede devolver `None`.
    pub identity: Option<FileIdentity>,
    /// Cantidad de nombres (hard links) del archivo. `Some` en `metadata()`; `read_dir()` puede
    /// devolver `None`.
    pub hard_links: Option<u32>,
}

/// Hijo directo de un directorio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub path: PathBuf,
    pub meta: FileMeta,
}

/// Fijacion de los ancestros de una ruta: mientras vive, nadie puede renombrar, borrar ni
/// reemplazar esos directorios. Se libera al soltarla (`Drop`).
pub trait AncestorPin: Send {}

/// Lectura del sistema de archivos.
pub trait FileSystem {
    /// Ruta canonica con nombres largos (sin 8.3), los reparse intermedios resueltos y el ultimo
    /// componente SIN seguir. Falla si no existe o si la ruta real no tiene unidad (volumen sin
    /// letra, UNC...).
    fn canonicalize(&self, path: &Path) -> Result<PathBuf, PortError>;
    /// Metadatos sin seguir reparse points.
    fn metadata(&self, path: &Path) -> Result<FileMeta, PortError>;
    /// Hijos directos, sin seguir reparse points.
    fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>, PortError>;
    /// Fija cada ancestro de `path` (de la raiz del volumen, excluida, hasta el padre) contra
    /// renombres y borrados, verificando que ninguno sea reparse point. Si algo falla no queda
    /// nada fijado.
    fn pin_ancestors(&self, path: &Path) -> Result<Box<dyn AncestorPin>, PortError>;
}

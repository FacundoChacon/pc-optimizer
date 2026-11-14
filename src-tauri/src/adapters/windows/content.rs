//! `WindowsContentReader`: implementacion real del puerto `ContentReader` (C-11, design D-06).
//!
//! Abre cada archivo con `std::fs::OpenOptions` (asi `std` resuelve las rutas largas y el `File`
//! cierra el handle en `Drop`) con el acceso minimo para leer, comparticion total (nunca bloquea
//! al usuario ni a la Papelera) y sin seguir el ultimo componente ni recuperar contenido de la
//! nube. Los metadatos se leen del handle abierto con el mismo codigo que
//! `WindowsFileSystem::metadata` y la lectura por offset es `seek_read` de `std` (`ReadFile` con
//! `OVERLAPPED`), de modo que este modulo no agrega `unsafe`.

use std::fs::{File, OpenOptions};
use std::io;
use std::os::windows::fs::{FileExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use windows::Win32::Storage::FileSystem::{
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_NO_RECALL, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_FLAG_SEQUENTIAL_SCAN, FILE_READ_ATTRIBUTES, FILE_READ_DATA,
};

use super::fs::{SHARE_ALL, handle_meta, map_io_error, require_absolute};
use crate::domain::ports::{ContentHandle, ContentReader, FileMeta, PortError};

/// Lector de contenido real de Windows. Sin estado.
#[derive(Debug, Clone, Copy, Default)]
pub struct WindowsContentReader;

/// Archivo abierto para lectura.
struct WindowsContentHandle {
    file: File,
    path: PathBuf,
}

/// Abre `path` solo para leer: sin seguir el ultimo componente (`FILE_FLAG_OPEN_REPARSE_POINT`;
/// `FILE_FLAG_BACKUP_SEMANTICS` permite abrir una junction como enlace), sin recuperar contenido
/// de la nube (`FILE_FLAG_OPEN_NO_RECALL`) y con lectura secuencial como pista.
fn open_for_read(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .access_mode(FILE_READ_DATA.0 | FILE_READ_ATTRIBUTES.0)
        .share_mode(SHARE_ALL)
        .custom_flags(
            FILE_FLAG_OPEN_REPARSE_POINT.0
                | FILE_FLAG_OPEN_NO_RECALL.0
                | FILE_FLAG_SEQUENTIAL_SCAN.0
                | FILE_FLAG_BACKUP_SEMANTICS.0,
        )
        .open(path)
}

impl ContentReader for WindowsContentReader {
    fn open(&self, path: &Path) -> Result<Box<dyn ContentHandle>, PortError> {
        require_absolute(path)?;
        let file = open_for_read(path).map_err(|e| map_io_error(path, &e))?;
        Ok(Box::new(WindowsContentHandle {
            file,
            path: path.to_path_buf(),
        }))
    }
}

impl ContentHandle for WindowsContentHandle {
    fn meta(&self) -> Result<FileMeta, PortError> {
        handle_meta(&self.file).map_err(|e| map_io_error(&self.path, &e))
    }

    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<usize, PortError> {
        // `seek_read` devuelve 0 en el final de archivo (`ERROR_HANDLE_EOF` ya traducido).
        self.file
            .seek_read(buf, offset)
            .map_err(|e| map_io_error(&self.path, &e))
    }
}

#[cfg(test)]
#[path = "content_tests.rs"]
mod tests;

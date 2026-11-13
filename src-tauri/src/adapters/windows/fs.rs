//! `WindowsFileSystem`: implementacion real del puerto `FileSystem` (design D-10).
//!
//! Cada entrada se abre con `std::fs::OpenOptions` (asi `std` resuelve las rutas largas y el
//! `File` cierra el handle en `Drop`) y se consulta por handle. Nunca se sigue el ultimo
//! componente (`FILE_FLAG_OPEN_REPARSE_POINT`). El unico `unsafe` es la consulta por handle.

use std::ffi::{OsString, c_void};
use std::fs::{File, OpenOptions};
use std::io;
use std::os::windows::ffi::OsStringExt;
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};

use windows::Win32::Foundation::HANDLE;
use windows::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_OFFLINE, FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS,
    FILE_ATTRIBUTE_RECALL_ON_OPEN, FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_TAG_INFO,
    FILE_BASIC_INFO, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_ID_INFO,
    FILE_INFO_BY_HANDLE_CLASS, FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE,
    FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_STANDARD_INFO, FileAttributeTagInfo, FileBasicInfo,
    FileIdInfo, FileStandardInfo, GetFileInformationByHandleEx, GetFinalPathNameByHandleW,
    VOLUME_NAME_DOS,
};

use crate::domain::model::EpochMillis;
use crate::domain::ports::{
    AncestorPin, DirEntry, EntryKind, FileIdentity, FileMeta, FileSystem, PortError,
};

/// Sistema de archivos real de Windows. Sin estado.
#[derive(Debug, Clone, Copy, Default)]
pub struct WindowsFileSystem;

/// Tag base de los reparse points de proveedores de nube (`IO_REPARSE_TAG_CLOUD`).
const IO_REPARSE_TAG_CLOUD: u32 = 0x9000_001A;
/// Bits variables (0..=15 de la parte baja) que distinguen `IO_REPARSE_TAG_CLOUD_1..F`.
const IO_REPARSE_TAG_CLOUD_MASK: u32 = 0x0000_F000;
const CLOUD_ATTRIBUTES: u32 = FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS.0
    | FILE_ATTRIBUTE_RECALL_ON_OPEN.0
    | FILE_ATTRIBUTE_OFFLINE.0;
/// Diferencia entre la epoca de FILETIME (1601) y la Unix (1970), en milisegundos.
const FILETIME_UNIX_DELTA_MS: i64 = 11_644_473_600_000;
const SHARE_ALL: u32 = FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0 | FILE_SHARE_DELETE.0;
/// Reintentos de `GetFinalPathNameByHandleW` si la ruta crece entre llamadas.
const FINAL_PATH_ATTEMPTS: usize = 4;

/// Traduce un error de E/S de Win32 al error del puerto (D-10). El detalle lleva ruta y codigo.
fn map_io_error(path: &Path, error: &io::Error) -> PortError {
    let detail = match error.raw_os_error() {
        Some(code) => format!("{} (os error {code})", path.display()),
        None => format!("{} ({error})", path.display()),
    };
    match error.raw_os_error() {
        Some(2 | 3) => PortError::NotFound(detail),
        Some(5) => PortError::PermissionDenied(detail),
        Some(32 | 33) => PortError::InUse(detail),
        Some(1 | 50 | 87) => PortError::Unsupported(detail),
        _ => PortError::Io(detail),
    }
}

/// Tipo y marca de nube a partir de atributos y, si es reparse, su tag.
///
/// Un reparse cuyo tag no se pudo leer sigue siendo `ReparsePoint` (el dominio lo protege).
fn classify(attributes: u32, reparse_tag: Option<u32>) -> (EntryKind, bool) {
    let is_reparse = attributes & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0;
    let cloud_tag = is_reparse
        && reparse_tag.is_some_and(|tag| tag & !IO_REPARSE_TAG_CLOUD_MASK == IO_REPARSE_TAG_CLOUD);
    let cloud = cloud_tag || attributes & CLOUD_ATTRIBUTES != 0;
    let kind = if is_reparse {
        EntryKind::ReparsePoint
    } else if attributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0 {
        EntryKind::Directory
    } else {
        EntryKind::File
    };
    (kind, cloud)
}

/// FILETIME (100 ns desde 1601) a milisegundos Unix; `None` si es cero o anterior a 1970.
fn filetime_to_millis(filetime: i64) -> Option<EpochMillis> {
    let millis = (filetime / 10_000).checked_sub(FILETIME_UNIX_DELTA_MS)?;
    (filetime > 0 && millis >= 0).then_some(EpochMillis(millis))
}

/// Convierte la ruta final de Win32 (`\\?\C:\...`) en una ruta de unidad; todo lo demas
/// (UNC, `Volume{GUID}`, dispositivos) no tiene letra de unidad y se rechaza.
fn drive_path(final_path: &Path) -> Result<PathBuf, PortError> {
    let text = final_path.to_str().ok_or_else(|| {
        PortError::Unsupported(format!("ruta no Unicode: {}", final_path.display()))
    })?;
    let stripped = text.strip_prefix(r"\\?\").unwrap_or(text);
    let bytes = stripped.as_bytes();
    let has_drive =
        bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'\\';
    if has_drive || (bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':') {
        Ok(PathBuf::from(stripped))
    } else {
        Err(PortError::Unsupported(format!(
            "la ruta real no tiene letra de unidad: {text}"
        )))
    }
}

/// Las rutas deben ser absolutas con unidad: una relativa dependeria del directorio actual.
fn require_absolute(path: &Path) -> Result<(), PortError> {
    if path.is_absolute() {
        Ok(())
    } else {
        Err(PortError::Unsupported(format!(
            "ruta no absoluta: {}",
            path.display()
        )))
    }
}

/// Abre `path` sin seguir el ultimo componente, con el acceso y la comparticion indicados.
fn open_entry(path: &Path, access: u32, share: u32) -> io::Result<File> {
    OpenOptions::new()
        .access_mode(access)
        .share_mode(share)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0 | FILE_FLAG_BACKUP_SEMANTICS.0)
        .open(path)
}

fn win32_error(error: &windows::core::Error) -> io::Error {
    // HRESULT_FROM_WIN32: el codigo Win32 esta en los 16 bits bajos.
    io::Error::from_raw_os_error(error.code().0 & 0xFFFF)
}

/// Lee una estructura de informacion de `file` (`GetFileInformationByHandleEx`).
///
/// Solo se instancia con las estructuras `FILE_*_INFO` de `windows`, que son datos planos
/// `repr(C)` validos con cualquier patron de bits.
fn query_info<T: Default>(file: &File, class: FILE_INFO_BY_HANDLE_CLASS) -> io::Result<T> {
    let mut info = T::default();
    let size = u32::try_from(size_of::<T>()).map_err(|_| io::Error::other("estructura enorme"))?;
    // SAFETY: el handle pertenece a `file`, que esta prestado y por tanto abierto durante la
    // llamada; `info` es un `T` inicializado, alineado y escribible de exactamente `size`
    // bytes, y `T` es un struct `repr(C)` de datos planos (ver arriba), asi que cualquier
    // contenido que escriba el SO es valido.
    unsafe {
        GetFileInformationByHandleEx(
            HANDLE(file.as_raw_handle()),
            class,
            (&raw mut info).cast::<c_void>(),
            size,
        )
    }
    .map_err(|error| win32_error(&error))?;
    Ok(info)
}

/// Ruta final (`\\?\...`) del handle, con los reparse intermedios resueltos.
fn final_path(file: &File) -> io::Result<PathBuf> {
    let mut buffer = vec![0u16; 512];
    for _ in 0..FINAL_PATH_ATTEMPTS {
        // SAFETY: el handle pertenece a `file` (abierto durante la llamada) y `buffer` es un
        // slice escribible cuya longitud real recibe la API; no se lee mas alla de lo escrito.
        let written = unsafe {
            GetFinalPathNameByHandleW(HANDLE(file.as_raw_handle()), &mut buffer, VOLUME_NAME_DOS)
        } as usize;
        if written == 0 {
            return Err(io::Error::last_os_error());
        }
        if written < buffer.len() {
            return Ok(PathBuf::from(OsString::from_wide(&buffer[..written])));
        }
        // `written` es el tamano requerido, con el nulo final.
        buffer.resize(written + 1, 0);
    }
    Err(io::Error::other("la ruta cambia de tamano entre llamadas"))
}

/// Tag de reparse de `path` (sin seguirlo), si se puede leer.
fn reparse_tag(path: &Path) -> Option<u32> {
    let file = open_entry(path, FILE_READ_ATTRIBUTES.0, SHARE_ALL).ok()?;
    let info: FILE_ATTRIBUTE_TAG_INFO = query_info(&file, FileAttributeTagInfo).ok()?;
    Some(info.ReparseTag)
}

impl FileSystem for WindowsFileSystem {
    fn canonicalize(&self, path: &Path) -> Result<PathBuf, PortError> {
        require_absolute(path)?;
        let file = open_entry(path, FILE_READ_ATTRIBUTES.0, SHARE_ALL)
            .map_err(|e| map_io_error(path, &e))?;
        let real = final_path(&file).map_err(|e| map_io_error(path, &e))?;
        drive_path(&real)
    }

    fn metadata(&self, path: &Path) -> Result<FileMeta, PortError> {
        require_absolute(path)?;
        let io = |e: io::Error| map_io_error(path, &e);
        let file = open_entry(path, FILE_READ_ATTRIBUTES.0, SHARE_ALL).map_err(io)?;
        let tag: FILE_ATTRIBUTE_TAG_INFO = query_info(&file, FileAttributeTagInfo).map_err(io)?;
        let basic: FILE_BASIC_INFO = query_info(&file, FileBasicInfo).map_err(io)?;
        let standard: FILE_STANDARD_INFO = query_info(&file, FileStandardInfo).map_err(io)?;
        // Sin identidad (volumen que no la informa) el dominio decide: protege.
        let identity = query_info::<FILE_ID_INFO>(&file, FileIdInfo)
            .ok()
            .map(|info| FileIdentity {
                volume_serial: info.VolumeSerialNumber,
                file_id: u128::from_le_bytes(info.FileId.Identifier),
            });
        let (kind, cloud_placeholder) = classify(tag.FileAttributes, Some(tag.ReparseTag));
        Ok(FileMeta {
            kind,
            size_bytes: u64::try_from(standard.EndOfFile).unwrap_or(0),
            modified: filetime_to_millis(basic.LastWriteTime),
            cloud_placeholder,
            identity,
            hard_links: Some(standard.NumberOfLinks),
        })
    }

    fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>, PortError> {
        match self.metadata(path)?.kind {
            // El contenido de un reparse point no se expone: el destino se lista por su ruta.
            EntryKind::ReparsePoint => return Ok(Vec::new()),
            EntryKind::File => {
                return Err(PortError::Io(format!(
                    "not a directory: {}",
                    path.display()
                )));
            }
            EntryKind::Directory => {}
        }
        let io = |e: io::Error| map_io_error(path, &e);
        let mut children = Vec::new();
        for entry in std::fs::read_dir(path).map_err(io)? {
            let entry = entry.map_err(io)?;
            // Con los datos de la enumeracion: no sigue enlaces.
            let metadata = entry.metadata().map_err(io)?;
            let attributes = metadata.file_attributes();
            let child = entry.path();
            let tag = (attributes & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0)
                .then(|| reparse_tag(&child))
                .flatten();
            let (kind, cloud_placeholder) = classify(attributes, tag);
            children.push(DirEntry {
                meta: FileMeta {
                    kind,
                    size_bytes: if kind == EntryKind::File {
                        metadata.file_size()
                    } else {
                        0
                    },
                    modified: i64::try_from(metadata.last_write_time())
                        .ok()
                        .and_then(filetime_to_millis),
                    cloud_placeholder,
                    identity: None,
                    hard_links: None,
                },
                path: child,
            });
        }
        Ok(children)
    }

    fn pin_ancestors(&self, path: &Path) -> Result<Box<dyn AncestorPin>, PortError> {
        require_absolute(path)?;
        let mut ancestors: Vec<&Path> = path
            .ancestors()
            .skip(1)
            .filter(|ancestor| ancestor.parent().is_some())
            .collect();
        ancestors.reverse();
        let mut handles = Vec::with_capacity(ancestors.len());
        for ancestor in ancestors {
            let io = |e: io::Error| map_io_error(ancestor, &e);
            // `FILE_LIST_DIRECTORY` participa del control de comparticion y, sin
            // `FILE_SHARE_DELETE`, nadie puede abrir el directorio con `DELETE`: no se renombra,
            // borra ni reemplaza mientras viva el handle.
            let file = open_entry(
                ancestor,
                FILE_LIST_DIRECTORY.0 | FILE_READ_ATTRIBUTES.0,
                FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0,
            )
            .map_err(io)?;
            let info: FILE_ATTRIBUTE_TAG_INFO =
                query_info(&file, FileAttributeTagInfo).map_err(io)?;
            match classify(info.FileAttributes, Some(info.ReparseTag)).0 {
                EntryKind::Directory => handles.push(file),
                EntryKind::ReparsePoint => {
                    return Err(PortError::Io(format!(
                        "reparse point: {}",
                        ancestor.display()
                    )));
                }
                EntryKind::File => {
                    return Err(PortError::Io(format!(
                        "not a directory: {}",
                        ancestor.display()
                    )));
                }
            }
        }
        Ok(Box::new(WindowsPin { _handles: handles }))
    }
}

/// Handles abiertos sobre los ancestros; se cierran al soltar (`Drop` de `File`).
/// Si algo falla antes de construirla, los handles ya abiertos se sueltan con el `Vec`.
struct WindowsPin {
    _handles: Vec<File>,
}

impl AncestorPin for WindowsPin {}

#[cfg(test)]
#[path = "fs_tests.rs"]
mod tests;

//! `WindowsVolumeSpace`: espacio total y libre del volumen de una ruta (C-07, D-06).
//!
//! Solo lectura: una llamada a `GetDiskFreeSpaceExW`. Unico `unsafe`: esa llamada.

use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
use windows::core::PCWSTR;

use crate::domain::ports::{PortError, VolumeSpace, VolumeSpaceInfo};

/// Consulta de espacio del volumen. Sin estado.
#[derive(Debug, Clone, Copy, Default)]
pub struct WindowsVolumeSpace;

/// Traduce un `HRESULT` de Win32 (`0x8007xxxx`) al error del puerto. El detalle lleva la ruta y
/// el codigo, nunca el texto del sistema.
pub(crate) fn map_hresult(hresult: i32, path: &Path) -> PortError {
    let raw = u32::from_ne_bytes(hresult.to_ne_bytes());
    let detail = format!("{} (0x{raw:08x})", path.display());
    if raw >> 16 != 0x8007 {
        return PortError::Io(detail);
    }
    match raw & 0xFFFF {
        2 | 3 => PortError::NotFound(detail),
        5 => PortError::PermissionDenied(detail),
        _ => PortError::Io(detail),
    }
}

impl VolumeSpace for WindowsVolumeSpace {
    fn space(&self, root: &Path) -> Result<VolumeSpaceInfo, PortError> {
        if root.as_os_str().is_empty() {
            return Err(PortError::NotFound("empty path".into()));
        }
        let mut wide: Vec<u16> = root.as_os_str().encode_wide().collect();
        wide.push(0);
        let mut total: u64 = 0;
        let mut free: u64 = 0;
        // SAFETY: `wide` es un buffer UTF-16 terminado en nulo que vive hasta despues de la
        // llamada; `total` y `free` son `u64` locales validos para escritura y la API no retiene
        // ninguno de los punteros. El parametro de "libre para el llamador" se omite.
        let result = unsafe {
            GetDiskFreeSpaceExW(
                PCWSTR(wide.as_ptr()),
                None,
                Some(&mut total),
                Some(&mut free),
            )
        };
        result.map_err(|error| map_hresult(error.code().0, root))?;
        Ok(VolumeSpaceInfo {
            total_bytes: total,
            free_bytes: free,
        })
    }
}

#[cfg(test)]
#[path = "volume_tests.rs"]
mod tests;

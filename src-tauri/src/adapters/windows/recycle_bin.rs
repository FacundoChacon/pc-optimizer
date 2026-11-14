//! Medicion de la Papelera con `SHQueryRecycleBinW` (C-08, D-08).
//!
//! Solo mide (elementos y bytes): no lista, restaura ni vacia. Unico archivo de C-08 con
//! `unsafe`; el bloque es uno solo y minimo, y la conversion de los contadores es pura.

use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows::Win32::UI::Shell::{SHQUERYRBINFO, SHQueryRecycleBinW};
use windows::core::PCWSTR;

use super::privileges::port_error;
use crate::domain::ports::{PortError, RecycleBinInfo, RecycleBinUsage};

/// Papelera real del volumen indicado (por el token del proceso).
#[derive(Debug, Default, Clone, Copy)]
pub struct WindowsRecycleBin;

/// Convierte los contadores `i64` de `SHQUERYRBINFO` en un uso valido.
///
/// Un valor negativo no es un tamano posible: se rechaza en vez de envolverlo con `as`.
pub(crate) fn usage_from_raw(
    size_bytes: i64,
    item_count: i64,
) -> Result<RecycleBinUsage, PortError> {
    match (u64::try_from(size_bytes), u64::try_from(item_count)) {
        (Ok(bytes), Ok(items)) => Ok(RecycleBinUsage { items, bytes }),
        _ => Err(PortError::Io(format!(
            "SHQueryRecycleBinW: negative counters (size {size_bytes}, items {item_count})"
        ))),
    }
}

impl RecycleBinInfo for WindowsRecycleBin {
    fn query(&self, volume_root: &Path) -> Result<RecycleBinUsage, PortError> {
        let mut root: Vec<u16> = volume_root.as_os_str().encode_wide().collect();
        if root.contains(&0) {
            return Err(PortError::Io("recycle bin root contains a NUL".to_owned()));
        }
        root.push(0);
        let mut info = SHQUERYRBINFO {
            cbSize: u32::try_from(size_of::<SHQUERYRBINFO>())
                .map_err(|_| PortError::Io("SHQUERYRBINFO size overflow".to_owned()))?,
            ..SHQUERYRBINFO::default()
        };
        // SAFETY: `root` es un buffer UTF-16 terminado en nulo que vive hasta despues de la
        // llamada; `info` es un `SHQUERYRBINFO` valido con `cbSize` igual a su tamano y se
        // pasa como puntero de salida unico; la API no retiene ninguno de los dos punteros.
        let result = unsafe { SHQueryRecycleBinW(PCWSTR(root.as_ptr()), &raw mut info) };
        result.map_err(|error| port_error("SHQueryRecycleBinW", error.code().0))?;
        usage_from_raw(info.i64Size, info.i64NumItems)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_from_raw_should_convert_regular_counters() {
        assert_eq!(
            usage_from_raw(340_000_000, 12),
            Ok(RecycleBinUsage {
                items: 12,
                bytes: 340_000_000
            })
        );
        assert_eq!(
            usage_from_raw(0, 0),
            Ok(RecycleBinUsage { items: 0, bytes: 0 })
        );
        assert_eq!(
            usage_from_raw(i64::MAX, i64::MAX),
            Ok(RecycleBinUsage {
                items: i64::MAX as u64,
                bytes: i64::MAX as u64
            })
        );
    }

    #[test]
    fn usage_from_raw_should_reject_negative_counters_instead_of_wrapping() {
        for (size, items) in [(-1, 5), (5, -1), (i64::MIN, 0), (0, i64::MIN), (-1, -1)] {
            assert!(
                matches!(usage_from_raw(size, items), Err(PortError::Io(_))),
                "({size}, {items})"
            );
        }
    }

    #[test]
    #[ignore = "consulta la Papelera real del usuario; correr a mano con --ignored"]
    fn query_should_measure_the_real_system_drive_recycle_bin() {
        let usage = WindowsRecycleBin.query(Path::new(r"C:\"));
        assert!(usage.is_ok(), "{usage:?}");
    }
}

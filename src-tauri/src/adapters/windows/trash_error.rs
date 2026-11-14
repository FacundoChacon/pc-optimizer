//! Traduccion pura de `trash::Error` a `TrashError` (design D-11).
//!
//! Las constantes `HRESULT` se confirmaron en el fuente de `windows` 0.62.2 (`COPYENGINE_E_*`)
//! y `trash` 5.3.0. Nunca se propaga la descripcion del sistema: lo desconocido se reduce a un
//! codigo ASCII estable (`io_0x...`).

use crate::domain::ports::TrashError;

const SHARING_VIOLATION_SRC: u32 = 0x8027_0027;
const RECYCLE_FORCE_NUKE: u32 = 0x8027_0036;
const RECYCLE_BIN_NOT_FOUND: u32 = 0x8027_003A;
const RECYCLE_SIZE_TOO_BIG: u32 = 0x8027_0037;
const RECYCLE_PATH_TOO_LONG: u32 = 0x8027_0038;
const PATH_TOO_DEEP_SRC: u32 = 0x8027_001D;
const ACCESS_DENIED_SRC: u32 = 0x8027_0021;
const REQUIRES_ELEVATION: u32 = 0x8027_0002;
const PATH_NOT_FOUND_SRC: u32 = 0x8027_0023;
const E_ACCESSDENIED: u32 = 0x8007_0005;
/// `HRESULT_FROM_WIN32(ERROR_FILE_NOT_FOUND)`.
const WIN32_FILE_NOT_FOUND: u32 = 0x8007_0002;
/// `HRESULT_FROM_WIN32(ERROR_PATH_NOT_FOUND)`.
const WIN32_PATH_NOT_FOUND: u32 = 0x8007_0003;
/// `HRESULT_FROM_WIN32(ERROR_SHARING_VIOLATION)`.
const WIN32_SHARING_VIOLATION: u32 = 0x8007_0020;
/// `HRESULT_FROM_WIN32(ERROR_LOCK_VIOLATION)`.
const WIN32_LOCK_VIOLATION: u32 = 0x8007_0021;

/// Traduce el `HRESULT` de `trash::Error::Os`.
pub(crate) fn from_hresult(code: i32) -> TrashError {
    // Reinterpreta los bits del HRESULT con signo: los codigos de falla tienen el bit alto en 1.
    let bits = u32::from_ne_bytes(code.to_ne_bytes());
    match bits {
        SHARING_VIOLATION_SRC | WIN32_SHARING_VIOLATION | WIN32_LOCK_VIOLATION => TrashError::InUse,
        RECYCLE_FORCE_NUKE | RECYCLE_BIN_NOT_FOUND => TrashError::BinUnavailable,
        RECYCLE_SIZE_TOO_BIG => TrashError::TooLarge,
        PATH_TOO_DEEP_SRC | RECYCLE_PATH_TOO_LONG => TrashError::PathTooLong,
        ACCESS_DENIED_SRC | E_ACCESSDENIED | REQUIRES_ELEVATION => TrashError::PermissionDenied,
        PATH_NOT_FOUND_SRC | WIN32_FILE_NOT_FOUND | WIN32_PATH_NOT_FOUND => TrashError::NotFound,
        other => TrashError::Other(format!("io_0x{other:08x}")),
    }
}

/// Traduce un error de `trash` (sin exponer su descripcion).
pub(crate) fn map_trash_error(error: &trash::Error) -> TrashError {
    match error {
        trash::Error::Os { code, .. } => from_hresult(*code),
        trash::Error::CanonicalizePath { .. } => TrashError::NotFound,
        trash::Error::CouldNotAccess { .. } => TrashError::Other("could_not_access".into()),
        trash::Error::TargetedRoot => TrashError::Other("targeted_root".into()),
        trash::Error::Unknown { .. } => TrashError::Other("aborted".into()),
        _ => TrashError::Other("unsupported_error".into()),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn hr(bits: u32) -> i32 {
        i32::from_ne_bytes(bits.to_ne_bytes())
    }

    #[test]
    fn from_hresult_should_map_each_confirmed_constant_to_its_variant() {
        let table = [
            (0x8027_0027, TrashError::InUse),
            (0x8007_0020, TrashError::InUse),
            (0x8007_0021, TrashError::InUse),
            (0x8027_0036, TrashError::BinUnavailable),
            (0x8027_003A, TrashError::BinUnavailable),
            (0x8027_0037, TrashError::TooLarge),
            (0x8027_001D, TrashError::PathTooLong),
            (0x8027_0038, TrashError::PathTooLong),
            (0x8027_0021, TrashError::PermissionDenied),
            (0x8007_0005, TrashError::PermissionDenied),
            (0x8027_0002, TrashError::PermissionDenied),
            (0x8027_0023, TrashError::NotFound),
            (0x8007_0002, TrashError::NotFound),
            (0x8007_0003, TrashError::NotFound),
        ];
        for (bits, expected) in table {
            assert_eq!(from_hresult(hr(bits)), expected, "HRESULT {bits:#010x}");
        }
    }

    #[test]
    fn from_hresult_should_reduce_unknown_codes_to_a_hex_code_without_system_text() {
        assert_eq!(
            from_hresult(hr(0x8027_0099)),
            TrashError::Other("io_0x80270099".into())
        );
        assert_eq!(from_hresult(0), TrashError::Other("io_0x00000000".into()));
    }

    #[test]
    fn map_trash_error_should_use_the_hresult_and_ignore_the_description() {
        let error = trash::Error::Os {
            code: hr(0x8027_0027),
            description: r"C:\Users\secreto\x.txt en uso".into(),
        };
        assert_eq!(map_trash_error(&error), TrashError::InUse);
        let unknown = trash::Error::Os {
            code: hr(0x8027_0099),
            description: r"C:\Users\secreto".into(),
        };
        let mapped = map_trash_error(&unknown);
        assert_eq!(mapped, TrashError::Other("io_0x80270099".into()));
    }

    #[test]
    fn map_trash_error_should_map_the_non_os_variants() {
        let canonicalize = trash::Error::CanonicalizePath {
            original: PathBuf::from("x"),
        };
        assert_eq!(map_trash_error(&canonicalize), TrashError::NotFound);
        assert_eq!(
            map_trash_error(&trash::Error::TargetedRoot),
            TrashError::Other("targeted_root".into())
        );
        let unknown = trash::Error::Unknown {
            description: r"C:\secreto".into(),
        };
        assert_eq!(
            map_trash_error(&unknown),
            TrashError::Other("aborted".into())
        );
        let access = trash::Error::CouldNotAccess {
            target: r"C:\secreto".into(),
        };
        assert_eq!(
            map_trash_error(&access),
            TrashError::Other("could_not_access".into())
        );
    }
}

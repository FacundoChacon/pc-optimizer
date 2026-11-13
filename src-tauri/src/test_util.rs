//! Utilidades de test que necesitan APIs de plataforma vedadas al dominio.

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;

/// Ruta de Windows que no es Unicode valido (sustituto UTF-16 suelto): `to_str()` da `None`.
pub(crate) fn non_unicode_path() -> PathBuf {
    let wide: Vec<u16> = r"C:\Users\ana\AppData\Local\Temp\"
        .encode_utf16()
        .chain([0xD800, u16::from(b'x')])
        .collect();
    PathBuf::from(OsString::from_wide(&wide))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_unicode_path_should_not_be_convertible_to_str() {
        assert_eq!(non_unicode_path().to_str(), None);
    }
}

//! Lectura de archivos de texto del repo con saltos de linea normalizados (C-13).
//!
//! En los runners de Windows `core.autocrlf=true` deja los archivos con CRLF; los tests buscan
//! subcadenas con `\n`, asi que todo archivo del repo se lee por aca y queda siempre en LF.

use std::path::Path;

/// Convierte `\r\n` en `\n`.
pub fn normalize_newlines(text: &str) -> String {
    text.replace("\r\n", "\n")
}

/// Lee un archivo UTF-8 y normaliza sus saltos de linea a `\n`.
pub fn read_text(path: &Path) -> std::io::Result<String> {
    std::fs::read_to_string(path).map(|text| normalize_newlines(&text))
}

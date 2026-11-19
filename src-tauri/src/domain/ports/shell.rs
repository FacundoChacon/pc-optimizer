//! Puertos del shell de Windows (C-07, D-02 y D-13): selector de carpeta y "Mostrar en el
//! Explorador". Ninguno ejecuta ni abre archivos: el selector devuelve una ruta elegida por el
//! usuario y el Explorador solo muestra una carpeta con un elemento seleccionado.

use std::path::{Path, PathBuf};

use crate::domain::ports::error::PortError;

/// Selector de carpeta del sistema (dialogo modal).
pub trait FolderPicker {
    /// `Ok(None)` si el usuario cancela; `Err` si el dialogo no pudo abrirse.
    fn pick(&self) -> Result<Option<PathBuf>, PortError>;
}

/// Muestra un elemento en el Explorador de Windows.
pub trait Explorer {
    /// Abre la carpeta que contiene `path` y lo selecciona. Nunca lo ejecuta.
    fn reveal(&self, path: &Path) -> Result<(), PortError>;
}

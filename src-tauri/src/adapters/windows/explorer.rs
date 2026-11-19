//! `WindowsExplorer`: "Mostrar en el Explorador" sin ejecutar nada (C-07, design D-13).
//!
//! Unica llamada del shell: `SHParseDisplayName` + `SHOpenFolderAndSelectItems`, que abre la
//! carpeta contenedora y selecciona el elemento. NUNCA `ShellExecute*` ni "abrir" el elemento:
//! un `.exe` o un `.lnk` no se ejecuta jamas (lo impone `tests/architecture.rs`).
//!
//! - Cada llamada corre en un hilo propio y de vida corta (`pcopt-reveal`) con COM STA, dentro de
//!   `catch_unwind`.
//! - `unsafe`: bloques minimos (inicializar COM, parsear la ruta, abrir el Explorador, liberar el
//!   identificador del shell y cerrar COM).

use std::os::windows::ffi::OsStrExt;
use std::panic::{self, AssertUnwindSafe};
use std::path::Path;
use std::thread;

use windows::Win32::System::Com::{
    COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoTaskMemFree,
    CoUninitialize, IBindCtx,
};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{SHOpenFolderAndSelectItems, SHParseDisplayName};
use windows::core::PCWSTR;

use super::folder_picker::code_detail;
use crate::domain::ports::{Explorer, PortError};

/// Fallo del backend, antes de traducirlo a `PortError`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BackendError {
    /// No se pudo inicializar COM.
    Init,
    /// `HRESULT` de una llamada del shell.
    Hresult(i32),
}

/// Backend real: COM STA -> identificador del shell -> abrir y seleccionar -> COM cerrado.
fn shell_backend(path: &Path) -> Result<(), BackendError> {
    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    wide.push(0);
    // SAFETY: `pvreserved` es nulo (obligatorio) y el apartamento se cierra mas abajo en este
    // mismo hilo solo si esta llamada tuvo exito (`S_OK` o `S_FALSE`).
    let init = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
    if init.is_err() {
        return Err(BackendError::Init);
    }
    let outcome = reveal(&wide);
    // SAFETY: empareja el `CoInitializeEx` exitoso de este mismo hilo.
    unsafe { CoUninitialize() };
    outcome
}

fn reveal(wide: &[u16]) -> Result<(), BackendError> {
    let mut pidl: *mut ITEMIDLIST = core::ptr::null_mut();
    // SAFETY: `wide` es un buffer UTF-16 terminado en nulo que vive durante la llamada; `pidl` es
    // un puntero local valido para escritura; sin contexto de enlace ni atributos pedidos.
    unsafe { SHParseDisplayName(PCWSTR(wide.as_ptr()), None::<&IBindCtx>, &mut pidl, 0, None) }
        .map_err(|error| BackendError::Hresult(error.code().0))?;
    // SAFETY: `pidl` lo asigno `SHParseDisplayName` con exito; sin `apidl` el shell abre la
    // carpeta contenedora y selecciona el propio elemento. No ejecuta nada.
    let result = unsafe { SHOpenFolderAndSelectItems(pidl.cast_const(), None, 0) };
    // SAFETY: `pidl` se asigno con `CoTaskMemAlloc` y no se usa despues de esta liberacion.
    unsafe { CoTaskMemFree(Some(pidl.cast_const().cast())) };
    result.map_err(|error| BackendError::Hresult(error.code().0))
}

type Backend = Box<dyn Fn(&Path) -> Result<(), BackendError> + Send + Sync>;

/// "Mostrar en el Explorador" de Windows.
pub struct WindowsExplorer {
    backend: Backend,
}

impl Default for WindowsExplorer {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowsExplorer {
    /// Con el shell real.
    pub fn new() -> Self {
        Self::with_backend(shell_backend)
    }

    /// Con un backend inyectado (los tests nunca abren el Explorador real).
    pub(crate) fn with_backend<F>(backend: F) -> Self
    where
        F: Fn(&Path) -> Result<(), BackendError> + Send + Sync + 'static,
    {
        Self {
            backend: Box::new(backend),
        }
    }
}

impl Explorer for WindowsExplorer {
    fn reveal(&self, path: &Path) -> Result<(), PortError> {
        if path.to_string_lossy().contains('\0') {
            return Err(PortError::Unsupported("invalid_path".into()));
        }
        let outcome = thread::scope(|scope| {
            thread::Builder::new()
                .name("pcopt-reveal".into())
                .spawn_scoped(scope, || {
                    panic::catch_unwind(AssertUnwindSafe(|| (self.backend)(path)))
                })
                .ok()
                .and_then(|handle| handle.join().ok())
        });
        match outcome {
            Some(Ok(Ok(()))) => Ok(()),
            Some(Ok(Err(BackendError::Hresult(code)))) => {
                Err(PortError::Unsupported(code_detail(code)))
            }
            Some(Ok(Err(BackendError::Init))) => Err(PortError::Unsupported("com_init".into())),
            // No se pudo crear el hilo o el backend entro en panico.
            Some(Err(_)) | None => Err(PortError::Unsupported("explorer_unavailable".into())),
        }
    }
}

#[cfg(test)]
#[path = "explorer_tests.rs"]
mod tests;

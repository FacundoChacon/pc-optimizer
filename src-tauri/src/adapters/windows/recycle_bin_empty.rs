//! Vaciado REAL de la Papelera (C-10, design D-07): la UNICA accion permanente del producto.
//!
//! Unico archivo del repo que contiene la llamada del shell que vacia la Papelera (lo impone
//! `tests/architecture.rs`). Solo lo usa `commands/recycle_bin.rs`, con un `AcknowledgedEmpty`.
//!
//! - Cada vaciado corre en un hilo propio y de vida corta (`pcopt-empty-bin`): ahi se inicializa
//!   COM en un apartamento STA, se hace UNA llamada bloqueante y se cierra COM. Asi no queda un
//!   STA colgado en el pool de bloqueo de Tauri (mismo criterio que C-09 D-10).
//! - Todo va dentro de `catch_unwind`: un panico o un `CoInitializeEx` fallido se traduce a
//!   `BackendUnavailable`.
//! - Flags: sin confirmacion, sin ventana de progreso y sin sonido del shell; nuestras dos
//!   confirmaciones las reemplazan. `hwnd` nulo.
//! - `unsafe`: 3 bloques minimos (`CoInitializeEx`, la llamada del shell y `CoUninitialize`).

use std::os::windows::ffi::OsStrExt;
use std::panic::{self, AssertUnwindSafe};
use std::thread;

use windows::Win32::Foundation::E_ACCESSDENIED;
use windows::Win32::System::Com::{
    COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoUninitialize,
};
use windows::core::PCWSTR;

use crate::domain::ports::{EmptyError, RecycleBinEmptier};
use crate::domain::recycle_bin::root::VolumeRoot;

/// Sin confirmacion, sin progreso, sin sonido.
const SHELL_FLAGS: u32 = windows::Win32::UI::Shell::SHERB_NOCONFIRMATION
    | windows::Win32::UI::Shell::SHERB_NOPROGRESSUI
    | windows::Win32::UI::Shell::SHERB_NOSOUND;

/// Fallo del backend, antes de traducirlo a `EmptyError`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BackendError {
    /// No se pudo inicializar COM.
    Init,
    /// `HRESULT` de la llamada del shell.
    Hresult(i32),
}

/// Traduce el `HRESULT` del shell: acceso denegado a su variante; el resto a un codigo hex
/// estable. La descripcion del sistema nunca se propaga.
pub(crate) fn empty_error_from_hresult(hresult: i32) -> EmptyError {
    if hresult == E_ACCESSDENIED.0 {
        return EmptyError::PermissionDenied;
    }
    EmptyError::Other(format!(
        "io_0x{:08x}",
        u32::from_ne_bytes(hresult.to_ne_bytes())
    ))
}

/// Backend real: COM STA -> una llamada al shell -> COM cerrado.
fn shell_backend(root: &VolumeRoot) -> Result<(), BackendError> {
    let mut wide: Vec<u16> = root.as_path().as_os_str().encode_wide().collect();
    // `VolumeRoot` solo admite letras: no hay NUL interno posible.
    wide.push(0);
    // SAFETY: `pvreserved` es nulo (obligatorio) y el apartamento se cierra mas abajo en este
    // mismo hilo solo si esta llamada tuvo exito (`S_OK` o `S_FALSE`).
    let init = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
    if init.is_err() {
        return Err(BackendError::Init);
    }
    // SAFETY: `hwnd` es nulo (sin ventana duena); `wide` es un buffer UTF-16 terminado en nulo
    // que vive hasta despues de la llamada y la API no retiene el puntero; COM esta inicializado
    // en este hilo.
    let result = unsafe {
        windows::Win32::UI::Shell::SHEmptyRecycleBinW(None, PCWSTR(wide.as_ptr()), SHELL_FLAGS)
    };
    // SAFETY: empareja el `CoInitializeEx` exitoso de este mismo hilo.
    unsafe { CoUninitialize() };
    result.map_err(|error| BackendError::Hresult(error.code().0))
}

type Backend = Box<dyn Fn(&VolumeRoot) -> Result<(), BackendError> + Send + Sync>;

/// Vaciador de la Papelera de Windows.
pub struct WindowsRecycleBinEmptier {
    backend: Backend,
}

impl Default for WindowsRecycleBinEmptier {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowsRecycleBinEmptier {
    /// Con el backend real (vacia la Papelera de verdad).
    pub fn new() -> Self {
        Self::with_backend(shell_backend)
    }

    /// Con un backend inyectado (los tests nunca usan el real).
    pub(crate) fn with_backend<F>(backend: F) -> Self
    where
        F: Fn(&VolumeRoot) -> Result<(), BackendError> + Send + Sync + 'static,
    {
        Self {
            backend: Box::new(backend),
        }
    }
}

impl RecycleBinEmptier for WindowsRecycleBinEmptier {
    fn empty(&self, root: &VolumeRoot) -> Result<(), EmptyError> {
        let outcome = thread::scope(|scope| {
            thread::Builder::new()
                .name("pcopt-empty-bin".into())
                .spawn_scoped(scope, || {
                    panic::catch_unwind(AssertUnwindSafe(|| (self.backend)(root)))
                })
                .ok()
                .and_then(|handle| handle.join().ok())
        });
        match outcome {
            Some(Ok(Ok(()))) => Ok(()),
            Some(Ok(Err(BackendError::Hresult(hresult)))) => Err(empty_error_from_hresult(hresult)),
            // No se pudo crear el hilo, panico o COM no inicializable.
            Some(Ok(Err(BackendError::Init))) | Some(Err(_)) | None => {
                Err(EmptyError::BackendUnavailable)
            }
        }
    }
}

#[cfg(test)]
#[path = "recycle_bin_empty_tests.rs"]
mod tests;

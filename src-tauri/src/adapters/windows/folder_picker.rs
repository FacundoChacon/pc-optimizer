//! `WindowsFolderPicker`: selector de carpeta nativo (`IFileOpenDialog`) abierto por el backend
//! (C-07, design D-02). Alternativa propia al plugin `dialog`, que exigiria ampliar capabilities.
//!
//! - Cada seleccion corre en un hilo propio y de vida corta (`pcopt-folder-picker`): ahi se
//!   inicializa COM en un apartamento STA, se muestra el dialogo modal sobre la ventana
//!   principal, se lee la ruta elegida y se cierra COM. Asi no queda un STA colgado en el pool de
//!   bloqueo de Tauri (mismo criterio que el vaciado de la Papelera).
//! - Todo va dentro de `catch_unwind`: un panico se traduce a un error del puerto.
//! - Opciones: solo carpetas del sistema de archivos que existen, sin cambiar el directorio de
//!   trabajo y sin agregar a "recientes" (`FOS_PICKFOLDERS` evita la vista previa de archivos de
//!   las extensiones de terceros, que se cargan igual en el proceso elevado).
//! - `unsafe`: bloques minimos (inicializar COM, mostrar y leer el dialogo, liberar la cadena del
//!   shell y cerrar COM).

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::panic::{self, AssertUnwindSafe};
use std::path::PathBuf;
use std::thread;

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoCreateInstance,
    CoInitializeEx, CoTaskMemFree, CoUninitialize,
};
use windows::Win32::UI::Shell::{
    FOS_DONTADDTORECENT, FOS_FORCEFILESYSTEM, FOS_NOCHANGEDIR, FOS_PATHMUSTEXIST, FOS_PICKFOLDERS,
    FileOpenDialog, IFileOpenDialog, SIGDN_FILESYSPATH,
};

use crate::domain::ports::{FolderPicker, PortError};

/// `HRESULT_FROM_WIN32(ERROR_CANCELLED)`: el usuario cerro el dialogo sin elegir.
const HRESULT_CANCELLED: i32 = 0x8007_04C7_u32 as i32;

/// Fallo del backend, antes de traducirlo a `PortError`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BackendError {
    /// No se pudo inicializar COM.
    Init,
    /// `HRESULT` de una llamada del dialogo.
    Hresult(i32),
}

/// Codigo ASCII estable de un `HRESULT`; la descripcion del sistema nunca se propaga.
pub(crate) fn code_detail(hresult: i32) -> String {
    format!("io_0x{:08x}", u32::from_ne_bytes(hresult.to_ne_bytes()))
}

fn into_port_result(
    result: Result<Option<PathBuf>, BackendError>,
) -> Result<Option<PathBuf>, PortError> {
    match result {
        Ok(path) => Ok(path),
        Err(BackendError::Hresult(HRESULT_CANCELLED)) => Ok(None),
        Err(BackendError::Hresult(code)) => Err(PortError::Unsupported(code_detail(code))),
        Err(BackendError::Init) => Err(PortError::Unsupported("com_init".into())),
    }
}

/// Backend real: COM STA -> dialogo modal -> ruta elegida -> COM cerrado.
fn shell_backend(owner: Option<isize>) -> Result<Option<PathBuf>, BackendError> {
    // SAFETY: `pvreserved` es nulo (obligatorio) y el apartamento se cierra mas abajo en este
    // mismo hilo solo si esta llamada tuvo exito (`S_OK` o `S_FALSE`).
    let init = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
    if init.is_err() {
        return Err(BackendError::Init);
    }
    let outcome = show_dialog(owner);
    // SAFETY: empareja el `CoInitializeEx` exitoso de este mismo hilo; `show_dialog` ya soltó
    // todas las interfaces COM que creo.
    unsafe { CoUninitialize() };
    outcome
}

fn hresult_of(error: &windows::core::Error) -> BackendError {
    BackendError::Hresult(error.code().0)
}

fn show_dialog(owner: Option<isize>) -> Result<Option<PathBuf>, BackendError> {
    // SAFETY: COM esta inicializado en este hilo (`shell_backend`); `FileOpenDialog` es un CLSID
    // valido de `IFileOpenDialog` y `punkouter` es nulo.
    let dialog: IFileOpenDialog =
        unsafe { CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER) }
            .map_err(|error| hresult_of(&error))?;
    // SAFETY: `dialog` es una interfaz valida y viva en este hilo.
    unsafe {
        dialog.SetOptions(
            FOS_PICKFOLDERS
                | FOS_FORCEFILESYSTEM
                | FOS_PATHMUSTEXIST
                | FOS_NOCHANGEDIR
                | FOS_DONTADDTORECENT,
        )
    }
    .map_err(|error| hresult_of(&error))?;
    // El `HWND` es el de la ventana principal del propio proceso (o ninguno): un valor invalido
    // solo hace que el dialogo no tenga duena, nunca lo comparte con otro proceso.
    let owner = owner.map(|raw| HWND(raw as *mut core::ffi::c_void));
    // SAFETY: `dialog` es valida; `Show` es bloqueante y modal en este hilo STA.
    unsafe { dialog.Show(owner) }.map_err(|error| hresult_of(&error))?;
    // SAFETY: tras un `Show` exitoso hay un resultado; devuelve una interfaz valida.
    let item = unsafe { dialog.GetResult() }.map_err(|error| hresult_of(&error))?;
    // SAFETY: `item` es valida; `GetDisplayName` asigna una cadena con `CoTaskMemAlloc` que se
    // libera abajo.
    let name =
        unsafe { item.GetDisplayName(SIGDN_FILESYSPATH) }.map_err(|error| hresult_of(&error))?;
    // SAFETY: `name` apunta a una cadena UTF-16 terminada en nulo asignada por el shell; se lee
    // una sola vez, antes de liberarla.
    let path = PathBuf::from(OsString::from_wide(unsafe { name.as_wide() }));
    // SAFETY: `name` se asigno con `CoTaskMemAlloc` y no se usa despues de esta liberacion.
    unsafe { CoTaskMemFree(Some(name.0.cast_const().cast())) };
    Ok(Some(path))
}

type Backend = Box<dyn Fn(Option<isize>) -> Result<Option<PathBuf>, BackendError> + Send + Sync>;

/// Selector de carpeta del sistema.
pub struct WindowsFolderPicker {
    owner: Option<isize>,
    backend: Backend,
}

impl WindowsFolderPicker {
    /// Con el dialogo real. `owner` es el `HWND` de la ventana principal como entero (o `None`).
    pub fn new(owner: Option<isize>) -> Self {
        Self::with_backend(owner, shell_backend)
    }

    /// Con un backend inyectado (los tests nunca abren el dialogo real).
    pub(crate) fn with_backend<F>(owner: Option<isize>, backend: F) -> Self
    where
        F: Fn(Option<isize>) -> Result<Option<PathBuf>, BackendError> + Send + Sync + 'static,
    {
        Self {
            owner,
            backend: Box::new(backend),
        }
    }
}

impl FolderPicker for WindowsFolderPicker {
    fn pick(&self) -> Result<Option<PathBuf>, PortError> {
        let outcome = thread::scope(|scope| {
            thread::Builder::new()
                .name("pcopt-folder-picker".into())
                .spawn_scoped(scope, || {
                    panic::catch_unwind(AssertUnwindSafe(|| (self.backend)(self.owner)))
                })
                .ok()
                .and_then(|handle| handle.join().ok())
        });
        match outcome {
            Some(Ok(result)) => into_port_result(result),
            // No se pudo crear el hilo o el backend entro en panico.
            Some(Err(_)) | None => Err(PortError::Unsupported("picker_unavailable".into())),
        }
    }
}

#[cfg(test)]
#[path = "folder_picker_tests.rs"]
mod tests;

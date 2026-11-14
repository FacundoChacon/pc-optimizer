//! Papelera real sobre el crate `trash` en un hilo dedicado (design D-08, D-09, D-10, D-12).
//!
//! - Un hilo propio por ejecucion de limpieza hace TODAS las llamadas a `trash`: el apartamento
//!   STA que `trash` inicializa por hilo no contamina los hilos del pool de Tauri, y el
//!   `thread_local` de COM se destruye al terminar el hilo (al soltar el adaptador).
//! - Una llamada por elemento (`trash::delete`), nunca por lotes: el resultado es exacto por
//!   elemento y solo se mueve lo que tiene su `DeleteTarget` vivo.
//! - Cada llamada va dentro de `catch_unwind`: `trash` hace `panic!` si `CoInitializeEx` falla;
//!   un panico se traduce a `BackendUnavailable` y el hilo sigue atendiendo pedidos.
//! - Sin `unsafe` propio: todo el COM lo hace `trash`.

use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};

use super::trash_error::map_trash_error;
use crate::domain::ports::{Trash, TrashError};
use crate::domain::protection::service::DeleteTarget;

/// Pedido al hilo: la ruta a mover y el canal donde responder.
struct Request {
    path: PathBuf,
    reply: Sender<Result<(), TrashError>>,
}

/// Mueve UNA ruta con la funcion de borrado inyectada, traduciendo errores y panicos.
fn run_one<F>(delete: &F, path: &Path) -> Result<(), TrashError>
where
    F: Fn(&Path) -> Result<(), trash::Error>,
{
    match panic::catch_unwind(AssertUnwindSafe(|| delete(path))) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(map_trash_error(&error)),
        Err(_) => Err(TrashError::BackendUnavailable),
    }
}

/// Papelera de Windows. Soltarla termina el hilo dedicado.
pub struct WindowsTrash {
    // `Option` para poder cerrar el canal antes de unir el hilo en `Drop`.
    requests: Option<Sender<Request>>,
    worker: Option<JoinHandle<()>>,
}

impl WindowsTrash {
    /// Arranca el hilo dedicado con el backend real (`trash::delete`, un elemento por llamada).
    pub fn new() -> Result<Self, TrashError> {
        Self::with_backend(|path: &Path| trash::delete(path))
    }

    /// Igual que `new`, con la funcion de borrado inyectada (tests de panico y errores).
    pub(crate) fn with_backend<F>(delete: F) -> Result<Self, TrashError>
    where
        F: Fn(&Path) -> Result<(), trash::Error> + Send + 'static,
    {
        let (requests, inbox) = mpsc::channel::<Request>();
        let worker = thread::Builder::new()
            .name("pcopt-trash".into())
            .spawn(move || {
                // Termina cuando se suelta el `Sender` del adaptador.
                for request in inbox {
                    let result = run_one(&delete, &request.path);
                    // Si el dominio abandono la espera no hay a quien responder.
                    let _ = request.reply.send(result);
                }
            })
            .map_err(|_| TrashError::BackendUnavailable)?;
        Ok(Self {
            requests: Some(requests),
            worker: Some(worker),
        })
    }

    fn move_path(&self, path: &Path) -> Result<(), TrashError> {
        let requests = self
            .requests
            .as_ref()
            .ok_or(TrashError::BackendUnavailable)?;
        let (reply, answer) = mpsc::channel();
        requests
            .send(Request {
                path: path.to_path_buf(),
                reply,
            })
            .map_err(|_| TrashError::BackendUnavailable)?;
        answer.recv().map_err(|_| TrashError::BackendUnavailable)?
    }
}

impl Trash for WindowsTrash {
    fn move_to_trash(&self, target: &DeleteTarget) -> Result<(), TrashError> {
        self.move_path(target.path())
    }
}

impl Drop for WindowsTrash {
    fn drop(&mut self) {
        self.requests = None;
        if let Some(worker) = self.worker.take() {
            // Si el hilo murio fuera de `catch_unwind` no hay nada que recuperar.
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
#[path = "trash_tests.rs"]
mod tests;

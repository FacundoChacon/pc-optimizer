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

use super::recycle_bin_policy::WindowsRecycleBinPolicy;
use super::trash_error::map_trash_error;
use crate::domain::ports::{RecycleBinPolicy, Trash, TrashError, check_for_move};
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
    policy: Box<dyn RecycleBinPolicy + Send + Sync>,
    // `Option` para poder cerrar el canal antes de unir el hilo en `Drop`.
    requests: Option<Sender<Request>>,
    worker: Option<JoinHandle<()>>,
}

impl WindowsTrash {
    /// Arranca el hilo dedicado con el backend real (`trash::delete`, un elemento por llamada).
    pub fn new() -> Result<Self, TrashError> {
        Self::with_backend_and_policy(|path: &Path| trash::delete(path), WindowsRecycleBinPolicy)
    }

    /// Igual que `new`, con la funcion de borrado y la politica inyectadas.
    pub(crate) fn with_backend_and_policy<F, P>(delete: F, policy: P) -> Result<Self, TrashError>
    where
        F: Fn(&Path) -> Result<(), trash::Error> + Send + 'static,
        P: RecycleBinPolicy + Send + Sync + 'static,
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
            policy: Box::new(policy),
            requests: Some(requests),
            worker: Some(worker),
        })
    }

    /// Backend inyectado con la Papelera siempre disponible y sin limite (tests del hilo y del mapeo).
    #[cfg(test)]
    pub(crate) fn with_backend<F>(delete: F) -> Result<Self, TrashError>
    where
        F: Fn(&Path) -> Result<(), trash::Error> + Send + 'static,
    {
        use crate::domain::ports::BinAvailability;
        use crate::domain::ports::fakes::FakeRecycleBinPolicy;
        Self::with_backend_and_policy(
            delete,
            FakeRecycleBinPolicy::with(BinAvailability::Enabled {
                max_capacity_bytes: None,
            }),
        )
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
    fn preflight(&self, sample: &Path) -> Result<(), TrashError> {
        // Tamano 0: solo importa si la Papelera esta disponible (la capacidad se mira por archivo).
        check_for_move(self.policy.as_ref(), sample, 0)
    }

    fn move_to_trash(&self, target: &DeleteTarget) -> Result<(), TrashError> {
        // D-12 B: el control vive AQUI, justo antes de cada `trash::delete`, para que no pueda
        // saltearse: con la Papelera deshabilitada o un archivo que no entra, el shell destruye el
        // archivo sin devolver error. Si la politica no se puede leer, no se mueve (fail-closed).
        check_for_move(
            self.policy.as_ref(),
            target.path(),
            target.checked().size_bytes,
        )?;
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

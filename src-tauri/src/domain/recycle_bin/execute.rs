//! Preparacion y ejecucion del vaciado (design D-01, D-04, D-05).
//!
//! `prepare_empty` mide (puro sobre `RecycleBinInfo`); `run_empty_recycle_bin` es lo UNICO que
//! llama al puerto de vaciado y solo acepta un `AcknowledgedEmpty`.

use crate::domain::model::EpochMillis;
use crate::domain::ports::{Clock, PortError, RecycleBinEmptier, RecycleBinInfo};
use crate::domain::privileges::scope_access;
use crate::domain::privileges::{MismatchReason, PrivilegeContext, ScopeAccess, UserScope};

use super::outcome::{EmptyOutcome, classify_empty};
use super::request::{AcknowledgedEmpty, EmptyRequestError, PreparedEmpty};
use super::root::VolumeRoot;

/// Por que un paso del vaciado no se ejecuto (no hubo accion).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum EmptyFlowError {
    /// La Papelera es por-usuario y la sesion no es la del token.
    #[error("recycle bin blocked: {}", .0.as_str())]
    Blocked(MismatchReason),
    /// Sin unidad del sistema o la medicion fallo.
    #[error("recycle bin unavailable")]
    Unavailable,
    /// No hay nada que vaciar.
    #[error("recycle bin already empty")]
    AlreadyEmpty,
    #[error("empty request not found")]
    NotFound,
    #[error("empty request expired")]
    Expired,
    /// La Papelera cambio desde que el usuario la reviso.
    #[error("recycle bin changed since it was measured")]
    Changed,
}

impl From<EmptyRequestError> for EmptyFlowError {
    fn from(error: EmptyRequestError) -> Self {
        match error {
            EmptyRequestError::NotFound | EmptyRequestError::NotAcknowledged => Self::NotFound,
            EmptyRequestError::Expired => Self::Expired,
        }
    }
}

/// La Papelera es por-usuario: en `mismatch` no se mide ni se vacia.
pub fn ensure_not_blocked(privileges: &PrivilegeContext) -> Result<(), EmptyFlowError> {
    match scope_access(privileges, UserScope::PerUser) {
        ScopeAccess::Allowed => Ok(()),
        ScopeAccess::Blocked(reason) => Err(EmptyFlowError::Blocked(reason)),
    }
}

/// Primer paso: mide la Papelera del volumen del sistema y crea la solicitud. No vacia nada.
///
/// `root` es `None` si no se pudo resolver la unidad del sistema (no se adivina una letra).
pub fn prepare_empty(
    privileges: &PrivilegeContext,
    root: Option<VolumeRoot>,
    bin: &impl RecycleBinInfo,
    now: EpochMillis,
    request_id: u64,
) -> Result<PreparedEmpty, EmptyFlowError> {
    ensure_not_blocked(privileges)?;
    let root = root.ok_or(EmptyFlowError::Unavailable)?;
    let usage = bin
        .query(&root.as_path())
        .map_err(|_: PortError| EmptyFlowError::Unavailable)?;
    if usage.items == 0 {
        return Err(EmptyFlowError::AlreadyEmpty);
    }
    Ok(PreparedEmpty::new(request_id, root, usage, now))
}

/// Segundo paso: re-valida, re-mide con igualdad estricta, vacia UNA vez y clasifica por medicion.
///
/// Orden: mismatch -> token y vigencia -> re-medicion (fallo = `Unavailable`; distinta =
/// `Changed`) -> llamada al shell -> medicion posterior. En cualquier rechazo el vaciador no se
/// llama.
pub fn run_empty_recycle_bin(
    request: AcknowledgedEmpty,
    token: u64,
    privileges: &PrivilegeContext,
    bin: &impl RecycleBinInfo,
    emptier: &impl RecycleBinEmptier,
    clock: &impl Clock,
) -> Result<EmptyOutcome, EmptyFlowError> {
    ensure_not_blocked(privileges)?;
    request.check(token, clock.now())?;
    let root = request.root();
    let before = bin
        .query(&root.as_path())
        .map_err(|_: PortError| EmptyFlowError::Unavailable)?;
    if before != request.usage() {
        return Err(EmptyFlowError::Changed);
    }
    let call = emptier.empty(&root);
    let after = bin.query(&root.as_path());
    Ok(EmptyOutcome {
        root,
        before,
        result: classify_empty(before, call, after),
        finished_at: clock.now(),
    })
}

#[cfg(test)]
#[path = "execute_tests.rs"]
mod tests;

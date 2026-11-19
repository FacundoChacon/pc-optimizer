//! Codigos estables de `error_detail` y su mapeo desde las decisiones de proteccion y de la
//! Papelera (design D-06).
//!
//! Todo codigo es ASCII `snake_case`, sin rutas ni texto localizado: la UI lo traduce por
//! familia con un respaldo generico. Los codigos de proteccion son los de
//! `ProtectedReason::code()`.

use crate::domain::model::CleanupOutcome;
use crate::domain::ports::TrashError;
use crate::domain::privileges::MismatchReason;
use crate::domain::protection::decision::{ProtectedReason, ProtectionDecision, ResolveFailure};

/// Barrera 1 (alcance por-usuario) con la sesion de otro usuario.
pub const BLOCKED_DIFFERENT_USER: &str = "blocked_different_user";
/// Barrera 1 con la identidad de la sesion sin determinar.
pub const BLOCKED_UNDETERMINED: &str = "blocked_undetermined";
/// Nube: el elemento o un ancestro paso a ser un placeholder.
pub const SKIPPED_CLOUD: &str = "skipped_cloud";
/// Tamano o fecha distintos de los del escaneo (archivo abierto o reemplazado).
pub const CHANGED_SINCE_SCAN: &str = "changed_since_scan";
/// Escrito hace menos de 24 h o sin fecha.
pub const RECENTLY_MODIFIED: &str = "recently_modified";
/// La Papelera informo violacion de comparticion.
pub const IN_USE: &str = "in_use";
/// La Papelera no esta disponible en el volumen (aborta la ejecucion).
pub const RECYCLE_BIN_UNAVAILABLE: &str = "recycle_bin_unavailable";
/// El elemento no entra en la Papelera.
pub const RECYCLE_BIN_TOO_SMALL: &str = "recycle_bin_too_small";
/// Ruta demasiado larga para el shell.
pub const PATH_TOO_LONG: &str = "path_too_long";
/// Acceso denegado o requiere elevacion.
pub const PERMISSION_DENIED: &str = "permission_denied";
/// Desaparecio entre verificar y mover.
pub const NOT_FOUND: &str = "not_found";
/// La Papelera respondio exito pero el archivo sigue en su lugar.
pub const NOT_MOVED: &str = "not_moved";
/// El mecanismo de Papelera no puede usarse (aborta la ejecucion).
pub const TRASH_UNAVAILABLE: &str = "trash_unavailable";
/// Otro fallo sin codigo.
pub const IO: &str = "io";

/// Codigos propios de la limpieza (los de proteccion salen de `ProtectedReason::code()`).
#[cfg(test)]
pub(crate) const ALL_CODES: [&str; 14] = [
    BLOCKED_DIFFERENT_USER,
    BLOCKED_UNDETERMINED,
    SKIPPED_CLOUD,
    CHANGED_SINCE_SCAN,
    RECENTLY_MODIFIED,
    IN_USE,
    RECYCLE_BIN_UNAVAILABLE,
    RECYCLE_BIN_TOO_SMALL,
    PATH_TOO_LONG,
    PERMISSION_DENIED,
    NOT_FOUND,
    NOT_MOVED,
    TRASH_UNAVAILABLE,
    IO,
];

/// Resultado y detalle de una decision de proteccion que no es `Allowed` (`None` si lo es).
///
/// Un archivo que otro proceso tiene abierto (`Unresolvable(InUse)`) es `skipped_locked`, no
/// `skipped_protected`.
pub fn outcome_for_decision(
    decision: &ProtectionDecision,
) -> Option<(CleanupOutcome, &'static str)> {
    match decision {
        ProtectionDecision::Allowed(_) => None,
        ProtectionDecision::SkippedCloud { .. } => {
            Some((CleanupOutcome::SkippedCloud, SKIPPED_CLOUD))
        }
        ProtectionDecision::Protected(reason) => {
            let outcome = match reason {
                ProtectedReason::Unresolvable(ResolveFailure::InUse) => {
                    CleanupOutcome::SkippedLocked
                }
                _ => CleanupOutcome::SkippedProtected,
            };
            Some((outcome, reason.code()))
        }
    }
}

/// Detalle de la barrera 1 (alcance por-usuario bloqueado) segun el motivo del mismatch.
pub fn blocked_detail(reason: MismatchReason) -> &'static str {
    match reason {
        MismatchReason::DifferentUser => BLOCKED_DIFFERENT_USER,
        MismatchReason::Undetermined => BLOCKED_UNDETERMINED,
    }
}

/// Resultado y detalle de un fallo de la Papelera. `Other` conserva su codigo solo si es ASCII
/// `snake_case` estable; cualquier otra cosa (rutas, texto del sistema) se reduce a `io`.
pub fn outcome_for_trash(error: &TrashError) -> (CleanupOutcome, String) {
    let (outcome, detail) = match error {
        TrashError::InUse => (CleanupOutcome::SkippedLocked, IN_USE),
        TrashError::BinUnavailable => (CleanupOutcome::Error, RECYCLE_BIN_UNAVAILABLE),
        TrashError::TooLarge => (CleanupOutcome::Error, RECYCLE_BIN_TOO_SMALL),
        TrashError::PathTooLong => (CleanupOutcome::Error, PATH_TOO_LONG),
        TrashError::PermissionDenied => (CleanupOutcome::Error, PERMISSION_DENIED),
        TrashError::NotFound => (CleanupOutcome::Error, NOT_FOUND),
        TrashError::BackendUnavailable => (CleanupOutcome::Error, TRASH_UNAVAILABLE),
        TrashError::Other(code) => {
            let stable = !code.is_empty()
                && code
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
            return (
                CleanupOutcome::Error,
                if stable { code.clone() } else { IO.to_string() },
            );
        }
    };
    (outcome, detail.to_string())
}

#[cfg(test)]
#[path = "detail_tests.rs"]
mod tests;

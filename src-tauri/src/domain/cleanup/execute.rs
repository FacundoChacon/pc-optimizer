//! Ejecucion de la limpieza (design D-05, D-06, D-13..D-18).

use std::sync::atomic::{AtomicBool, Ordering};

use crate::domain::junk::category::CategoryScope;
use crate::domain::model::{CleanupOutcome, EpochMillis, NewCleanupLogEntry};
use crate::domain::ports::{CleanupLogStore, Clock, EntryKind, FileSystem, Trash, TrashError};
use crate::domain::privileges::{PrivilegeContext, ScopeAccess, UserScope, scope_access};
use crate::domain::protection::context::{ContextError, ProtectionContext};
use crate::domain::protection::decision::{AllowRule, CheckedPath, ProtectedReason};
use crate::domain::protection::path::normalize_lexical;
use crate::domain::protection::service::PathProtection;

use super::detail::{self, blocked_detail, outcome_for_decision, outcome_for_trash};
use super::model::{
    AbortReason, CleanupItem, CleanupPreview, CleanupStatus, CleanupSummary, is_recent,
};
use super::summary::Tally;

/// Receptor del avance de una limpieza.
pub trait CleanupObserver {
    /// Una vez, antes del primer item, con el total a procesar.
    fn started(&mut self, total: u64);
    /// Cada [`PROGRESS_EVERY_ITEMS`] items procesados y una vez mas al terminar el ciclo.
    fn progress(&mut self, processed: u64, total: u64, moved_count: u64, moved_bytes: u64);
}

/// Cada cuantos items se avisa el progreso (D-18): por conteo, sin reloj.
pub const PROGRESS_EVERY_ITEMS: u64 = 32;

/// Resultado de procesar un item, listo para registrar.
struct Step {
    outcome: CleanupOutcome,
    detail: Option<String>,
    path: String,
    size_bytes: u64,
    /// Si el resultado obliga a detener la ejecucion (D-17).
    abort: Option<AbortReason>,
}

impl Step {
    fn new(outcome: CleanupOutcome, detail: Option<&str>, path: String, size_bytes: u64) -> Self {
        Self {
            outcome,
            detail: detail.map(str::to_string),
            path,
            size_bytes,
            abort: None,
        }
    }
}

/// Ejecuta el preview: re-verifica cada item contra el disco y lo mueve a la Papelera.
///
/// Consume el preview (de un solo uso). Un fallo por item nunca detiene el resto.
#[expect(
    clippy::too_many_arguments,
    reason = "cada argumento es un puerto o una entrada independiente de la ejecucion; agruparlos solo agregaria una estructura sin significado"
)]
pub fn run_cleanup<F, T, L, C, O>(
    preview: CleanupPreview,
    privileges: &PrivilegeContext,
    protection: Result<ProtectionContext, ContextError>,
    fs: &F,
    trash: &T,
    log: &L,
    clock: &C,
    cancel: &AtomicBool,
    observer: &mut O,
) -> CleanupSummary
where
    F: FileSystem,
    T: Trash,
    L: CleanupLogStore,
    C: Clock,
    O: CleanupObserver,
{
    let started_at = clock.now();
    observer.started(preview.items().len() as u64);
    let context_ok = protection.is_ok();
    let service = PathProtection::new(fs, protection);
    let mut tally = Tally::new(&preview);
    let mut status = CleanupStatus::Completed;
    let mut last_reported = 0;
    for item in preview.items() {
        // El item en curso siempre termina: la cancelacion solo se mira entre items.
        if cancel.load(Ordering::SeqCst) {
            status = CleanupStatus::Cancelled;
            break;
        }
        let step = process_item(
            item,
            &Env {
                privileges,
                context_ok,
                now: clock.now(),
                service: &service,
                fs,
                trash,
            },
        );
        let entry = NewCleanupLogEntry {
            ts: clock.now(),
            category: item.category.as_str().to_string(),
            path: step.path.clone(),
            size_bytes: step.size_bytes,
            outcome: step.outcome,
            scan_id: Some(preview.scan_id()),
            error_detail: step.detail.clone(),
        };
        let logged = log.append_cleanup_log(&[entry]);
        // Lo que se hizo se informa aunque no haya podido registrarse.
        tally.record(
            item.category,
            step.outcome,
            step.detail.as_deref(),
            step.size_bytes,
        );
        if logged.is_err() {
            status = CleanupStatus::Aborted(AbortReason::LogUnavailable);
            break;
        }
        if let Some(reason) = step.abort {
            status = CleanupStatus::Aborted(reason);
            break;
        }
        if tally.processed().is_multiple_of(PROGRESS_EVERY_ITEMS) {
            report(observer, &tally);
            last_reported = tally.processed();
        }
    }
    if tally.processed() != last_reported {
        report(observer, &tally);
    }
    tally.finish(status, started_at, clock.now())
}

fn report<O: CleanupObserver>(observer: &mut O, tally: &Tally) {
    let (moved_count, moved_bytes) = tally.moved();
    observer.progress(tally.processed(), tally.total(), moved_count, moved_bytes);
}

/// Lo que `process_item` necesita del entorno de la ejecucion.
struct Env<'a, F: FileSystem, T: Trash> {
    privileges: &'a PrivilegeContext,
    context_ok: bool,
    now: EpochMillis,
    service: &'a PathProtection<'a, F>,
    fs: &'a F,
    trash: &'a T,
}

/// Re-verifica un item y, si todo coincide, lo mueve (D-05 pasos 2 a 9).
fn process_item<F: FileSystem, T: Trash>(item: &CleanupItem, env: &Env<'_, F, T>) -> Step {
    // Sin verificar el disco se registra la ruta y el tamano persistidos.
    let skipped = |outcome: CleanupOutcome, detail: &str| {
        Step::new(outcome, Some(detail), item.path.clone(), item.size_bytes)
    };
    // Barrera 1: el alcance por-usuario se reevalua con el contexto de AHORA, sin tocar el disco.
    if item.category.scope() == CategoryScope::PerUser
        && let ScopeAccess::Blocked(reason) = scope_access(env.privileges, UserScope::PerUser)
    {
        return skipped(CleanupOutcome::SkippedProtected, blocked_detail(reason));
    }
    if !env.context_ok {
        return skipped(
            CleanupOutcome::SkippedProtected,
            ProtectedReason::ContextUnavailable.code(),
        );
    }
    let lexical = match normalize_lexical(&item.path) {
        Ok(path) => path,
        Err(rejection) => {
            return skipped(
                CleanupOutcome::SkippedProtected,
                ProtectedReason::Unparseable(rejection).code(),
            );
        }
    };
    let scanned = CheckedPath {
        path: lexical,
        kind: EntryKind::File,
        size_bytes: item.size_bytes,
        modified: item.modified_at,
        identity: item.identity,
        rule: AllowRule::Default,
    };
    let target = match env.service.prepare_delete(&scanned) {
        Ok(target) => target,
        Err(decision) => {
            return match outcome_for_decision(&decision) {
                Some((outcome, code)) => skipped(outcome, code),
                None => skipped(CleanupOutcome::Error, detail::IO),
            };
        }
    };
    let real = target.path().display().to_string();
    let current = target.checked();
    let size = current.size_bytes;
    // Con lo leido del archivo ABIERTO: el listado de carpeta puede ir atrasado (D-27).
    let verified =
        |outcome: CleanupOutcome, code: &str| Step::new(outcome, Some(code), real.clone(), size);
    if current.size_bytes != item.size_bytes || current.modified != item.modified_at {
        return verified(CleanupOutcome::SkippedLocked, detail::CHANGED_SINCE_SCAN);
    }
    if is_recent(current.modified, env.now) {
        return verified(CleanupOutcome::SkippedLocked, detail::RECENTLY_MODIFIED);
    }
    if let Err(error) = env.trash.move_to_trash(&target) {
        let (outcome, code) = outcome_for_trash(&error);
        let mut step = verified(outcome, &code);
        step.abort = match error {
            TrashError::BinUnavailable => Some(AbortReason::RecycleBinUnavailable),
            TrashError::BackendUnavailable => Some(AbortReason::TrashUnavailable),
            _ => None,
        };
        return step;
    }
    // Defensa en profundidad: la Papelera dijo que si; si el mismo archivo sigue ahi, no se movio.
    let still_there = matches!(
        env.fs.metadata(target.path()),
        Ok(meta) if meta.identity == Some(current.identity)
    );
    if still_there {
        return verified(CleanupOutcome::Error, detail::NOT_MOVED);
    }
    // `target` se suelta al salir: la fijacion vive hasta despues de la post-verificacion.
    Step::new(CleanupOutcome::MovedToTrash, None, real.clone(), size)
}

#[cfg(test)]
#[path = "execute_tests.rs"]
mod tests;

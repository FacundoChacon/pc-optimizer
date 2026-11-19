//! `DuplicateContentGuard`: la verificacion de contenido que corre dentro de `run_cleanup`
//! (design D-13, D-14).
//!
//! Corre con la fijacion de los ancestros viva y justo antes de mover cada copia:
//!
//! 1. **Copia conservada, una vez por grupo** (al llegar a su primer item): basta UNA copia
//!    conservada que siga en su ruta con la misma identidad, tamano, fecha y hash completo. Si
//!    ninguna lo cumple, todas las copias a eliminar de ese grupo se omiten
//!    (`kept_copy_unavailable`) y se recuerda el resultado.
//! 2. **Cada copia a eliminar**: se abre la ruta real fijada, la identidad del handle debe ser la
//!    verificada y su hash completo debe ser el del grupo.
//!
//! La copia conservada NO se fija (no se va a mover): si el usuario la borra despues de su
//! verificacion y antes de que termine la ejecucion, es una accion propia del usuario.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use crate::domain::cleanup::detail::{
    CONTENT_CHANGED, CONTENT_UNREADABLE, IN_USE, KEPT_COPY_UNAVAILABLE, SKIPPED_CLOUD,
};
use crate::domain::cleanup::execute::{GuardVerdict, ItemGuard};
use crate::domain::cleanup::model::CleanupItem;
use crate::domain::duplicates::hash::{Expected, HashFailure, hash_full, open_verified, to_hex};
use crate::domain::duplicates::preview::{DuplicatePlan, GroupPlan};
use crate::domain::model::CleanupOutcome;
use crate::domain::ports::ContentReader;
use crate::domain::protection::decision::ProtectedReason;
use crate::domain::protection::service::DeleteTarget;

/// Identidad cambiada: mismo codigo que `ProtectedReason::IdentityChanged`.
fn identity_changed() -> &'static str {
    ProtectedReason::IdentityChanged.code()
}

/// Verificacion de contenido de las copias de un preview de duplicados.
pub struct DuplicateContentGuard<'a, R: ContentReader> {
    reader: &'a R,
    plan: &'a DuplicatePlan,
    /// `finding_id` de cada copia a eliminar -> indice de su grupo en el plan.
    group_of_item: HashMap<i64, usize>,
    /// Resultado de verificar la copia conservada de cada grupo (una sola vez).
    kept_ok: HashMap<usize, bool>,
}

impl<'a, R: ContentReader> DuplicateContentGuard<'a, R> {
    pub fn new(reader: &'a R, plan: &'a DuplicatePlan) -> Self {
        let group_of_item = plan
            .groups
            .iter()
            .enumerate()
            .flat_map(|(index, group)| group.item_file_ids.iter().map(move |id| (*id, index)))
            .collect();
        Self {
            reader,
            plan,
            group_of_item,
            kept_ok: HashMap::new(),
        }
    }

    /// `Ok(true)` si al menos una copia conservada sigue igual; `Err(())` si se cancelo.
    fn kept_copy_is_valid(&self, group: &GroupPlan, cancel: &AtomicBool) -> Result<bool, ()> {
        for kept in &group.kept {
            let expected = Expected {
                identity: kept.identity,
                size_bytes: kept.size_bytes,
                modified: kept.modified,
            };
            let outcome = open_verified(self.reader, Path::new(&kept.path), &expected)
                .and_then(|mut handle| hash_full(handle.as_mut(), &expected, cancel, |_| {}));
            match outcome {
                Ok(hash) if to_hex(&hash) == group.blake3 => return Ok(true),
                Err(HashFailure::Cancelled) => return Err(()),
                // Otra copia conservada puede estar bien.
                Ok(_) | Err(_) => {}
            }
        }
        Ok(false)
    }
}

/// Resultado y detalle de un fallo al abrir o leer la copia a eliminar.
fn skip_for(failure: HashFailure) -> GuardVerdict {
    match failure {
        HashFailure::Cancelled => GuardVerdict::Interrupted,
        HashFailure::IdentityMismatch => {
            GuardVerdict::Skip(CleanupOutcome::SkippedProtected, identity_changed())
        }
        HashFailure::Changed => GuardVerdict::Skip(CleanupOutcome::SkippedLocked, CONTENT_CHANGED),
        HashFailure::InUse => GuardVerdict::Skip(CleanupOutcome::SkippedLocked, IN_USE),
        HashFailure::Unreadable => {
            GuardVerdict::Skip(CleanupOutcome::SkippedLocked, CONTENT_UNREADABLE)
        }
        HashFailure::Cloud => GuardVerdict::Skip(CleanupOutcome::SkippedCloud, SKIPPED_CLOUD),
    }
}

impl<R: ContentReader> ItemGuard for DuplicateContentGuard<'_, R> {
    fn verify(
        &mut self,
        item: &CleanupItem,
        target: &DeleteTarget,
        cancel: &AtomicBool,
    ) -> GuardVerdict {
        let kept_unavailable =
            GuardVerdict::Skip(CleanupOutcome::SkippedProtected, KEPT_COPY_UNAVAILABLE);
        // Un item que no figura en el plan no deberia existir: ante la duda, no se mueve.
        let Some(&index) = self.group_of_item.get(&item.finding_id) else {
            return kept_unavailable;
        };
        let group = &self.plan.groups[index];

        // 1. La copia conservada, una sola vez por grupo.
        let kept_ok = match self.kept_ok.get(&index) {
            Some(known) => *known,
            None => match self.kept_copy_is_valid(group, cancel) {
                Ok(valid) => {
                    self.kept_ok.insert(index, valid);
                    valid
                }
                Err(()) => return GuardVerdict::Interrupted,
            },
        };
        if !kept_ok {
            return kept_unavailable;
        }

        // 2. La copia a eliminar: el archivo ABIERTO es el verificado y su contenido el del grupo.
        let checked = target.checked();
        let expected = Expected {
            identity: checked.identity,
            size_bytes: checked.size_bytes,
            modified: checked.modified,
        };
        let outcome = open_verified(self.reader, target.path(), &expected)
            .and_then(|mut handle| hash_full(handle.as_mut(), &expected, cancel, |_| {}));
        match outcome {
            Ok(hash) if to_hex(&hash) == group.blake3 => GuardVerdict::Proceed,
            Ok(_) => GuardVerdict::Skip(CleanupOutcome::SkippedLocked, CONTENT_CHANGED),
            Err(failure) => skip_for(failure),
        }
    }
}

#[cfg(test)]
#[path = "guard_tests.rs"]
mod tests;

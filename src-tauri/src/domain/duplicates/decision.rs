//! Validacion de las decisiones del usuario sobre los grupos (design D-10, D-12 pasos 2 a 4).
//!
//! Una decision dice, por grupo, que copias se CONSERVAN; las copias a eliminar son el resto del
//! grupo. Todo se valida aqui, en el dominio, antes de armar el preview: nunca se confia en la
//! interfaz (RN-DUP-04).

use std::collections::BTreeSet;

use crate::domain::duplicates::model::{StoredDuplicateFile, StoredDuplicateGroup};

/// Mayor entero que JavaScript representa sin perdida (`Number.MAX_SAFE_INTEGER`).
pub const MAX_SAFE_ID: i64 = (1 << 53) - 1;

/// Lo que el usuario decidio para un grupo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    pub group_id: i64,
    pub keep_file_ids: Vec<i64>,
}

/// Por que una decision no es valida.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DecisionError {
    /// Decisiones vacias, grupo repetido, id fuera de rango, copia repetida o de otro grupo, o
    /// una decision que conserva todas las copias.
    #[error("invalid argument: {0}")]
    InvalidArgument(&'static str),
    /// El grupo no existe en el escaneo.
    #[error("duplicate group not found")]
    GroupNotFound,
    /// El grupo ya se resolvio: no admite otra decision hasta un escaneo nuevo.
    #[error("duplicate group already resolved")]
    GroupResolved,
    /// La decision no conserva ninguna copia (eliminaria todas): RN-DUP-04.
    #[error("at least one copy must be kept")]
    KeepRequired,
}

/// Una decision ya validada contra su grupo.
#[derive(Debug, Clone)]
pub struct ValidatedDecision<'a> {
    pub group: &'a StoredDuplicateGroup,
    /// Copias a conservar, en el orden del grupo.
    pub kept: Vec<&'a StoredDuplicateFile>,
    /// Copias a eliminar (al menos una), en el orden del grupo (por ruta).
    pub delete: Vec<&'a StoredDuplicateFile>,
}

fn in_range(id: i64) -> bool {
    (0..=MAX_SAFE_ID).contains(&id)
}

/// Valida `decisions` contra `groups` (los grupos del escaneo que existen, con sus copias).
///
/// Orden de los chequeos: forma de la lista (no vacia, ids en rango, sin grupos repetidos); y por
/// cada decision, en orden: grupo existente, no resuelto, al menos una copia conservada, copias
/// conservadas validas (sin repetir y del grupo) y al menos una copia a eliminar.
pub fn validate_decisions<'a>(
    decisions: &[Decision],
    groups: &'a [StoredDuplicateGroup],
) -> Result<Vec<ValidatedDecision<'a>>, DecisionError> {
    if decisions.is_empty() {
        return Err(DecisionError::InvalidArgument("empty decisions"));
    }
    let mut seen_groups = BTreeSet::new();
    for decision in decisions {
        if !in_range(decision.group_id) || !decision.keep_file_ids.iter().all(|id| in_range(*id)) {
            return Err(DecisionError::InvalidArgument("id out of range"));
        }
        if !seen_groups.insert(decision.group_id) {
            return Err(DecisionError::InvalidArgument("repeated group"));
        }
    }
    let mut validated = Vec::with_capacity(decisions.len());
    for decision in decisions {
        let group = groups
            .iter()
            .find(|group| group.id == decision.group_id)
            .ok_or(DecisionError::GroupNotFound)?;
        if group.resolved_at.is_some() {
            return Err(DecisionError::GroupResolved);
        }
        if decision.keep_file_ids.is_empty() {
            return Err(DecisionError::KeepRequired);
        }
        let mut kept_ids = BTreeSet::new();
        for id in &decision.keep_file_ids {
            if !kept_ids.insert(*id) {
                return Err(DecisionError::InvalidArgument("repeated kept copy"));
            }
            if !group.files.iter().any(|file| file.id == *id) {
                return Err(DecisionError::InvalidArgument("copy is not in the group"));
            }
        }
        let (kept, delete): (Vec<_>, Vec<_>) = group
            .files
            .iter()
            .partition(|file| kept_ids.contains(&file.id));
        if delete.is_empty() {
            return Err(DecisionError::InvalidArgument("nothing to delete"));
        }
        validated.push(ValidatedDecision {
            group,
            kept,
            delete,
        });
    }
    Ok(validated)
}

#[cfg(test)]
#[path = "decision_tests.rs"]
mod tests;

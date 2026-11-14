//! Carpetas de contenido seleccionadas: estado por raiz, anidadas y atribucion (design D-01).
//!
//! El estado de una raiz se evalua con `metadata` SIN seguir reparse (no con la proteccion, porque
//! la carpeta en si es `anchor`): una raiz que no es un directorio comun no se recorre.

use std::path::PathBuf;

use crate::domain::duplicates::model::{RootReason, RootStatus};
use crate::domain::ports::{EntryKind, FileSystem, PortError};
use crate::domain::protection::context::{ContentFolder, ContextError, ProtectionContext};
use crate::domain::protection::path::NormalizedPath;

/// Plan de una carpeta de contenido para el escaneo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootPlan {
    pub folder: ContentFolder,
    pub status: RootStatus,
    /// Ruta de la carpeta segun el contexto de proteccion (`None` si no hay).
    pub path: Option<NormalizedPath>,
}

impl RootPlan {
    /// Las raices que se recorren son las `Done`.
    pub fn is_walked(&self) -> bool {
        self.status == RootStatus::Done
    }
}

/// Un plan por carpeta de contenido, en el orden de `ContentFolder::ALL`.
pub fn resolve_roots<F: FileSystem>(
    fs: &F,
    context: &Result<ProtectionContext, ContextError>,
    selected: &[ContentFolder],
) -> Vec<RootPlan> {
    let mut plans: Vec<RootPlan> = ContentFolder::ALL
        .into_iter()
        .map(|folder| {
            if !selected.contains(&folder) {
                return RootPlan {
                    folder,
                    status: RootStatus::NotSelected,
                    path: None,
                };
            }
            match context {
                Err(_) => RootPlan {
                    folder,
                    status: RootStatus::Unavailable(RootReason::ContextUnavailable),
                    path: None,
                },
                Ok(ctx) => evaluate(fs, ctx, folder),
            }
        })
        .collect();
    mark_nested(&mut plans);
    plans
}

fn evaluate<F: FileSystem>(fs: &F, ctx: &ProtectionContext, folder: ContentFolder) -> RootPlan {
    let path = ctx
        .user()
        .and_then(|user| user.content_folder(folder))
        .cloned();
    let unavailable = |reason: RootReason, path: Option<NormalizedPath>| RootPlan {
        folder,
        status: RootStatus::Unavailable(reason),
        path,
    };
    let Some(path) = path else {
        return unavailable(RootReason::NotFound, None);
    };
    // Otro volumen: ni se consulta (PA-12).
    if !ctx.supported_volumes().contains(&path.drive()) {
        return unavailable(RootReason::UnsupportedVolume, Some(path));
    }
    let reason = match fs.metadata(&PathBuf::from(path.to_string())) {
        Err(PortError::NotFound(_)) => Some(RootReason::NotFound),
        Err(_) => Some(RootReason::Inaccessible),
        // La nube tiene prioridad sobre el reparse (un reparse de nube informa ambos).
        Ok(meta) if meta.cloud_placeholder => Some(RootReason::Cloud),
        Ok(meta) => match meta.kind {
            EntryKind::ReparsePoint => Some(RootReason::ReparsePoint),
            EntryKind::File => Some(RootReason::NotADirectory),
            EntryKind::Directory => None,
        },
    };
    match reason {
        Some(reason) => unavailable(reason, Some(path)),
        None => RootPlan {
            folder,
            status: RootStatus::Done,
            path: Some(path),
        },
    }
}

/// Una raiz recorrible que esta dentro de otra recorrible pasa a `Nested` (se recorre una vez,
/// desde la de arriba). Con rutas iguales, la que viene despues en `ContentFolder::ALL` es la
/// anidada.
fn mark_nested(plans: &mut [RootPlan]) {
    let walked: Vec<(usize, NormalizedPath)> = plans
        .iter()
        .enumerate()
        .filter(|(_, plan)| plan.is_walked())
        .filter_map(|(index, plan)| plan.path.clone().map(|path| (index, path)))
        .collect();
    for (index, path) in &walked {
        let inside_other = walked.iter().any(|(other_index, other)| {
            other_index != index
                && path.starts_with(other)
                && (path.is_strictly_under(other) || other_index < index)
        });
        if inside_other {
            plans[*index].status = RootStatus::Nested;
        }
    }
}

/// Carpeta de contenido a la que se atribuye `path`: la mas profunda, entre las recorridas o
/// anidadas, que lo contiene.
pub fn owning_root(plans: &[RootPlan], path: &NormalizedPath) -> Option<ContentFolder> {
    plans
        .iter()
        .filter(|plan| matches!(plan.status, RootStatus::Done | RootStatus::Nested))
        .filter_map(|plan| plan.path.as_ref().map(|root| (plan.folder, root)))
        .filter(|(_, root)| path.starts_with(root))
        .max_by_key(|(_, root)| root.len())
        .map(|(folder, _)| folder)
}

#[cfg(test)]
#[path = "roots_tests.rs"]
mod tests;

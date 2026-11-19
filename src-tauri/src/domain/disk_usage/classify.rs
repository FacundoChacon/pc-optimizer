//! Clasificacion de carpetas (C-07, design D-04, D-07 b y D-14): opacas, de sistema y con
//! sugerencia de limpieza. Puro: solo mira la ruta y el contexto de proteccion.

use crate::domain::disk_usage::model::OpaqueReason;
use crate::domain::junk::sources::{JunkSource, Layout, sources};
use crate::domain::protection::context::ProtectionContext;
use crate::domain::protection::decision::{ProtectedReason, RuleVerdict};
use crate::domain::protection::path::{NormalizedPath, fold_component};
use crate::domain::protection::rules::evaluate_rules;

const RECYCLE_BIN: &str = "$Recycle.Bin";
const SYSTEM_VOLUME_INFORMATION: &str = "System Volume Information";

/// Clasifica carpetas contra un contexto de proteccion ya restringido por privilegios: sin
/// usuario (over-the-shoulder) todos los perfiles son ajenos y solo hay ubicaciones de maquina.
pub struct Classifier<'a> {
    ctx: &'a ProtectionContext,
    junk_sources: Vec<JunkSource>,
}

impl<'a> Classifier<'a> {
    pub fn new(ctx: &'a ProtectionContext) -> Self {
        Self {
            ctx,
            junk_sources: sources(ctx),
        }
    }

    /// Motivo por el que la carpeta es opaca (D-04). Solo evalua la carpeta en si: quien recorre
    /// hereda la opacidad hacia abajo.
    pub fn opaque(&self, path: &NormalizedPath) -> Option<OpaqueReason> {
        if path.len() == 1 {
            let name = path.components().first()?.folded();
            if name == fold_component(RECYCLE_BIN) {
                return Some(OpaqueReason::RecycleBin);
            }
            if name == fold_component(SYSTEM_VOLUME_INFORMATION) {
                return Some(OpaqueReason::SystemVolumeInformation);
            }
        }
        match evaluate_rules(self.ctx, path) {
            RuleVerdict::Protected(ProtectedReason::OtherUserProfile) => {
                Some(OpaqueReason::OtherUserProfile)
            }
            _ => None,
        }
    }

    /// Archivos de Windows o de programas, o entrada de sistema de la raiz del volumen (D-07 b).
    pub fn is_system(&self, path: &NormalizedPath) -> bool {
        path.starts_with(self.ctx.windows_dir())
            || self
                .ctx
                .program_files()
                .iter()
                .any(|root| path.starts_with(root))
            || path.starts_with(self.ctx.program_data())
            || matches!(
                evaluate_rules(self.ctx, path),
                RuleVerdict::Protected(ProtectedReason::VolumeSystemEntry)
            )
    }

    /// La carpeta es una ubicacion de la whitelist de basura o esta dentro de una (D-14).
    pub fn junk_hint(&self, path: &NormalizedPath) -> bool {
        self.junk_sources.iter().any(|source| match source.layout {
            Layout::Direct => path.starts_with(&source.root),
            Layout::Profiles { subdirs } => {
                // `<raiz>\<perfil>\<subcarpeta de cache>[\...]`
                path.strip_prefix(&source.root).is_some_and(|rest| {
                    rest.get(1).is_some_and(|component| {
                        subdirs
                            .iter()
                            .any(|name| fold_component(name) == component.folded())
                    })
                })
            }
        })
    }
}

#[cfg(test)]
#[path = "classify_tests.rs"]
mod tests;

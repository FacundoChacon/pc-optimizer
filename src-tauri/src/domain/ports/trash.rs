//! Puerto de la Papelera de reciclaje.

use std::path::Path;

use crate::domain::protection::service::DeleteTarget;

/// Por que no se pudo mover un elemento a la Papelera.
///
/// Las variantes se distinguen porque sus consecuencias son distintas para la limpieza: unas
/// omiten el elemento y siguen (`InUse`, `TooLarge`, ...), otras abortan la ejecucion
/// (`BinUnavailable`, `BackendUnavailable`). `Other` lleva un codigo ASCII estable
/// (`io_0x80270021`), nunca la descripcion del sistema.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TrashError {
    #[error("item in use by another process")]
    InUse,
    #[error("recycle bin unavailable on the volume")]
    BinUnavailable,
    #[error("item too large for the recycle bin")]
    TooLarge,
    #[error("path too long for the shell")]
    PathTooLong,
    #[error("permission denied")]
    PermissionDenied,
    #[error("item not found")]
    NotFound,
    #[error("recycle bin mechanism unusable")]
    BackendUnavailable,
    #[error("other failure: {0}")]
    Other(String),
}

/// Unica via de borrado: mover a la Papelera. No existe borrado permanente ni vaciado.
///
/// La unica entrada aceptada es un [`DeleteTarget`]: solo `PathProtection::prepare_delete` puede
/// construirlo, asi que no compila mover una ruta que no paso por la re-verificacion. El
/// adaptador MUST NOT seguir el ultimo componente: si el objetivo resulta ser un enlace, se
/// mueve el enlace y no su destino.
pub trait Trash {
    /// Comprobacion UNA vez por ejecucion, antes de mover el primer elemento (D-12 B): `Err` si
    /// la Papelera del volumen de `sample` no esta disponible o su politica no pudo leerse. No
    /// reemplaza el control por elemento que hace `move_to_trash`.
    fn preflight(&self, sample: &Path) -> Result<(), TrashError>;

    fn move_to_trash(&self, target: &DeleteTarget) -> Result<(), TrashError>;
}

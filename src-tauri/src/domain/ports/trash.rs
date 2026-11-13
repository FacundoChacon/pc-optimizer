//! Puerto de la Papelera de reciclaje.

use std::path::Path;

use crate::domain::ports::error::PortError;

/// Unica via de borrado: mover a la Papelera. No existe borrado permanente.
pub trait Trash {
    fn move_to_trash(&self, path: &Path) -> Result<(), PortError>;
}

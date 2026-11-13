//! Puerto de identidad de sesion: quien es el usuario del token del proceso y quien es el
//! usuario de la sesion interactiva (C-05, D-05/D-06). El dominio no consulta el SO.

use super::error::PortError;
use crate::domain::privileges::UserSid;

/// Consultas de identidad del proceso y de su sesion de Windows.
///
/// Cada metodo es independiente y puede fallar sin afectar a los demas; `Ok(None)` significa
/// "la senal no existe" (p. ej. no hay shell en la sesion) y `Err` "no se pudo consultar".
pub trait SessionIdentity {
    /// Usuario del token del proceso (siempre existe si la consulta funciona).
    fn process_user(&self) -> Result<UserSid, PortError>;
    /// Si el token del proceso esta elevado.
    fn process_elevated(&self) -> Result<bool, PortError>;
    /// Dueno del proceso de shell de la misma sesion que el proceso, si hay.
    fn shell_user(&self) -> Result<Option<UserSid>, PortError>;
    /// Usuario de logon de la misma sesion que el proceso, si hay.
    fn logon_user(&self) -> Result<Option<UserSid>, PortError>;
}

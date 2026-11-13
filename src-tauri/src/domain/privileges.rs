//! Contexto de privilegios (C-05): quien es el usuario del token del proceso frente al de la
//! sesion interactiva, de forma fail-closed, y el contrato de alcance para C-08/C-09/C-18.
//!
//! Todo es puro: la identidad llega por el puerto `SessionIdentity`. Los usuarios se comparan
//! por SID (bytes), nunca por nombre ni por variables de entorno.

use super::ports::SessionIdentity;
use super::protection::context::RawProtectionContext;

const SID_HEADER_LEN: usize = 8;
const SID_SUBAUTHORITY_LEN: usize = 4;
const SID_REVISION: u8 = 1;
const SID_MAX_SUBAUTHORITIES: usize = 15;

/// SID de usuario en su forma binaria (`SID` de Windows), validado estructuralmente.
///
/// Igualdad por bytes. Nunca se serializa ni se muestra: la UI solo recibe `PrivilegeContext`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UserSid(Vec<u8>);

impl UserSid {
    /// Acepta solo bytes con revision 1, hasta 15 subautoridades y longitud `8 + 4n`.
    pub fn from_bytes(bytes: Vec<u8>) -> Option<UserSid> {
        let (&revision, &count) = (bytes.first()?, bytes.get(1)?);
        let count = usize::from(count);
        let valid = revision == SID_REVISION
            && count <= SID_MAX_SUBAUTHORITIES
            && bytes.len() == SID_HEADER_LEN + SID_SUBAUTHORITY_LEN * count;
        valid.then_some(UserSid(bytes))
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Lo que se pudo averiguar de la identidad del proceso y de su sesion.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionEvidence {
    /// Usuario del token del proceso.
    pub process_user: Option<UserSid>,
    /// Dueno del shell de la sesion.
    pub shell_user: Option<UserSid>,
    /// Usuario de logon de la sesion.
    pub logon_user: Option<UserSid>,
}

/// Por que no se puede afirmar que el proceso corre con el usuario interactivo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MismatchReason {
    /// La sesion pertenece, sin duda, a otro usuario (over-the-shoulder).
    DifferentUser,
    /// No se pudo verificar (token ilegible, sin senales o senales contradictorias).
    Undetermined,
}

impl MismatchReason {
    /// Codigo estable que viaja por IPC.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DifferentUser => "different_user",
            Self::Undetermined => "undetermined",
        }
    }
}

/// Resultado de comparar el usuario del token con el de la sesion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserMatch {
    Match,
    Mismatch(MismatchReason),
}

/// Contexto de privilegios del proceso; se calcula una vez al arrancar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrivilegeContext {
    user_match: UserMatch,
    elevated: bool,
}

impl PrivilegeContext {
    pub fn new(user_match: UserMatch, elevated: bool) -> Self {
        Self {
            user_match,
            elevated,
        }
    }

    /// Contexto fail-closed cuando la deteccion no pudo ejecutarse.
    pub fn undetermined() -> Self {
        Self::new(UserMatch::Mismatch(MismatchReason::Undetermined), false)
    }

    pub fn user_match(&self) -> UserMatch {
        self.user_match
    }

    /// Solo informativo: nunca cambia la decision ni amplia lo que puede limpiarse (RN-PRIV-05).
    pub fn elevated(&self) -> bool {
        self.elevated
    }
}

/// Decide la coincidencia (D-06). `Match` solo si el token es legible, hay al menos una senal
/// y todas las senales coinciden con el. Senales iguales entre si pero distintas del token son
/// `DifferentUser`; cualquier otro caso es `Undetermined`.
pub fn decide(evidence: &SessionEvidence) -> UserMatch {
    let undetermined = UserMatch::Mismatch(MismatchReason::Undetermined);
    let Some(token) = &evidence.process_user else {
        return undetermined;
    };
    let mut signals = [&evidence.shell_user, &evidence.logon_user]
        .into_iter()
        .flatten();
    let Some(first) = signals.next() else {
        return undetermined;
    };
    if signals.any(|other| other != first) {
        return undetermined;
    }
    if first == token {
        UserMatch::Match
    } else {
        UserMatch::Mismatch(MismatchReason::DifferentUser)
    }
}

/// Calcula el contexto con el puerto. Nunca falla: un `Err` es una senal ausente y una
/// elevacion ilegible es `false`.
pub fn detect(identity: &impl SessionIdentity) -> PrivilegeContext {
    let evidence = SessionEvidence {
        process_user: identity.process_user().ok(),
        shell_user: identity.shell_user().ok().flatten(),
        logon_user: identity.logon_user().ok().flatten(),
    };
    let elevated = identity.process_elevated().unwrap_or(false);
    PrivilegeContext::new(decide(&evidence), elevated)
}

/// Alcance de una categoria de limpieza o de una mejora (D-09).
///
/// Guia para C-08/C-09/C-18: es `PerUser` todo lo que actua sobre el perfil o el registro del
/// usuario actual (`%TEMP%` del usuario, cachés de navegador, miniaturas, Papelera, partes
/// por-usuario de volcados, y TODA accion sobre HKCU); es `Machine` el resto (`C:\Windows\Temp`,
/// caché de Windows Update). Cada consumidor debe consultar `scope_access` al escanear y de nuevo
/// justo antes de actuar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserScope {
    Machine,
    PerUser,
}

/// Si un alcance puede usarse con el contexto actual.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeAccess {
    Allowed,
    Blocked(MismatchReason),
}

/// En `mismatch` el alcance por-usuario queda bloqueado con su motivo; el de maquina sigue.
pub fn scope_access(ctx: &PrivilegeContext, scope: UserScope) -> ScopeAccess {
    match (ctx.user_match(), scope) {
        (UserMatch::Mismatch(reason), UserScope::PerUser) => ScopeAccess::Blocked(reason),
        _ => ScopeAccess::Allowed,
    }
}

/// Ajusta el contexto crudo de proteccion al contexto de privilegios (D-10): en `mismatch` se
/// quita el usuario actual, de modo que toda ruta bajo la raiz de perfiles es perfil ajeno.
pub fn restrict_protection_context(
    mut raw: RawProtectionContext,
    ctx: &PrivilegeContext,
) -> RawProtectionContext {
    if matches!(ctx.user_match(), UserMatch::Mismatch(_)) {
        raw.user = None;
    }
    raw
}

#[cfg(test)]
#[path = "privileges_tests.rs"]
mod tests;

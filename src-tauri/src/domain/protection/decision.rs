//! Tipos de la decision de proteccion (design D-09).
//!
//! La decision es una enumeracion cerrada con codigos textuales estables: son claves del
//! catalogo de la UI y del `error_detail` del log.

use crate::domain::model::CleanupOutcome;
use crate::domain::ports::{EntryKind, FileIdentity, PortError};

use super::path::NormalizedPath;
pub use super::path::PathRejection;

/// Navegador cuyo perfil se protege.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Browser {
    Chrome,
    Edge,
    Brave,
    Firefox,
}

/// Excepciones de la lista cerrada (RN-PROT-06): subarboles estrictos de una raiz protegida.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExceptionId {
    WindowsTemp,
    WindowsUpdateDownload,
    Minidump,
    WerReportArchive,
    WerReportQueue,
    BrowserCache(Browser),
}

/// Que ancla del contexto (o raiz de excepcion) es la ruta.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnchorKind {
    UsersRoot,
    UserProfile,
    LocalAppData,
    RoamingAppData,
    WindowsDir,
    ProgramFiles,
    ProgramData,
    AppDir,
    Temp,
    KnownFolder,
    ExceptionRoot,
    /// Ancestro estricto de otra ancla o raiz protegida: borrarlo arrastraria lo protegido.
    AncestorOfProtectedRoot,
}

/// Clase de ancla; importa solo para validar raices de reglas personalizadas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorClass {
    /// Su contenido jamas es elegible ni puede ser raiz de una regla.
    Structural,
    /// Carpeta cuyo contenido puede limpiarse (Descargas, temporal, raiz de excepcion).
    Content,
}

impl AnchorKind {
    pub fn class(self) -> AnchorClass {
        match self {
            Self::Temp | Self::KnownFolder | Self::ExceptionRoot => AnchorClass::Content,
            Self::UsersRoot
            | Self::UserProfile
            | Self::LocalAppData
            | Self::RoamingAppData
            | Self::WindowsDir
            | Self::ProgramFiles
            | Self::ProgramData
            | Self::AppDir
            | Self::AncestorOfProtectedRoot => AnchorClass::Structural,
        }
    }
}

/// Por que no se pudo resolver la ruta (se traduce de `PortError` sin perder el tipo).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolveFailure {
    NotFound,
    PermissionDenied,
    InUse,
    Unsupported,
    Io,
    NotADirectory,
    MissingIdentity,
    PinFailed,
}

impl From<&PortError> for ResolveFailure {
    fn from(error: &PortError) -> Self {
        match error {
            PortError::NotFound(_) => Self::NotFound,
            PortError::PermissionDenied(_) => Self::PermissionDenied,
            PortError::InUse(_) => Self::InUse,
            PortError::Unsupported(_) => Self::Unsupported,
            PortError::Io(_) => Self::Io,
        }
    }
}

/// Motivo de proteccion (lista cerrada).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtectedReason {
    Unparseable(PathRejection),
    ContextUnavailable,
    UnsupportedVolume,
    VolumeRoot,
    VolumeSystemEntry,
    Anchor(AnchorKind),
    OtherUserProfile,
    WindowsDir,
    ProgramFiles,
    ProgramData,
    BrowserProfile(Browser),
    AppDir,
    ReparsePointInPath,
    ReparsePoint,
    MultipleHardLinks,
    ResolvedPathMismatch,
    IdentityChanged,
    Unresolvable(ResolveFailure),
}

impl ProtectedReason {
    /// Codigo textual estable (snake_case) para log y catalogo de la UI.
    pub fn code(self) -> &'static str {
        match self {
            Self::Unparseable(rejection) => match rejection {
                PathRejection::Empty => "empty_path",
                PathRejection::NonUnicode => "non_unicode_path",
                PathRejection::NotAbsolute => "not_absolute",
                PathRejection::NetworkPath => "network_path",
                PathRejection::DevicePath => "device_path",
                PathRejection::AlternateDataStream => "alternate_data_stream",
                PathRejection::InvalidCharacter => "invalid_character",
                PathRejection::ReservedDeviceName => "reserved_device_name",
                PathRejection::TrailingDotOrSpace => "trailing_dot_or_space",
                PathRejection::EscapesRoot => "escapes_root",
                PathRejection::VerbatimNotNormalized => "verbatim_not_normalized",
            },
            Self::ContextUnavailable => "context_unavailable",
            Self::UnsupportedVolume => "unsupported_volume",
            Self::VolumeRoot => "volume_root",
            Self::VolumeSystemEntry => "volume_system_entry",
            Self::Anchor(_) => "anchor",
            Self::OtherUserProfile => "other_user_profile",
            Self::WindowsDir => "windows_dir",
            Self::ProgramFiles => "program_files",
            Self::ProgramData => "program_data",
            Self::BrowserProfile(_) => "browser_profile",
            Self::AppDir => "app_dir",
            Self::ReparsePointInPath => "reparse_point_in_path",
            Self::ReparsePoint => "reparse_point",
            Self::MultipleHardLinks => "multiple_hard_links",
            Self::ResolvedPathMismatch => "resolved_path_mismatch",
            Self::IdentityChanged => "identity_changed",
            Self::Unresolvable(failure) => match failure {
                ResolveFailure::NotFound => "unresolvable_not_found",
                ResolveFailure::PermissionDenied => "unresolvable_permission_denied",
                ResolveFailure::InUse => "unresolvable_in_use",
                ResolveFailure::Unsupported => "unresolvable_unsupported",
                ResolveFailure::Io => "unresolvable_io",
                ResolveFailure::NotADirectory => "unresolvable_not_a_directory",
                ResolveFailure::MissingIdentity => "unresolvable_missing_identity",
                ResolveFailure::PinFailed => "unresolvable_pin_failed",
            },
        }
    }
}

/// Por que regla resulta elegible una ruta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllowRule {
    /// Fuera de toda raiz protegida.
    Default,
    /// Dentro de una excepcion de la lista cerrada.
    Exception(ExceptionId),
}

/// Veredicto de las reglas puras (sin I/O).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleVerdict {
    Allowed(AllowRule),
    Protected(ProtectedReason),
}

impl RuleVerdict {
    pub fn is_allowed(self) -> bool {
        matches!(self, Self::Allowed(_))
    }
}

/// Ruta certificada como elegible, con lo verificado en el sistema de archivos.
///
/// Certifica solo esa entrada: un directorio `Allowed` no hace elegible su contenido.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedPath {
    /// Ruta real con nombres largos (la que debe usarse para mover a la Papelera).
    pub path: NormalizedPath,
    pub kind: EntryKind,
    pub size_bytes: u64,
    pub identity: FileIdentity,
    pub rule: AllowRule,
}

/// Decision final del servicio (nunca un `Result`: fail-closed por construccion).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtectionDecision {
    Allowed(CheckedPath),
    SkippedCloud { path: NormalizedPath },
    Protected(ProtectedReason),
}

impl ProtectionDecision {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed(_))
    }

    /// Resultado de limpieza que corresponde a una omision; `None` si es elegible.
    pub fn skip_outcome(&self) -> Option<CleanupOutcome> {
        match self {
            Self::Allowed(_) => None,
            Self::SkippedCloud { .. } => Some(CleanupOutcome::SkippedCloud),
            Self::Protected(_) => Some(CleanupOutcome::SkippedProtected),
        }
    }
}

#[cfg(test)]
#[path = "decision_tests.rs"]
mod tests;

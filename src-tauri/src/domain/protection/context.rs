//! Contexto de proteccion inyectado (design D-05).
//!
//! Las ubicaciones del sistema y del usuario las resuelve el SO (adaptador) y llegan como
//! cadenas crudas; `ProtectionContext::from_raw` las normaliza y valida. Las reglas no contienen
//! letras de unidad ni rutas fijas: todo es relativo a estas anclas.

use super::path::{NormalizedPath, PathRejection, normalize_lexical};

/// De donde salen las carpetas del usuario actual.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserSource {
    /// Token del proceso (C-03).
    ProcessToken,
    /// Usuario de la sesion interactiva (costura para C-05).
    InteractiveSession,
}

/// Carpetas crudas del usuario actual, tal como las informa el SO.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawUserFolders {
    pub profile: String,
    pub local_app_data: String,
    pub roaming_app_data: String,
    pub temp: Option<String>,
    /// Desktop, Documents, Downloads, Music, Pictures, Videos.
    pub content_folders: Vec<String>,
    pub source: UserSource,
}

/// Contexto crudo: lo llena el adaptador; las cadenas no estan validadas.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RawProtectionContext {
    /// `FOLDERID_Windows` (obligatoria).
    pub windows_dir: Option<String>,
    /// `FOLDERID_ProgramFiles*` (al menos una).
    pub program_files: Vec<String>,
    /// `FOLDERID_ProgramData` (obligatoria).
    pub program_data: Option<String>,
    /// `FOLDERID_UserProfiles` (obligatoria).
    pub users_root: Option<String>,
    /// `None` => toda la raiz de perfiles es ajena.
    pub user: Option<RawUserFolders>,
    /// Datos de la app y carpeta del ejecutable.
    pub app_dirs: Vec<String>,
}

/// Por que el contexto no es utilizable (todo pasa a `Protected(ContextUnavailable)`).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ContextError {
    #[error("falta la ubicacion obligatoria `{0}`")]
    MissingRequired(&'static str),
    #[error("la ubicacion `{location}` no es una ruta interpretable: {rejection:?}")]
    Unnormalizable {
        location: &'static str,
        rejection: PathRejection,
    },
    #[error("la ubicacion `{0}` es la raiz de un volumen")]
    VolumeRootAnchor(&'static str),
    #[error("el perfil del usuario coincide con la raiz de perfiles")]
    ProfileIsUsersRoot,
}

/// Carpetas validadas del usuario actual.
#[derive(Debug, Clone)]
pub struct UserContext {
    profile: NormalizedPath,
    local_app_data: NormalizedPath,
    roaming_app_data: NormalizedPath,
    temp: Option<NormalizedPath>,
    content_folders: Vec<NormalizedPath>,
    source: UserSource,
}

impl UserContext {
    pub fn profile(&self) -> &NormalizedPath {
        &self.profile
    }

    pub fn local_app_data(&self) -> &NormalizedPath {
        &self.local_app_data
    }

    pub fn roaming_app_data(&self) -> &NormalizedPath {
        &self.roaming_app_data
    }

    pub fn temp(&self) -> Option<&NormalizedPath> {
        self.temp.as_ref()
    }

    pub fn content_folders(&self) -> &[NormalizedPath] {
        &self.content_folders
    }

    pub fn source(&self) -> UserSource {
        self.source
    }
}

/// Contexto validado: anclas normalizadas y volumenes soportados.
#[derive(Debug, Clone)]
pub struct ProtectionContext {
    system_drive: char,
    supported_volumes: Vec<char>,
    windows_dir: NormalizedPath,
    program_files: Vec<NormalizedPath>,
    program_data: NormalizedPath,
    users_root: NormalizedPath,
    user: Option<UserContext>,
    app_dirs: Vec<NormalizedPath>,
}

/// Normaliza una ancla: debe ser interpretable y no ser la raiz de un volumen.
fn anchor(location: &'static str, raw: &str) -> Result<NormalizedPath, ContextError> {
    let path = normalize_lexical(raw).map_err(|rejection| ContextError::Unnormalizable {
        location,
        rejection,
    })?;
    if path.is_volume_root() {
        return Err(ContextError::VolumeRootAnchor(location));
    }
    Ok(path)
}

fn required(location: &'static str, raw: Option<&String>) -> Result<NormalizedPath, ContextError> {
    anchor(
        location,
        raw.ok_or(ContextError::MissingRequired(location))?,
    )
}

impl ProtectionContext {
    /// Valida y normaliza el contexto crudo (fail-closed: ante la duda, `Err`).
    ///
    /// - Obligatorias (`windows_dir`, `program_files`, `program_data`, `users_root`) y `app_dirs`:
    ///   ausente, no interpretable o raiz de volumen => error.
    /// - Carpeta opcional del usuario (`temp`, `content_folders`) no interpretable => se
    ///   descarta (solo agregaria proteccion); si es raiz de volumen => error.
    /// - Si no se puede resolver `profile`, `local_app_data` o `roaming_app_data` => sin usuario
    ///   actual (toda la raiz de perfiles queda como ajena).
    pub fn from_raw(raw: RawProtectionContext) -> Result<Self, ContextError> {
        let windows_dir = required("windows_dir", raw.windows_dir.as_ref())?;
        let program_data = required("program_data", raw.program_data.as_ref())?;
        let users_root = required("users_root", raw.users_root.as_ref())?;
        if raw.program_files.is_empty() {
            return Err(ContextError::MissingRequired("program_files"));
        }
        let program_files = raw
            .program_files
            .iter()
            .map(|path| anchor("program_files", path))
            .collect::<Result<Vec<_>, _>>()?;
        let app_dirs = raw
            .app_dirs
            .iter()
            .map(|path| anchor("app_dirs", path))
            .collect::<Result<Vec<_>, _>>()?;
        let user = match raw.user {
            Some(folders) => Self::user_from_raw(folders, &users_root)?,
            None => None,
        };
        let system_drive = windows_dir.drive();
        Ok(Self {
            system_drive,
            supported_volumes: vec![system_drive],
            windows_dir,
            program_files,
            program_data,
            users_root,
            user,
            app_dirs,
        })
    }

    fn user_from_raw(
        raw: RawUserFolders,
        users_root: &NormalizedPath,
    ) -> Result<Option<UserContext>, ContextError> {
        let anchors = (
            anchor("profile", &raw.profile),
            anchor("local_app_data", &raw.local_app_data),
            anchor("roaming_app_data", &raw.roaming_app_data),
        );
        let (profile, local_app_data, roaming_app_data) = match anchors {
            (Ok(profile), Ok(local), Ok(roaming)) => (profile, local, roaming),
            // Una ancla de usuario que es raiz de volumen es un contexto corrupto, no "sin usuario".
            (Err(error @ ContextError::VolumeRootAnchor(_)), _, _)
            | (_, Err(error @ ContextError::VolumeRootAnchor(_)), _)
            | (_, _, Err(error @ ContextError::VolumeRootAnchor(_))) => return Err(error),
            _ => return Ok(None),
        };
        if profile == *users_root {
            return Err(ContextError::ProfileIsUsersRoot);
        }
        let temp = match raw.temp.as_deref() {
            Some(text) => optional_anchor("temp", text)?,
            None => None,
        };
        let mut content_folders = Vec::new();
        for text in &raw.content_folders {
            content_folders.extend(optional_anchor("content_folders", text)?);
        }
        Ok(Some(UserContext {
            profile,
            local_app_data,
            roaming_app_data,
            temp,
            content_folders,
            source: raw.source,
        }))
    }

    /// Unidad del sistema (la de `windows_dir`).
    pub fn system_drive(&self) -> char {
        self.system_drive
    }

    /// Volumenes soportados para limpieza (PA-12: solo el del sistema).
    pub fn supported_volumes(&self) -> &[char] {
        &self.supported_volumes
    }

    pub fn windows_dir(&self) -> &NormalizedPath {
        &self.windows_dir
    }

    pub fn program_files(&self) -> &[NormalizedPath] {
        &self.program_files
    }

    pub fn program_data(&self) -> &NormalizedPath {
        &self.program_data
    }

    pub fn users_root(&self) -> &NormalizedPath {
        &self.users_root
    }

    pub fn user(&self) -> Option<&UserContext> {
        self.user.as_ref()
    }

    pub fn app_dirs(&self) -> &[NormalizedPath] {
        &self.app_dirs
    }
}

/// Ancla opcional: no interpretable => se descarta; raiz de volumen => error.
fn optional_anchor(
    location: &'static str,
    raw: &str,
) -> Result<Option<NormalizedPath>, ContextError> {
    match anchor(location, raw) {
        Ok(path) => Ok(Some(path)),
        Err(ContextError::Unnormalizable { .. }) => Ok(None),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
#[path = "context_tests.rs"]
mod tests;

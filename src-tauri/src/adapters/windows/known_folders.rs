//! Resolucion del contexto de proteccion desde el SO (design D-05).
//!
//! Las anclas salen de `SHGetKnownFolderPath`, nunca de variables de entorno (se heredan del
//! usuario que lanzo el proceso y son triviales de falsear). La unica excepcion es la carpeta
//! temporal, que solo agrega proteccion/excepcion sobre el perfil ya resuelto.

use std::ffi::c_void;
use std::path::Path;

use windows::Win32::Storage::FileSystem::GetTempPathW;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::{
    FOLDERID_Desktop, FOLDERID_Documents, FOLDERID_Downloads, FOLDERID_LocalAppData,
    FOLDERID_Music, FOLDERID_Pictures, FOLDERID_Profile, FOLDERID_ProgramData,
    FOLDERID_ProgramFiles, FOLDERID_ProgramFilesX64, FOLDERID_ProgramFilesX86,
    FOLDERID_RoamingAppData, FOLDERID_UserProfiles, FOLDERID_Videos, FOLDERID_Windows,
    KF_FLAG_DEFAULT, SHGetKnownFolderPath,
};
use windows::core::GUID;

use super::fs::WindowsFileSystem;
use crate::domain::ports::FileSystem;
use crate::domain::protection::context::{RawProtectionContext, RawUserFolders, UserSource};

/// De donde salen las carpetas del usuario actual.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserFolderSource {
    /// Token del proceso (`hToken = NULL`). C-05 sumara el usuario de la sesion interactiva.
    ProcessToken,
}

/// Carpetas conocidas que consulta el ensamblado.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum KnownFolder {
    Windows,
    ProgramFiles,
    ProgramFilesX86,
    ProgramFilesX64,
    ProgramData,
    UserProfiles,
    Profile,
    LocalAppData,
    RoamingAppData,
    Desktop,
    Documents,
    Downloads,
    Music,
    Pictures,
    Videos,
}

/// Fuente de rutas; la real consulta el SO y los tests inyectan una falsa.
pub(crate) trait FolderResolver {
    fn known(&self, folder: KnownFolder) -> Option<String>;
    fn temp(&self) -> Option<String>;
}

/// Carpetas de contenido del usuario (anclas `Content`).
const CONTENT_FOLDERS: [KnownFolder; 6] = [
    KnownFolder::Desktop,
    KnownFolder::Documents,
    KnownFolder::Downloads,
    KnownFolder::Music,
    KnownFolder::Pictures,
    KnownFolder::Videos,
];

/// Ensambla el contexto crudo con lo que informa `resolver` (funcion pura).
///
/// - Obligatoria que falla (`windows_dir`, `program_data`, `users_root`) => campo `None`.
/// - Program Files: las variantes que resuelven, sin repetidas (el contexto exige al menos una).
/// - Si falla `profile`, `local_app_data` o `roaming_app_data` => `user = None`.
/// - Opcional que falla (`temp`, carpeta de contenido) => se omite.
pub(crate) fn assemble(
    resolver: &impl FolderResolver,
    source: UserSource,
    app_dirs: Vec<String>,
) -> RawProtectionContext {
    let mut program_files: Vec<String> = Vec::new();
    for folder in [
        KnownFolder::ProgramFiles,
        KnownFolder::ProgramFilesX86,
        KnownFolder::ProgramFilesX64,
    ] {
        if let Some(path) = resolver.known(folder)
            && !program_files
                .iter()
                .any(|seen| seen.eq_ignore_ascii_case(&path))
        {
            program_files.push(path);
        }
    }
    let user = match (
        resolver.known(KnownFolder::Profile),
        resolver.known(KnownFolder::LocalAppData),
        resolver.known(KnownFolder::RoamingAppData),
    ) {
        (Some(profile), Some(local_app_data), Some(roaming_app_data)) => Some(RawUserFolders {
            profile,
            local_app_data,
            roaming_app_data,
            temp: resolver.temp(),
            content_folders: CONTENT_FOLDERS
                .iter()
                .filter_map(|folder| resolver.known(*folder))
                .collect(),
            source,
        }),
        _ => None,
    };
    RawProtectionContext {
        windows_dir: resolver.known(KnownFolder::Windows),
        program_files,
        program_data: resolver.known(KnownFolder::ProgramData),
        users_root: resolver.known(KnownFolder::UserProfiles),
        user,
        app_dirs,
    }
}

/// Resuelve el contexto crudo de la maquina. Nunca falla: lo que no se resuelve queda ausente y
/// `ProtectionContext::from_raw` decide (fail-closed) si el contexto es utilizable.
pub fn resolve_protection_context(
    user: UserFolderSource,
    app_dirs: Vec<String>,
) -> RawProtectionContext {
    match user {
        UserFolderSource::ProcessToken => {
            assemble(&SystemResolver, UserSource::ProcessToken, app_dirs)
        }
    }
}

/// Resolvedor real: `SHGetKnownFolderPath` con el token del proceso.
struct SystemResolver;

impl SystemResolver {
    fn guid(folder: KnownFolder) -> GUID {
        match folder {
            KnownFolder::Windows => FOLDERID_Windows,
            KnownFolder::ProgramFiles => FOLDERID_ProgramFiles,
            KnownFolder::ProgramFilesX86 => FOLDERID_ProgramFilesX86,
            KnownFolder::ProgramFilesX64 => FOLDERID_ProgramFilesX64,
            KnownFolder::ProgramData => FOLDERID_ProgramData,
            KnownFolder::UserProfiles => FOLDERID_UserProfiles,
            KnownFolder::Profile => FOLDERID_Profile,
            KnownFolder::LocalAppData => FOLDERID_LocalAppData,
            KnownFolder::RoamingAppData => FOLDERID_RoamingAppData,
            KnownFolder::Desktop => FOLDERID_Desktop,
            KnownFolder::Documents => FOLDERID_Documents,
            KnownFolder::Downloads => FOLDERID_Downloads,
            KnownFolder::Music => FOLDERID_Music,
            KnownFolder::Pictures => FOLDERID_Pictures,
            KnownFolder::Videos => FOLDERID_Videos,
        }
    }
}

impl FolderResolver for SystemResolver {
    fn known(&self, folder: KnownFolder) -> Option<String> {
        let id = Self::guid(folder);
        // SAFETY: `id` es un GUID valido que vive durante la llamada; `hToken = None` es el
        // token del proceso; la API no retiene los punteros.
        let pwstr = unsafe { SHGetKnownFolderPath(&raw const id, KF_FLAG_DEFAULT, None) }.ok()?;
        // SAFETY: `pwstr` lo devolvio `SHGetKnownFolderPath` con exito: apunta a una cadena
        // UTF-16 terminada en nulo que sigue asignada hasta el `CoTaskMemFree` de abajo.
        let text = unsafe { pwstr.to_string() }.ok();
        // SAFETY: `pwstr.0` se asigno con `CoTaskMemAlloc` (contrato de la API), se libera una
        // sola vez y no se usa despues; `text` ya es una copia propia.
        unsafe { CoTaskMemFree(Some(pwstr.0.cast_const().cast::<c_void>())) };
        text.filter(|path| !path.is_empty())
    }

    fn temp(&self) -> Option<String> {
        // `GetTempPathW` (no `GetTempPath2W`: no existe en Windows 10, que es objetivo). Puede
        // devolver forma 8.3 y barra final: se expande con el puerto y se recorta.
        let mut buffer = vec![0u16; 32_768];
        // SAFETY: `buffer` es un slice escribible cuya longitud recibe la API; solo se lee el
        // prefijo que la API informa haber escrito.
        let written = unsafe { GetTempPathW(Some(&mut buffer)) } as usize;
        if written == 0 || written >= buffer.len() {
            return None;
        }
        let raw = String::from_utf16(&buffer[..written]).ok()?;
        let trimmed = raw.trim_end_matches('\\');
        let expanded = WindowsFileSystem
            .canonicalize(Path::new(trimmed))
            .ok()
            .and_then(|path| path.to_str().map(str::to_owned));
        Some(expanded.unwrap_or_else(|| trimmed.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::domain::protection::context::ProtectionContext;

    #[derive(Default)]
    struct FakeResolver {
        folders: HashMap<KnownFolder, String>,
        temp: Option<String>,
    }

    impl FakeResolver {
        /// Maquina tipica: sistema en `C:`, usuario `ana`.
        fn typical() -> Self {
            let mut folders = HashMap::new();
            for (folder, path) in [
                (KnownFolder::Windows, r"C:\Windows"),
                (KnownFolder::ProgramFiles, r"C:\Program Files"),
                (KnownFolder::ProgramFilesX86, r"C:\Program Files (x86)"),
                (KnownFolder::ProgramFilesX64, r"C:\Program Files"),
                (KnownFolder::ProgramData, r"C:\ProgramData"),
                (KnownFolder::UserProfiles, r"C:\Users"),
                (KnownFolder::Profile, r"C:\Users\ana"),
                (KnownFolder::LocalAppData, r"C:\Users\ana\AppData\Local"),
                (KnownFolder::RoamingAppData, r"C:\Users\ana\AppData\Roaming"),
                (KnownFolder::Desktop, r"C:\Users\ana\Desktop"),
                (KnownFolder::Documents, r"C:\Users\ana\Documents"),
                (KnownFolder::Downloads, r"C:\Users\ana\Downloads"),
                (KnownFolder::Music, r"C:\Users\ana\Music"),
                (KnownFolder::Pictures, r"C:\Users\ana\Pictures"),
                (KnownFolder::Videos, r"C:\Users\ana\Videos"),
            ] {
                folders.insert(folder, path.to_owned());
            }
            Self {
                folders,
                temp: Some(r"C:\Users\ana\AppData\Local\Temp".to_owned()),
            }
        }

        fn without(mut self, folder: KnownFolder) -> Self {
            self.folders.remove(&folder);
            self
        }
    }

    impl FolderResolver for FakeResolver {
        fn known(&self, folder: KnownFolder) -> Option<String> {
            self.folders.get(&folder).cloned()
        }
        fn temp(&self) -> Option<String> {
            self.temp.clone()
        }
    }

    fn assemble_typical(resolver: &FakeResolver) -> RawProtectionContext {
        assemble(
            resolver,
            UserSource::ProcessToken,
            vec![r"C:\Users\ana\AppData\Local\pc-optimizer".to_owned()],
        )
    }

    #[test]
    fn assemble_should_fill_every_field_when_all_folders_resolve() {
        let raw = assemble_typical(&FakeResolver::typical());
        assert_eq!(raw.windows_dir.as_deref(), Some(r"C:\Windows"));
        assert_eq!(raw.program_data.as_deref(), Some(r"C:\ProgramData"));
        assert_eq!(raw.users_root.as_deref(), Some(r"C:\Users"));
        assert_eq!(
            raw.program_files,
            vec![r"C:\Program Files", r"C:\Program Files (x86)"]
        );
        assert_eq!(
            raw.app_dirs,
            vec![r"C:\Users\ana\AppData\Local\pc-optimizer"]
        );
        let user = raw.user.unwrap();
        assert_eq!(user.profile, r"C:\Users\ana");
        assert_eq!(user.local_app_data, r"C:\Users\ana\AppData\Local");
        assert_eq!(user.roaming_app_data, r"C:\Users\ana\AppData\Roaming");
        assert_eq!(
            user.temp.as_deref(),
            Some(r"C:\Users\ana\AppData\Local\Temp")
        );
        assert_eq!(user.content_folders.len(), 6);
        assert_eq!(user.source, UserSource::ProcessToken);
    }

    #[test]
    fn assemble_should_leave_a_mandatory_folder_that_fails_as_none() {
        let no_windows = assemble_typical(&FakeResolver::typical().without(KnownFolder::Windows));
        assert_eq!(no_windows.windows_dir, None);
        assert!(no_windows.program_data.is_some());
        let no_data = assemble_typical(&FakeResolver::typical().without(KnownFolder::ProgramData));
        assert_eq!(no_data.program_data, None);
        let no_users =
            assemble_typical(&FakeResolver::typical().without(KnownFolder::UserProfiles));
        assert_eq!(no_users.users_root, None);
        // Y el contexto resultante es inutilizable (fail-closed).
        assert!(ProtectionContext::from_raw(no_data).is_err());
    }

    #[test]
    fn assemble_should_collect_only_the_program_files_that_resolve_without_duplicates() {
        let resolver = FakeResolver::typical().without(KnownFolder::ProgramFilesX86);
        assert_eq!(
            assemble_typical(&resolver).program_files,
            vec![r"C:\Program Files"]
        );
        let none = FakeResolver::typical()
            .without(KnownFolder::ProgramFiles)
            .without(KnownFolder::ProgramFilesX86)
            .without(KnownFolder::ProgramFilesX64);
        let raw = assemble_typical(&none);
        assert!(raw.program_files.is_empty());
        assert!(ProtectionContext::from_raw(raw).is_err());
    }

    #[test]
    fn assemble_should_drop_optional_folders_that_fail() {
        let mut resolver = FakeResolver::typical()
            .without(KnownFolder::Downloads)
            .without(KnownFolder::Music);
        resolver.temp = None;
        let user = assemble_typical(&resolver).user.unwrap();
        assert_eq!(user.temp, None);
        assert_eq!(user.content_folders.len(), 4);
        assert!(
            user.content_folders
                .iter()
                .all(|path| !path.ends_with("Downloads") && !path.ends_with("Music"))
        );
    }

    /// Solo lectura: consulta las carpetas conocidas de esta maquina, sin tocar el disco.
    #[test]
    #[ignore = "depende del perfil real del usuario (falla en cuentas de servicio sin perfil); correr a mano con --ignored"]
    fn resolve_protection_context_should_build_a_valid_context_on_this_machine() {
        let app_dirs = vec![r"C:\ruta\inventada\de\la\app".to_owned()];
        let raw = resolve_protection_context(UserFolderSource::ProcessToken, app_dirs.clone());
        assert_eq!(raw.app_dirs, app_dirs);
        let ctx = ProtectionContext::from_raw(raw).unwrap();
        let user = ctx.user().expect("el proceso tiene perfil de usuario");
        assert!(user.profile().is_strictly_under(ctx.users_root()));
        assert_eq!(ctx.system_drive(), ctx.windows_dir().drive());
        assert_eq!(ctx.supported_volumes(), [ctx.windows_dir().drive()]);
        assert!(user.local_app_data().starts_with(user.profile()));
        assert!(user.temp().is_some());
        assert_eq!(user.content_folders().len(), 6);
        assert_eq!(user.source(), UserSource::ProcessToken);
        assert!(!ctx.program_files().is_empty());
    }

    #[test]
    fn assemble_should_report_no_current_user_when_a_user_anchor_fails() {
        for missing in [
            KnownFolder::Profile,
            KnownFolder::LocalAppData,
            KnownFolder::RoamingAppData,
        ] {
            let raw = assemble_typical(&FakeResolver::typical().without(missing));
            assert_eq!(raw.user, None, "sin {missing:?}");
            assert!(raw.windows_dir.is_some());
        }
    }
}

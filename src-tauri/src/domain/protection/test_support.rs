//! Contextos de prueba reutilizables (solo tests).

use super::context::{ProtectionContext, RawProtectionContext, RawUserFolders, UserSource};

/// Contexto crudo de una maquina con Windows en `<drive>:\Windows` y usuario `user`.
pub(crate) fn raw_for(drive: char, user: &str) -> RawProtectionContext {
    let d = |rest: &str| format!(r"{drive}:\{rest}");
    let profile = d(&format!(r"Users\{user}"));
    RawProtectionContext {
        windows_dir: Some(d("Windows")),
        program_files: vec![d("Program Files"), d("Program Files (x86)")],
        program_data: Some(d("ProgramData")),
        users_root: Some(d("Users")),
        user: Some(RawUserFolders {
            local_app_data: format!(r"{profile}\AppData\Local"),
            roaming_app_data: format!(r"{profile}\AppData\Roaming"),
            temp: Some(format!(r"{profile}\AppData\Local\Temp")),
            content_folders: [
                "Desktop",
                "Documents",
                "Downloads",
                "Music",
                "Pictures",
                "Videos",
            ]
            .iter()
            .map(|name| format!(r"{profile}\{name}"))
            .collect(),
            profile,
            source: UserSource::ProcessToken,
        }),
        app_dirs: vec![d(&format!(r"Users\{user}\AppData\Local\pc-optimizer"))],
    }
}

/// Contexto crudo estandar: sistema en `C:`, usuario `ana`.
pub(crate) fn raw_c_ana() -> RawProtectionContext {
    raw_for('C', "ana")
}

/// Contexto valido estandar: sistema en `C:`, usuario `ana`.
pub(crate) fn ctx_c_ana() -> ProtectionContext {
    ctx_from(raw_c_ana())
}

/// Contexto valido con el sistema en `D:` y usuario `ana`.
pub(crate) fn ctx_d_ana() -> ProtectionContext {
    ctx_from(raw_for('D', "ana"))
}

/// Contexto valido con el sistema en `C:` y el usuario indicado.
pub(crate) fn ctx_c_user(user: &str) -> ProtectionContext {
    ctx_from(raw_for('C', user))
}

/// Contexto valido sin usuario actual conocido.
pub(crate) fn ctx_c_no_user() -> ProtectionContext {
    let mut raw = raw_c_ana();
    raw.user = None;
    ctx_from(raw)
}

fn ctx_from(raw: RawProtectionContext) -> ProtectionContext {
    match ProtectionContext::from_raw(raw) {
        Ok(ctx) => ctx,
        Err(error) => panic!("el contexto de prueba debe ser valido: {error}"),
    }
}

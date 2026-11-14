//! Politica de la Papelera leida del registro (design D-12 B).
//!
//! El shell destruye el archivo (sin error) si la Papelera del volumen esta deshabilitada o el
//! archivo no entra: este adaptador lee las senales para que `WindowsTrash` se niegue ANTES de
//! llamar al shell. Solo lee: nunca escribe el registro. Fail-closed: cualquier error de lectura
//! distinto de "no existe la clave/valor" es `Err` y el llamador lo trata como no disponible.
//!
//! Claves (algunas no estan documentadas formalmente por Microsoft; ver design Riesgos):
//! - `HKLM|HKCU\Software\Microsoft\Windows\CurrentVersion\Policies\Explorer\NoRecycleFiles`
//! - `HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\BitBucket\NukeOnDelete` (global)
//! - `HKCU\...\Explorer\BitBucket\Volume\{GUID}\NukeOnDelete` y `\MaxCapacity` (MB)
//!
//! Unico archivo del change con `unsafe` nuevo: tres bloques minimos, cada uno con su `// SAFETY:`.

use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows::Win32::Storage::FileSystem::{GetVolumeNameForVolumeMountPointW, GetVolumePathNameW};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RegGetValueW,
};
use windows::core::PCWSTR;

use super::privileges::port_error;
use crate::domain::ports::{
    BinAvailability, PortError, RawBinPolicy, RecycleBinPolicy, evaluate_policy,
};

const POLICIES_EXPLORER: &str = r"Software\Microsoft\Windows\CurrentVersion\Policies\Explorer";
const BITBUCKET: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\BitBucket";
/// `\\?\Volume{GUID}\` ocupa 49 caracteres mas el nulo; con margen.
const VOLUME_NAME_CHARS: usize = 64;

/// Politica real de la Papelera (solo lectura del registro).
#[derive(Debug, Default, Clone, Copy)]
pub struct WindowsRecycleBinPolicy;

/// Clave de volumen del registro (`{GUID}`) a partir de `\\?\Volume{GUID}\`; `None` si no lo es.
pub(crate) fn volume_key(volume_name: &str) -> Option<String> {
    let rest = volume_name.strip_prefix(r"\\?\Volume")?;
    let rest = rest.strip_suffix(std::path::MAIN_SEPARATOR).unwrap_or(rest);
    let inner = rest.strip_prefix('{')?.strip_suffix('}')?;
    let groups: Vec<&str> = inner.split('-').collect();
    let shape = [8, 4, 4, 4, 12];
    let valid = groups.len() == shape.len()
        && groups
            .iter()
            .zip(shape)
            .all(|(g, n)| g.len() == n && g.chars().all(|c| c.is_ascii_hexdigit()));
    valid.then(|| format!("{{{inner}}}"))
}

fn wide_nul(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Lee un DWORD. `Ok(None)` solo si la clave o el valor no existen; todo otro error es `Err`
/// (tipo distinto, acceso denegado, ...).
fn read_dword(root: HKEY, subkey: &str, value: &str) -> Result<Option<u32>, PortError> {
    let subkey = wide_nul(subkey);
    let value = wide_nul(value);
    let mut data: u32 = 0;
    let mut size: u32 = 4;
    // SAFETY: `subkey` y `value` son buffers UTF-16 terminados en nulo que viven hasta despues
    // de la llamada; `data` y `size` son locales validos (4 bytes) usados como salida unica;
    // `root` es una raiz predefinida; la API no retiene ningun puntero.
    let status = unsafe {
        RegGetValueW(
            root,
            PCWSTR(subkey.as_ptr()),
            PCWSTR(value.as_ptr()),
            RRF_RT_REG_DWORD,
            None,
            Some((&raw mut data).cast()),
            Some(&raw mut size),
        )
    };
    if status == ERROR_SUCCESS {
        Ok(Some(data))
    } else if status == ERROR_FILE_NOT_FOUND {
        Ok(None)
    } else {
        Err(PortError::Io(format!(
            "RegGetValueW: win32 error {}",
            status.0
        )))
    }
}

/// Clave de registro del volumen que contiene `path`.
fn volume_key_of(path: &Path) -> Result<String, PortError> {
    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    if wide.contains(&0) {
        return Err(PortError::Io("path contains a NUL".to_owned()));
    }
    wide.push(0);
    let mut root = vec![0u16; wide.len().max(261)];
    // SAFETY: `wide` es UTF-16 terminado en nulo que vive durante la llamada; `root` es un
    // buffer propio y la API recibe su longitud por el slice; no se retienen punteros.
    unsafe { GetVolumePathNameW(PCWSTR(wide.as_ptr()), &mut root) }
        .map_err(|e| port_error("GetVolumePathNameW", e.code().0))?;
    let mut name = [0u16; VOLUME_NAME_CHARS];
    // SAFETY: `root` quedo terminado en nulo por `GetVolumePathNameW` y vive durante la
    // llamada; `name` es un buffer propio cuya longitud viaja en el slice.
    unsafe { GetVolumeNameForVolumeMountPointW(PCWSTR(root.as_ptr()), &mut name) }
        .map_err(|e| port_error("GetVolumeNameForVolumeMountPointW", e.code().0))?;
    let end = name.iter().position(|&c| c == 0).unwrap_or(name.len());
    volume_key(&String::from_utf16_lossy(&name[..end]))
        .ok_or_else(|| PortError::Io("volume name is not a volume GUID".to_owned()))
}

/// Valores crudos del volumen de `path` (tambien los usa el test de solo lectura).
pub(crate) fn read_raw_policy(path: &Path) -> Result<RawBinPolicy, PortError> {
    let key = volume_key_of(path)?;
    let volume = format!(r"{BITBUCKET}\Volume\{key}");
    Ok(RawBinPolicy {
        no_recycle_files_machine: read_dword(
            HKEY_LOCAL_MACHINE,
            POLICIES_EXPLORER,
            "NoRecycleFiles",
        )?,
        no_recycle_files_user: read_dword(HKEY_CURRENT_USER, POLICIES_EXPLORER, "NoRecycleFiles")?,
        nuke_on_delete_global: read_dword(HKEY_CURRENT_USER, BITBUCKET, "NukeOnDelete")?,
        nuke_on_delete_volume: read_dword(HKEY_CURRENT_USER, &volume, "NukeOnDelete")?,
        max_capacity_mb: read_dword(HKEY_CURRENT_USER, &volume, "MaxCapacity")?,
    })
}

impl RecycleBinPolicy for WindowsRecycleBinPolicy {
    fn availability(&self, path: &Path) -> Result<BinAvailability, PortError> {
        read_raw_policy(path).map(|raw| evaluate_policy(&raw))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_key_should_extract_the_braced_guid_from_a_volume_name() {
        assert_eq!(
            volume_key(r"\\?\Volume{55d47c52-c40e-45dd-8616-c6f1342ecb74}\"),
            Some("{55d47c52-c40e-45dd-8616-c6f1342ecb74}".to_owned())
        );
        assert_eq!(
            volume_key(r"\\?\Volume{ABCDEF01-0000-0000-0000-000000000000}"),
            Some("{ABCDEF01-0000-0000-0000-000000000000}".to_owned())
        );
    }

    #[test]
    fn volume_key_should_reject_names_that_are_not_a_volume_guid() {
        for bad in [
            "",
            r"C:\",
            r"\\?\Volume{}\",
            r"\\?\Volume{not a guid}\",
            r"\\server\share\",
            r"\\?\Volume{55d47c52-c40e-45dd-8616-c6f1342ecb74}\..\x",
        ] {
            assert_eq!(volume_key(bad), None, "{bad}");
        }
    }

    /// Solo lectura del registro: imprime la politica REAL de `C:` (nada se escribe).
    #[test]
    #[ignore = "lee el registro real de la maquina (solo lectura); correr a mano con --ignored"]
    fn real_policy_should_print_the_values_read_for_the_system_volume() {
        let path = Path::new(r"C:\Windows");
        let raw = read_raw_policy(path);
        let availability = WindowsRecycleBinPolicy.availability(path);
        eprintln!("RAW {path:?}: {raw:?}");
        eprintln!("DISPONIBILIDAD: {availability:?}");
        assert!(raw.is_ok(), "la lectura debe poder determinarse: {raw:?}");
    }
}

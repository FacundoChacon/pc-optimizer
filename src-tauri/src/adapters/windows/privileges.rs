//! Identidad de sesion real (C-05, D-05/D-14/D-15).
//!
//! Responde las cuatro preguntas del puerto `SessionIdentity` con APIs documentadas de Win32:
//! el usuario del token del proceso, si esta elevado, el dueno del shell de la MISMA sesion y
//! el usuario de logon de esa sesion (WTS). Los SID se copian a `Vec<u8>` antes de soltar el
//! buffer del token. Unico archivo de C-05 con `unsafe`: cada bloque es minimo y lleva su
//! `// SAFETY:`; la logica pura (nombres, codigos de error) vive fuera de los bloques.

use std::ffi::c_void;

use windows::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_BAD_LENGTH, ERROR_INSUFFICIENT_BUFFER, HANDLE,
};
use windows::Win32::Security::{
    GetLengthSid, GetTokenInformation, IsValidSid, LookupAccountNameW, PSID, SID_NAME_USE,
    TOKEN_ELEVATION, TOKEN_INFORMATION_CLASS, TOKEN_QUERY, TOKEN_USER, TokenElevation, TokenUser,
};
use windows::Win32::System::RemoteDesktop::{
    ProcessIdToSessionId, WTS_CURRENT_SERVER_HANDLE, WTS_INFO_CLASS, WTSDomainName, WTSFreeMemory,
    WTSQuerySessionInformationW, WTSUserName,
};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, OpenProcess, OpenProcessToken,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{GetShellWindow, GetWindowThreadProcessId};
use windows::core::{Owned, PCWSTR, PWSTR};

use crate::domain::ports::{PortError, SessionIdentity};
use crate::domain::privileges::UserSid;

/// Separador `DOMINIO\usuario` (escrito como escape unicode para no depender del literal).
const DOMAIN_SEPARATOR: char = '\u{5c}';

/// Prefijo de los HRESULT que envuelven un codigo Win32 (`HRESULT_FROM_WIN32`).
const WIN32_HRESULT_PREFIX: u32 = 0x8007_0000;

/// Une dominio y usuario de WTS en `DOMINIO\usuario`; sin usuario no hay cuenta.
pub(crate) fn account_name(domain: &str, user: &str) -> Option<String> {
    if user.is_empty() {
        return None;
    }
    if domain.is_empty() {
        return Some(user.to_owned());
    }
    Some(format!("{domain}{DOMAIN_SEPARATOR}{user}"))
}

/// Convierte bytes de SID ya copiados en un `UserSid` validado estructuralmente.
pub(crate) fn sid_from_copied_bytes(bytes: &[u8]) -> Option<UserSid> {
    UserSid::from_bytes(bytes.to_vec())
}

/// Codigo Win32 contenido en un HRESULT de `windows::core::Error`, si es de esa familia.
pub(crate) fn win32_code(hresult: i32) -> Option<u32> {
    let bits = u32::from_ne_bytes(hresult.to_ne_bytes());
    (bits & 0xFFFF_0000 == WIN32_HRESULT_PREFIX).then_some(bits & 0xFFFF)
}

/// Error tipado del puerto para una falla de `operation`; acceso denegado se distingue.
pub(crate) fn port_error(operation: &str, hresult: i32) -> PortError {
    match win32_code(hresult) {
        Some(code) if code == ERROR_ACCESS_DENIED.0 => {
            PortError::PermissionDenied(format!("{operation}: access denied"))
        }
        Some(code) => PortError::Io(format!("{operation}: win32 error {code}")),
        None => PortError::Io(format!(
            "{operation}: hresult {:#010x}",
            u32::from_ne_bytes(hresult.to_ne_bytes())
        )),
    }
}

/// Codigos con los que una llamada de dimensionado informa el tamano necesario.
fn is_size_probe(code: Option<u32>) -> bool {
    code == Some(ERROR_INSUFFICIENT_BUFFER.0) || code == Some(ERROR_BAD_LENGTH.0)
}

fn api_error(operation: &str, error: &windows::core::Error) -> PortError {
    port_error(operation, error.code().0)
}

/// Lee un bloque de `GetTokenInformation` (dos llamadas: tamano y datos).
///
/// Devuelve un buffer de `u64` para garantizar la alineacion de 8 bytes de `TOKEN_USER`.
fn token_information(
    token: HANDLE,
    class: TOKEN_INFORMATION_CLASS,
    operation: &str,
) -> Result<Vec<u64>, PortError> {
    let mut needed = 0u32;
    // SAFETY: llamada de dimensionado: sin buffer (`None`, longitud 0) y `needed` es un
    // puntero valido a un u32 local; `token` es un handle de token abierto por el llamador.
    let probe = unsafe { GetTokenInformation(token, class, None, 0, &mut needed) };
    match probe {
        // Las clases de tamano fijo (p. ej. TokenElevation) responden ERROR_BAD_LENGTH.
        Err(error) if is_size_probe(win32_code(error.code().0)) && needed > 0 => {}
        Err(error) => return Err(api_error(operation, &error)),
        Ok(()) => {
            return Err(PortError::Io(format!(
                "{operation}: empty token information"
            )));
        }
    }
    let mut buffer = vec![0u64; usize::try_from(needed).unwrap_or(0).div_ceil(8)];
    let mut written = 0u32;
    // SAFETY: `buffer` tiene al menos `needed` bytes (div_ceil a u64), esta alineado a 8 y
    // vive hasta despues de la llamada; `written` es un puntero valido a un u32 local.
    unsafe {
        GetTokenInformation(
            token,
            class,
            Some(buffer.as_mut_ptr().cast::<c_void>()),
            needed,
            &mut written,
        )
    }
    .map_err(|error| api_error(operation, &error))?;
    Ok(buffer)
}

/// SID del usuario de un token, copiado a un `UserSid` propio.
fn token_user_sid(token: HANDLE) -> Result<UserSid, PortError> {
    let buffer = token_information(token, TokenUser, "GetTokenInformation(TokenUser)")?;
    // SAFETY: `buffer` lo lleno `GetTokenInformation(TokenUser)` con un `TOKEN_USER` valido y
    // esta alineado a 8; solo se lee el campo `Sid` (un puntero dentro del mismo `buffer`).
    let sid = unsafe { (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid };
    // SAFETY: `sid` apunta dentro de `buffer`, que sigue vivo; `IsValidSid` solo lo inspecciona.
    if !unsafe { IsValidSid(sid) }.as_bool() {
        return Err(PortError::Io("token user SID is not valid".into()));
    }
    // SAFETY: `IsValidSid` confirmo que `sid` es un SID valido dentro de `buffer`.
    let length = unsafe { GetLengthSid(sid) } as usize;
    // SAFETY: un SID valido ocupa exactamente `GetLengthSid` bytes legibles a partir de `sid`,
    // dentro de `buffer`; se copian ahora, antes de que `buffer` se libere al salir.
    let bytes = unsafe { std::slice::from_raw_parts(sid.0.cast::<u8>(), length) };
    sid_from_copied_bytes(bytes).ok_or_else(|| PortError::Io("token user SID is malformed".into()))
}

/// Abre el token (solo consulta) de un proceso; se cierra solo al soltar el `Owned`.
fn open_token(process: HANDLE, operation: &str) -> Result<Owned<HANDLE>, PortError> {
    let mut token = HANDLE::default();
    // SAFETY: `process` es un handle de proceso valido (seudo-handle propio o abierto por
    // nosotros) y `token` es un puntero valido a un HANDLE local.
    unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) }
        .map_err(|error| api_error(operation, &error))?;
    // SAFETY: `OpenProcessToken` devolvio OK, asi que `token` es un handle recien abierto cuya
    // propiedad pasa al `Owned`, que lo cierra exactamente una vez.
    Ok(unsafe { Owned::new(token) })
}

/// Id del proceso actual.
fn current_pid() -> u32 {
    // SAFETY: sin argumentos ni punteros; solo lee el id del proceso actual.
    unsafe { GetCurrentProcessId() }
}

/// Sesion de Windows de un proceso.
fn session_of(pid: u32) -> Result<u32, PortError> {
    let mut session = 0u32;
    // SAFETY: `session` es un puntero valido a un u32 local; el pid solo se consulta.
    unsafe { ProcessIdToSessionId(pid, &mut session) }
        .map_err(|error| api_error("ProcessIdToSessionId", &error))?;
    Ok(session)
}

/// Buffer de texto devuelto por WTS: se libera con `WTSFreeMemory` exactamente una vez.
struct WtsString(PWSTR);

impl WtsString {
    fn query(session: u32, class: WTS_INFO_CLASS, operation: &str) -> Result<Self, PortError> {
        let mut buffer = PWSTR::null();
        let mut bytes = 0u32;
        // SAFETY: servidor local (`WTS_CURRENT_SERVER_HANDLE`); `buffer` y `bytes` son
        // punteros validos a locales. En exito WTS asigna `buffer`, que pasa al guard.
        unsafe {
            WTSQuerySessionInformationW(
                Some(WTS_CURRENT_SERVER_HANDLE),
                session,
                class,
                &mut buffer,
                &mut bytes,
            )
        }
        .map_err(|error| api_error(operation, &error))?;
        Ok(Self(buffer))
    }

    fn to_text(&self) -> Result<String, PortError> {
        if self.0.is_null() {
            return Ok(String::new());
        }
        // SAFETY: WTS devuelve una cadena UTF-16 terminada en nulo que vive hasta el `Drop`.
        unsafe { self.0.to_string() }.map_err(|_| PortError::Io("WTS text is not UTF-16".into()))
    }
}

impl Drop for WtsString {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: el puntero lo asigno `WTSQuerySessionInformationW`, este guard es su
            // unico dueno y `drop` corre una sola vez; no se usa despues de liberar.
            unsafe { WTSFreeMemory(self.0.0.cast::<c_void>()) };
        }
    }
}

/// SID de una cuenta `DOMINIO\usuario` resuelto por el SO (dos llamadas a `LookupAccountNameW`).
fn lookup_account_sid(account: &str) -> Result<UserSid, PortError> {
    let wide: Vec<u16> = account.encode_utf16().chain(std::iter::once(0)).collect();
    let mut sid_len = 0u32;
    let mut domain_len = 0u32;
    let mut sid_use = SID_NAME_USE::default();
    // SAFETY: llamada de dimensionado: sin buffers de salida; `wide` es una cadena terminada
    // en nulo que vive durante la llamada y los tres punteros son locales validos.
    let probe = unsafe {
        LookupAccountNameW(
            PCWSTR::null(),
            PCWSTR(wide.as_ptr()),
            None,
            &mut sid_len,
            None,
            &mut domain_len,
            &mut sid_use,
        )
    };
    match probe {
        Err(error)
            if win32_code(error.code().0) == Some(ERROR_INSUFFICIENT_BUFFER.0) && sid_len > 0 => {}
        Err(error) => return Err(api_error("LookupAccountNameW", &error)),
        Ok(()) => return Err(PortError::Io("LookupAccountNameW: empty result".into())),
    }
    let mut sid = vec![0u8; sid_len as usize];
    let mut domain = vec![0u16; domain_len as usize];
    // SAFETY: `sid` y `domain` tienen exactamente los tamanos que informo la llamada de
    // dimensionado y viven durante la llamada; `wide` sigue vivo y terminado en nulo.
    unsafe {
        LookupAccountNameW(
            PCWSTR::null(),
            PCWSTR(wide.as_ptr()),
            Some(PSID(sid.as_mut_ptr().cast::<c_void>())),
            &mut sid_len,
            Some(PWSTR(domain.as_mut_ptr())),
            &mut domain_len,
            &mut sid_use,
        )
    }
    .map_err(|error| api_error("LookupAccountNameW", &error))?;
    sid.truncate(sid_len as usize);
    sid_from_copied_bytes(&sid).ok_or_else(|| PortError::Io("account SID is malformed".into()))
}

/// Identidad de sesion consultada al SO. Sin estado: cada llamada vuelve a consultar.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemSessionIdentity;

impl SessionIdentity for SystemSessionIdentity {
    fn process_user(&self) -> Result<UserSid, PortError> {
        // SAFETY: devuelve el seudo-handle del proceso actual; no hay que cerrarlo.
        let process = unsafe { GetCurrentProcess() };
        let token = open_token(process, "OpenProcessToken(self)")?;
        token_user_sid(*token)
    }

    fn process_elevated(&self) -> Result<bool, PortError> {
        // SAFETY: devuelve el seudo-handle del proceso actual; no hay que cerrarlo.
        let process = unsafe { GetCurrentProcess() };
        let token = open_token(process, "OpenProcessToken(self)")?;
        let buffer = token_information(
            *token,
            TokenElevation,
            "GetTokenInformation(TokenElevation)",
        )?;
        // SAFETY: `buffer` lo lleno `GetTokenInformation(TokenElevation)` con un
        // `TOKEN_ELEVATION` valido (un u32) y esta alineado a 8.
        let elevation = unsafe { (*buffer.as_ptr().cast::<TOKEN_ELEVATION>()).TokenIsElevated };
        Ok(elevation != 0)
    }

    fn shell_user(&self) -> Result<Option<UserSid>, PortError> {
        // SAFETY: sin argumentos; devuelve el HWND del escritorio-shell o nulo si no hay.
        let shell = unsafe { GetShellWindow() };
        if shell.0.is_null() {
            return Ok(None);
        }
        let mut pid = 0u32;
        // SAFETY: `shell` es el HWND devuelto arriba y `pid` es un puntero valido a un u32 local.
        let thread = unsafe { GetWindowThreadProcessId(shell, Some(&mut pid)) };
        if thread == 0 || pid == 0 {
            return Ok(None);
        }
        if session_of(pid)? != session_of(current_pid())? {
            return Ok(None);
        }
        // SAFETY: `pid` es un id de proceso; `OpenProcess` no toma punteros.
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }
            .map_err(|error| api_error("OpenProcess(shell)", &error))?;
        // SAFETY: `OpenProcess` devolvio OK: handle recien abierto cuya propiedad pasa al
        // `Owned`, que lo cierra exactamente una vez.
        let process = unsafe { Owned::new(process) };
        let token = open_token(*process, "OpenProcessToken(shell)")?;
        token_user_sid(*token).map(Some)
    }

    fn logon_user(&self) -> Result<Option<UserSid>, PortError> {
        let session = session_of(current_pid())?;
        let user = WtsString::query(session, WTSUserName, "WTSQuerySessionInformationW(user)")?
            .to_text()?;
        let domain = WtsString::query(
            session,
            WTSDomainName,
            "WTSQuerySessionInformationW(domain)",
        )?
        .to_text()?;
        match account_name(&domain, &user) {
            Some(account) => lookup_account_sid(&account).map(Some),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
#[path = "privileges_tests.rs"]
mod tests;

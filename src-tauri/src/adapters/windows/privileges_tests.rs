//! Tests del adaptador de identidad de sesion. Los que consultan el token, la sesion o el
//! shell reales van `#[ignore]` y se corren a mano con `--ignored`.

use super::*;

fn well_known_system_sid() -> Vec<u8> {
    // S-1-5-18
    vec![1, 1, 0, 0, 0, 0, 0, 5, 18, 0, 0, 0]
}

#[test]
fn account_name_should_join_domain_and_user() {
    assert_eq!(account_name("PC", "ana").as_deref(), Some(r"PC\ana"));
    assert_eq!(
        account_name("MAQUINA-2", "luis").as_deref(),
        Some(r"MAQUINA-2\luis")
    );
}

#[test]
fn account_name_should_return_none_for_an_empty_user() {
    assert_eq!(account_name("PC", ""), None);
    assert_eq!(account_name("", ""), None);
}

#[test]
fn account_name_should_use_only_the_user_when_the_domain_is_empty() {
    assert_eq!(account_name("", "ana").as_deref(), Some("ana"));
}

#[test]
fn account_name_should_keep_spaces_and_unicode() {
    assert_eq!(
        account_name("PC", "Ana María").as_deref(),
        Some(r"PC\Ana María")
    );
    assert_eq!(account_name("PC-ñ", "jose").as_deref(), Some(r"PC-ñ\jose"));
}

#[test]
fn sid_from_copied_bytes_should_accept_a_well_formed_sid() {
    let sid = sid_from_copied_bytes(&well_known_system_sid()).unwrap();
    assert_eq!(sid.as_bytes(), well_known_system_sid().as_slice());
}

#[test]
fn sid_from_copied_bytes_should_reject_invalid_buffers() {
    assert_eq!(sid_from_copied_bytes(&[]), None);
    assert_eq!(sid_from_copied_bytes(&[1, 1, 0, 0, 0, 0, 0, 5]), None);
    let mut bad_revision = well_known_system_sid();
    bad_revision[0] = 2;
    assert_eq!(sid_from_copied_bytes(&bad_revision), None);
}

#[test]
fn win32_code_should_extract_the_code_from_win32_hresults_only() {
    assert_eq!(win32_code(0x8007_0005_u32 as i32), Some(5));
    assert_eq!(win32_code(0x8007_007A_u32 as i32), Some(122));
    assert_eq!(win32_code(0x8000_4005_u32 as i32), None);
    assert_eq!(win32_code(0), None);
}

#[test]
fn port_error_should_map_access_denied_to_permission_denied() {
    let error = port_error("OpenProcess", 0x8007_0005_u32 as i32);
    assert!(matches!(error, PortError::PermissionDenied(ref m) if m.contains("OpenProcess")));
}

#[test]
fn port_error_should_map_other_failures_to_io_with_context() {
    let win32 = port_error("OpenProcessToken", 0x8007_0057_u32 as i32);
    assert!(
        matches!(win32, PortError::Io(ref m) if m.contains("OpenProcessToken") && m.contains("87"))
    );
    let other = port_error("GetShellWindow", 0x8000_4005_u32 as i32);
    assert!(matches!(other, PortError::Io(ref m) if m.contains("GetShellWindow")));
}

// --- Consultas reales (ignoradas: leen el token, la sesion y el shell de esta maquina) ---

#[test]
#[ignore = "consulta el token y la sesion reales de esta maquina; correr a mano con --ignored"]
fn process_user_should_be_readable_and_stable() {
    let first = SystemSessionIdentity.process_user().unwrap();
    let second = SystemSessionIdentity.process_user().unwrap();
    assert_eq!(first, second);
    assert!(first.as_bytes().len() >= 12);
}

#[test]
#[ignore = "consulta el token y la sesion reales de esta maquina; correr a mano con --ignored"]
fn process_elevated_should_answer_without_error() {
    let elevated = SystemSessionIdentity.process_elevated().unwrap();
    println!("process_elevated = {elevated}");
}

#[test]
#[ignore = "consulta el token y la sesion reales de esta maquina; correr a mano con --ignored"]
fn session_signals_should_agree_with_the_process_user() {
    let process = SystemSessionIdentity.process_user().unwrap();
    let logon = SystemSessionIdentity.logon_user().unwrap();
    let shell = SystemSessionIdentity.shell_user().unwrap();
    println!(
        "logon_present = {}, shell_present = {}",
        logon.is_some(),
        shell.is_some()
    );
    assert!(logon.is_some() || shell.is_some(), "sin senales de sesion");
    if let Some(logon) = logon {
        assert_eq!(logon, process);
    }
    if let Some(shell) = shell {
        assert_eq!(shell, process);
    }
}

#[test]
#[ignore = "consulta el token y la sesion reales de esta maquina; correr a mano con --ignored"]
fn detect_should_match_on_a_normal_session_and_report_timing() {
    use crate::domain::privileges::{UserMatch, detect};
    let start = std::time::Instant::now();
    let context = detect(&SystemSessionIdentity);
    let elapsed = start.elapsed();
    println!("detect() = {context:?} in {elapsed:?}");
    assert_eq!(context.user_match(), UserMatch::Match);
}

#[test]
#[ignore = "consulta el token y la sesion reales de esta maquina; correr a mano con --ignored"]
fn lookup_account_sid_should_fail_for_an_unknown_account() {
    let error = lookup_account_sid("NO-EXISTE-PCOPT-XYZ").unwrap_err();
    assert!(matches!(
        error,
        PortError::Io(_) | PortError::PermissionDenied(_)
    ));
}

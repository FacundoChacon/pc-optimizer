//! Verifica el manifest embebido en los ejecutables (D-01, D-03): lector PE seguro, sin crates.

#[path = "support/pe_manifest.rs"]
mod pe_manifest;

use pe_manifest::read_manifest;

/// Literal presente en el binario de este test fuera de su recurso de manifest.
const TEXT_OUTSIDE_RESOURCE: &str = "requireAdministrator";

fn own_exe_bytes() -> Vec<u8> {
    let path = std::env::current_exe().expect("ruta del test");
    std::fs::read(path).expect("leer el propio ejecutable")
}

#[test]
fn read_manifest_should_reject_non_pe_text_without_panicking() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("nota.txt");
    std::fs::write(&file, format!("hola {TEXT_OUTSIDE_RESOURCE}")).expect("escribir");
    let bytes = std::fs::read(&file).expect("leer");
    assert!(read_manifest(&bytes).is_err());
    assert!(read_manifest(&[]).is_err());
}

#[test]
fn read_manifest_should_reject_truncated_executable() {
    let bytes = own_exe_bytes();
    assert!(read_manifest(&bytes[..512]).is_err());
    assert!(read_manifest(&bytes[..bytes.len() / 2]).is_err());
}

#[test]
fn read_manifest_should_report_own_test_binary_as_invoker_with_common_controls() {
    let info = read_manifest(&own_exe_bytes()).expect("manifest del propio test");
    // El literal TEXT_OUTSIDE_RESOURCE vive en el binario pero fuera del recurso.
    assert_eq!(TEXT_OUTSIDE_RESOURCE, "requireAdministrator");
    assert_eq!(info.execution_level.as_deref(), Some("asInvoker"));
    assert!(info.common_controls_v6);
}

#[test]
fn read_manifest_should_never_panic_on_corrupt_pe_headers() {
    let mut bytes = own_exe_bytes();
    // e_lfanew apunta mas alla del final del archivo.
    bytes[0x3C..0x40].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(read_manifest(&bytes).is_err());
    // Solo la firma MZ.
    assert!(read_manifest(b"MZ").is_err());
    // Cortes en muchos puntos distintos: siempre Err o Ok, nunca panico.
    let full = own_exe_bytes();
    for len in (0..full.len()).step_by(4099) {
        let _ = read_manifest(&full[..len]);
    }
}

#[test]
fn app_binary_should_match_build_profile_execution_level() {
    let bytes =
        std::fs::read(env!("CARGO_BIN_EXE_pc-optimizer")).expect("leer el binario de la app");
    let info = read_manifest(&bytes).expect("manifest del binario de la app");
    let expected = if cfg!(debug_assertions) {
        "asInvoker"
    } else {
        "requireAdministrator"
    };
    assert_eq!(info.execution_level.as_deref(), Some(expected));
    assert!(info.common_controls_v6);
}

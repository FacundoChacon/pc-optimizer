//! Tests de `WindowsVolumeSpace`: solo sobre la unidad de un `TempDir` (corren siempre).

use std::path::Path;

use tempfile::TempDir;

use super::*;
use crate::domain::ports::{PortError, VolumeSpace};

#[test]
fn space_should_report_total_above_zero_and_free_not_above_total() {
    let dir = TempDir::new().unwrap();
    let info = WindowsVolumeSpace.space(dir.path()).unwrap();
    assert!(info.total_bytes > 0);
    assert!(info.free_bytes <= info.total_bytes);
}

#[test]
fn space_should_answer_the_same_total_for_a_root_and_a_folder_of_the_same_drive() {
    let dir = TempDir::new().unwrap();
    let nested = dir.path().join("sub");
    std::fs::create_dir(&nested).unwrap();
    let from_folder = WindowsVolumeSpace.space(&nested).unwrap();
    let from_temp = WindowsVolumeSpace.space(dir.path()).unwrap();
    assert_eq!(from_folder.total_bytes, from_temp.total_bytes);
}

#[test]
fn space_should_fail_with_a_port_error_for_a_missing_path() {
    let dir = TempDir::new().unwrap();
    let missing = dir.path().join("no-existe").join("tampoco");
    let error = WindowsVolumeSpace.space(&missing).unwrap_err();
    assert!(
        matches!(error, PortError::NotFound(_)),
        "se esperaba NotFound: {error:?}"
    );
    let rootless = WindowsVolumeSpace.space(Path::new("")).unwrap_err();
    assert!(
        matches!(rootless, PortError::NotFound(_) | PortError::Io(_)),
        "{rootless:?}"
    );
}

#[test]
fn map_hresult_should_translate_win32_codes_to_the_port_errors() {
    let path = Path::new(r"C:\x");
    let win32 = |code: u32| 0x8007_0000_u32.wrapping_add(code) as i32;
    assert!(matches!(
        map_hresult(win32(2), path),
        PortError::NotFound(_)
    ));
    assert!(matches!(
        map_hresult(win32(3), path),
        PortError::NotFound(_)
    ));
    assert!(matches!(
        map_hresult(win32(5), path),
        PortError::PermissionDenied(_)
    ));
    assert!(matches!(map_hresult(win32(1234), path), PortError::Io(_)));
    assert!(matches!(
        map_hresult(0x8000_4005_u32 as i32, path),
        PortError::Io(_)
    ));
}

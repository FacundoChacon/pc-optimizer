//! Tests del Explorador real. NINGUNO abre el Explorador: usan un backend inyectado. El unico
//! test que lo abre esta marcado `#[ignore]` y solo se corre en el smoke guiado.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::*;
use crate::domain::ports::{Explorer, PortError};

fn explorer(
    backend: impl Fn(&Path) -> Result<(), BackendError> + Send + Sync + 'static,
) -> WindowsExplorer {
    WindowsExplorer::with_backend(backend)
}

#[test]
fn reveal_should_succeed_and_hand_the_exact_path_to_the_backend() {
    let seen: Arc<Mutex<Vec<(PathBuf, String)>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    let shell = explorer(move |path| {
        let name = std::thread::current()
            .name()
            .unwrap_or_default()
            .to_string();
        if let Ok(mut seen) = sink.lock() {
            seen.push((path.to_path_buf(), name));
        }
        Ok(())
    });
    assert_eq!(
        shell.reveal(Path::new(r"C:\Users\ana\Videos\viaje.mp4")),
        Ok(())
    );
    assert_eq!(shell.reveal(Path::new(r"C:\")), Ok(()));
    let seen = seen.lock().unwrap().clone();
    assert_eq!(
        seen,
        vec![
            (
                PathBuf::from(r"C:\Users\ana\Videos\viaje.mp4"),
                "pcopt-reveal".to_string()
            ),
            (PathBuf::from(r"C:\"), "pcopt-reveal".to_string()),
        ]
    );
}

#[test]
fn reveal_should_map_hresults_to_a_stable_code_without_system_text() {
    let error = explorer(|_| Err(BackendError::Hresult(0x8007_0002_u32 as i32)))
        .reveal(Path::new(r"C:\x"))
        .unwrap_err();
    assert_eq!(error, PortError::Unsupported("io_0x80070002".into()));
    let other = explorer(|_| Err(BackendError::Hresult(0x8000_4005_u32 as i32)))
        .reveal(Path::new(r"C:\x"))
        .unwrap_err();
    assert_eq!(other, PortError::Unsupported("io_0x80004005".into()));
}

#[test]
fn reveal_should_fail_when_com_cannot_be_initialised_or_the_path_has_a_nul() {
    let init = explorer(|_| Err(BackendError::Init))
        .reveal(Path::new(r"C:\x"))
        .unwrap_err();
    assert_eq!(init, PortError::Unsupported("com_init".into()));
    let nul = explorer(|_| Ok(()))
        .reveal(Path::new("C:\\x\0y"))
        .unwrap_err();
    assert_eq!(nul, PortError::Unsupported("invalid_path".into()));
}

#[test]
fn reveal_should_turn_a_panic_into_a_port_error_and_stay_usable() {
    let error = explorer(|_| panic!("el shell exploto"))
        .reveal(Path::new(r"C:\x"))
        .unwrap_err();
    assert_eq!(error, PortError::Unsupported("explorer_unavailable".into()));
    assert_eq!(explorer(|_| Ok(())).reveal(Path::new(r"C:\x")), Ok(()));
}

/// Abre el Explorador REAL: solo en el smoke guiado (`cargo test -- --ignored real_explorer`).
#[test]
#[ignore = "abre UI nativa; se prueba en el smoke"]
fn real_explorer_should_show_the_windows_folder() {
    let result = WindowsExplorer::new().reveal(Path::new(r"C:\Windows"));
    assert!(result.is_ok(), "{result:?}");
}

//! Tests del selector real. NINGUNO abre el dialogo nativo: usan un backend inyectado. El unico
//! test que abre UI esta marcado `#[ignore]` y solo se corre en el smoke guiado.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use super::*;
use crate::domain::ports::{FolderPicker, PortError};

type Seen = Vec<(Option<isize>, String)>;

const HRESULT_CANCELLED: i32 = 0x8007_04C7_u32 as i32;

fn picker(
    backend: impl Fn(Option<isize>) -> Result<Option<PathBuf>, BackendError> + Send + Sync + 'static,
) -> WindowsFolderPicker {
    WindowsFolderPicker::with_backend(Some(42), backend)
}

#[test]
fn pick_should_return_the_chosen_folder() {
    let result = picker(|_| Ok(Some(PathBuf::from(r"C:\Games")))).pick();
    assert_eq!(result, Ok(Some(PathBuf::from(r"C:\Games"))));
    let other = picker(|_| Ok(Some(PathBuf::from(r"C:\Users\ana\Videos")))).pick();
    assert_eq!(other, Ok(Some(PathBuf::from(r"C:\Users\ana\Videos"))));
}

#[test]
fn pick_should_report_a_cancelled_dialog_as_none() {
    assert_eq!(picker(|_| Ok(None)).pick(), Ok(None));
    assert_eq!(
        picker(|_| Err(BackendError::Hresult(HRESULT_CANCELLED))).pick(),
        Ok(None)
    );
}

#[test]
fn pick_should_map_other_hresults_to_a_code_without_system_text() {
    let error = picker(|_| Err(BackendError::Hresult(0x8000_FFFF_u32 as i32)))
        .pick()
        .unwrap_err();
    assert_eq!(error, PortError::Unsupported("io_0x8000ffff".into()));
    let denied = picker(|_| Err(BackendError::Hresult(0x8007_0005_u32 as i32)))
        .pick()
        .unwrap_err();
    assert_eq!(denied, PortError::Unsupported("io_0x80070005".into()));
}

#[test]
fn pick_should_fail_when_com_cannot_be_initialised() {
    let error = picker(|_| Err(BackendError::Init)).pick().unwrap_err();
    assert_eq!(error, PortError::Unsupported("com_init".into()));
}

#[test]
fn pick_should_turn_a_panic_into_a_port_error() {
    let error = picker(|_| panic!("el dialogo exploto")).pick().unwrap_err();
    assert_eq!(error, PortError::Unsupported("picker_unavailable".into()));
    // Despues de un panico el selector sigue usable.
    let again = WindowsFolderPicker::with_backend(None, |_| Ok(None));
    assert_eq!(again.pick(), Ok(None));
}

#[test]
fn pick_should_run_the_dialog_on_its_own_thread_with_the_owner_window() {
    let seen: Arc<Mutex<Seen>> = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    let selector = WindowsFolderPicker::with_backend(Some(1234), move |owner| {
        let name = std::thread::current()
            .name()
            .unwrap_or_default()
            .to_string();
        if let Ok(mut seen) = sink.lock() {
            seen.push((owner, name));
        }
        Ok(None)
    });
    selector.pick().unwrap();
    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen, vec![(Some(1234), "pcopt-folder-picker".to_string())]);
    assert_ne!(
        std::thread::current().name(),
        Some("pcopt-folder-picker"),
        "no corre en el hilo del llamador"
    );
}

#[test]
fn hresult_codes_should_be_stable_lowercase_hex() {
    assert_eq!(code_detail(0x8000_FFFF_u32 as i32), "io_0x8000ffff");
    assert_eq!(code_detail(1), "io_0x00000001");
    assert_eq!(code_detail(-1), "io_0xffffffff");
}

/// Abre el dialogo REAL: solo en el smoke guiado (`cargo test -- --ignored real_dialog`).
#[test]
#[ignore = "abre UI nativa; se prueba en el smoke"]
fn real_dialog_should_open_and_accept_a_cancel() {
    let result = WindowsFolderPicker::new(None).pick();
    assert!(result.is_ok(), "{result:?}");
}

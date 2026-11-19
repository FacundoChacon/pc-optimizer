//! Tests del vaciador real. NINGUNO llama al shell: usan un backend inyectado. El unico test que
//! vacia la Papelera REAL esta marcado `#[ignore]` y solo se corre en el smoke guiado (D-16).

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use super::*;
use crate::domain::ports::{RecycleBinInfo, RecycleBinUsage};

fn c_root() -> VolumeRoot {
    VolumeRoot::from_drive_letter('C').unwrap()
}

// --- empty_error_from_hresult ---

#[test]
fn empty_error_from_hresult_should_map_access_denied_to_permission_denied() {
    assert_eq!(
        empty_error_from_hresult(0x8007_0005_u32 as i32),
        EmptyError::PermissionDenied
    );
}

#[test]
fn empty_error_from_hresult_should_reduce_other_codes_to_a_hex_code_without_system_text() {
    let cases = [
        (0x8000_FFFF_u32, "io_0x8000ffff"),
        (0x8007_0020_u32, "io_0x80070020"),
        (0x0000_0001_u32, "io_0x00000001"),
        (0xFFFF_FFFF_u32, "io_0xffffffff"),
    ];
    for (code, expected) in cases {
        assert_eq!(
            empty_error_from_hresult(code as i32),
            EmptyError::Other(expected.to_owned()),
            "{code:#x}"
        );
    }
}

// --- hilo propio y mapeo del backend ---

#[test]
fn empty_should_run_the_backend_on_its_own_named_thread() {
    let seen = Arc::new(Mutex::new(None));
    let recorder = Arc::clone(&seen);
    let emptier = WindowsRecycleBinEmptier::with_backend(move |_root| {
        let current = thread::current();
        if let Ok(mut slot) = recorder.lock() {
            *slot = Some((current.id(), current.name().map(str::to_owned)));
        }
        Ok(())
    });
    assert_eq!(emptier.empty(&c_root()), Ok(()));
    let seen = seen.lock().unwrap().clone().unwrap();
    assert_ne!(seen.0, thread::current().id());
    assert_eq!(seen.1.as_deref(), Some("pcopt-empty-bin"));
}

#[test]
fn empty_should_hand_the_exact_root_to_the_backend() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let recorder = Arc::clone(&seen);
    let emptier = WindowsRecycleBinEmptier::with_backend(move |root| {
        if let Ok(mut roots) = recorder.lock() {
            roots.push(root.as_path());
        }
        Ok(())
    });
    emptier
        .empty(&VolumeRoot::from_drive_letter('d').unwrap())
        .unwrap();
    assert_eq!(
        *seen.lock().unwrap(),
        vec![std::path::PathBuf::from("D:\\")]
    );
}

#[test]
fn empty_should_translate_a_panicking_backend_to_backend_unavailable_and_keep_working() {
    let calls = Arc::new(AtomicU32::new(0));
    let counter = Arc::clone(&calls);
    let emptier = WindowsRecycleBinEmptier::with_backend(move |_root| {
        if counter.fetch_add(1, Ordering::SeqCst) == 0 {
            panic!("backend exploded");
        }
        Ok(())
    });
    assert_eq!(
        emptier.empty(&c_root()),
        Err(EmptyError::BackendUnavailable)
    );
    // El proceso sigue vivo y un segundo intento corre.
    assert_eq!(emptier.empty(&c_root()), Ok(()));
}

#[test]
fn empty_should_map_backend_failures_without_leaking_system_text() {
    let denied = WindowsRecycleBinEmptier::with_backend(|_| {
        Err(BackendError::Hresult(0x8007_0005_u32 as i32))
    });
    assert_eq!(denied.empty(&c_root()), Err(EmptyError::PermissionDenied));

    let other = WindowsRecycleBinEmptier::with_backend(|_| {
        Err(BackendError::Hresult(0x8000_FFFF_u32 as i32))
    });
    assert_eq!(
        other.empty(&c_root()),
        Err(EmptyError::Other("io_0x8000ffff".to_owned()))
    );

    let init = WindowsRecycleBinEmptier::with_backend(|_| Err(BackendError::Init));
    assert_eq!(init.empty(&c_root()), Err(EmptyError::BackendUnavailable));
}

#[test]
fn flags_should_silence_every_shell_confirmation_progress_and_sound() {
    assert_eq!(SHELL_FLAGS, 1 | 2 | 4);
}

// --- test REAL: vacia la Papelera de C: del usuario ---

#[test]
#[ignore = "VACÍA la Papelera REAL de C: del usuario; correr solo en el smoke guiado con el dueño (D-16)"]
fn real_empty_should_leave_the_system_recycle_bin_empty() {
    let root = c_root();
    let bin = crate::adapters::windows::WindowsRecycleBin;
    let before = bin.query(&root.as_path()).unwrap();
    println!("antes: {before:?}");
    let result = WindowsRecycleBinEmptier::new().empty(&root);
    println!("llamada: {result:?}");
    let after = bin.query(&root.as_path()).unwrap();
    assert_eq!(after, RecycleBinUsage { items: 0, bytes: 0 });
}

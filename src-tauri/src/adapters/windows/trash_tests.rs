//! Tests de `WindowsTrash`.
//!
//! Los tests sin `#[ignore]` NO tocan la Papelera: inyectan el backend. Los `#[ignore]` mueven a
//! la Papelera real SOLO archivos `pcopt-test-*` creados por el propio test dentro de un
//! `TempDir`; se corren a mano con `--ignored`.

use std::fs::{self, OpenOptions};
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use tempfile::TempDir;

use super::*;
use crate::adapters::windows::WindowsFileSystem;
use crate::adapters::windows::fs::tests::make_junction;
use crate::domain::ports::FileSystem;
use crate::domain::protection::context::{ProtectionContext, RawProtectionContext};
use crate::domain::protection::service::PathProtection;

const FILE_SHARE_READ_WRITE: u32 = 0x1 | 0x2;

fn os_error(code: u32) -> trash::Error {
    trash::Error::Os {
        code: i32::from_ne_bytes(code.to_ne_bytes()),
        description: "texto del sistema que no debe filtrarse".into(),
    }
}

// ---------------------------------------------------------------- siempre activos

#[test]
fn move_path_should_translate_a_backend_panic_to_backend_unavailable_and_keep_answering() {
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&calls);
    let trash = WindowsTrash::with_backend(move |_path: &Path| {
        if seen.fetch_add(1, Ordering::SeqCst) == 0 {
            panic!("CoInitializeEx simulado fallido");
        }
        Ok(())
    })
    .unwrap();

    assert_eq!(
        trash.move_path(Path::new("C:\\x\\a.txt")),
        Err(TrashError::BackendUnavailable)
    );
    // El hilo sobrevivio al panico y sigue respondiendo.
    assert_eq!(trash.move_path(Path::new("C:\\x\\b.txt")), Ok(()));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn move_path_should_map_backend_errors_without_leaking_the_system_text() {
    let trash =
        WindowsTrash::with_backend(
            |path: &Path| match path.file_name().and_then(|n| n.to_str()) {
                Some("busy.txt") => Err(os_error(0x8027_0027)),
                Some("bin.txt") => Err(os_error(0x8027_0036)),
                _ => Err(os_error(0x8027_0099)),
            },
        )
        .unwrap();

    assert_eq!(
        trash.move_path(Path::new("busy.txt")),
        Err(TrashError::InUse)
    );
    assert_eq!(
        trash.move_path(Path::new("bin.txt")),
        Err(TrashError::BinUnavailable)
    );
    assert_eq!(
        trash.move_path(Path::new("other.txt")),
        Err(TrashError::Other("io_0x80270099".into()))
    );
}

#[test]
fn move_path_should_pass_each_path_to_the_backend_on_a_dedicated_thread() {
    let main_thread = std::thread::current().id();
    let (tx, rx) = std::sync::mpsc::channel();
    let trash = WindowsTrash::with_backend(move |path: &Path| {
        let _ = tx.send((path.to_path_buf(), std::thread::current().id()));
        Ok(())
    })
    .unwrap();

    trash.move_path(Path::new("one.txt")).unwrap();
    trash.move_path(Path::new("two.txt")).unwrap();

    let (first, first_thread) = rx.recv().unwrap();
    let (second, second_thread) = rx.recv().unwrap();
    assert_eq!(first, PathBuf::from("one.txt"));
    assert_eq!(second, PathBuf::from("two.txt"));
    assert_ne!(first_thread, main_thread);
    assert_eq!(first_thread, second_thread);
}

#[test]
fn drop_should_finish_the_dedicated_thread() {
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    // `tx` vive dentro del cierre: se suelta cuando el hilo termina y suelta el backend.
    let trash = WindowsTrash::with_backend(move |_path: &Path| {
        let _keep_alive = &tx;
        Ok(())
    })
    .unwrap();
    trash.move_path(Path::new("x")).unwrap();
    drop(trash);
    // Con el hilo unido el cierre ya se destruyo: el canal esta desconectado.
    assert!(matches!(
        rx.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Disconnected)
    ));
}

// ---------------------------------------------------------------- politica (D-12 B)

const POLICY_FILE: &str = r"C:\Users\ana\AppData\Local\Temp\pcopt-policy.tmp";
const POLICY_MB: u64 = 1024 * 1024;

/// `DeleteTarget` de un archivo falso de `size` bytes (sin tocar el disco real).
fn fake_target(size: u64) -> crate::domain::protection::service::DeleteTarget {
    use crate::domain::ports::fakes::FakeFileSystem;
    use crate::domain::protection::decision::ProtectionDecision;
    use crate::domain::protection::test_support::ctx_c_ana;

    let old = crate::domain::model::EpochMillis(1_000);
    let fs = FakeFileSystem::new()
        .canonical_as_is()
        .with_dir(r"C:\Users")
        .with_dir(r"C:\Users\ana")
        .with_dir(r"C:\Users\ana\AppData")
        .with_dir(r"C:\Users\ana\AppData\Local")
        .with_dir(r"C:\Users\ana\AppData\Local\Temp")
        .with_file(POLICY_FILE, size, Some(old));
    let service = PathProtection::new(&fs, Ok(ctx_c_ana()));
    let checked = match service.check(Path::new(POLICY_FILE)) {
        ProtectionDecision::Allowed(checked) => checked,
        other => panic!("deberia ser elegible: {other:?}"),
    };
    match service.prepare_delete(&checked) {
        Ok(target) => target,
        Err(decision) => panic!("prepare_delete rechazo: {decision:?}"),
    }
}

/// Papelera con backend contador y la politica dada; devuelve tambien el contador del backend.
fn policy_trash(
    policy: crate::domain::ports::fakes::FakeRecycleBinPolicy,
) -> (
    WindowsTrash,
    Arc<AtomicUsize>,
    Arc<crate::domain::ports::fakes::FakeRecycleBinPolicy>,
) {
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&calls);
    let policy = Arc::new(policy);
    let trash = WindowsTrash::with_backend_and_policy(
        move |_path: &Path| {
            seen.fetch_add(1, Ordering::SeqCst);
            Ok(())
        },
        Arc::clone(&policy),
    )
    .unwrap();
    (trash, calls, policy)
}

fn enabled(limit_mb: Option<u64>) -> crate::domain::ports::BinAvailability {
    crate::domain::ports::BinAvailability::Enabled {
        max_capacity_bytes: limit_mb.map(|mb| mb * POLICY_MB),
    }
}

#[test]
fn move_to_trash_should_never_call_the_backend_when_the_policy_says_disabled() {
    use crate::domain::ports::fakes::FakeRecycleBinPolicy;
    use crate::domain::ports::{BinAvailability, DisabledReason};
    let (trash, calls, policy) = policy_trash(FakeRecycleBinPolicy::with(
        BinAvailability::Disabled(DisabledReason::NukeOnDeleteVolume),
    ));

    assert_eq!(
        trash.move_to_trash(&fake_target(10)),
        Err(TrashError::BinUnavailable)
    );

    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(policy.queried(), vec![PathBuf::from(POLICY_FILE)]);
}

#[test]
fn move_to_trash_should_never_call_the_backend_when_the_file_exceeds_the_capacity() {
    use crate::domain::ports::fakes::FakeRecycleBinPolicy;
    let (trash, calls, _policy) = policy_trash(FakeRecycleBinPolicy::with(enabled(Some(1))));

    assert_eq!(
        trash.move_to_trash(&fake_target(POLICY_MB + 1)),
        Err(TrashError::TooLarge)
    );

    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn move_to_trash_should_never_call_the_backend_when_the_policy_cannot_be_read() {
    use crate::domain::ports::PortError;
    use crate::domain::ports::fakes::FakeRecycleBinPolicy;
    let (trash, calls, _policy) = policy_trash(FakeRecycleBinPolicy::failing(PortError::Io(
        "registry read failed".into(),
    )));

    assert_eq!(
        trash.move_to_trash(&fake_target(10)),
        Err(TrashError::BinUnavailable)
    );

    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn move_to_trash_should_call_the_backend_once_when_the_bin_is_enabled_and_the_file_fits() {
    use crate::domain::ports::fakes::FakeRecycleBinPolicy;
    for limit in [Some(1), None] {
        let (trash, calls, _policy) = policy_trash(FakeRecycleBinPolicy::with(enabled(limit)));

        assert_eq!(trash.move_to_trash(&fake_target(POLICY_MB)), Ok(()));

        assert_eq!(calls.load(Ordering::SeqCst), 1, "limite {limit:?}");
    }
}

#[test]
fn preflight_should_refuse_when_disabled_or_unreadable_and_accept_when_enabled_without_the_backend()
{
    use crate::domain::ports::fakes::FakeRecycleBinPolicy;
    use crate::domain::ports::{BinAvailability, DisabledReason, PortError};
    let sample = Path::new(POLICY_FILE);

    let (disabled, calls, _p) = policy_trash(FakeRecycleBinPolicy::with(
        BinAvailability::Disabled(DisabledReason::PolicyUser),
    ));
    assert_eq!(disabled.preflight(sample), Err(TrashError::BinUnavailable));
    let (unknown, unknown_calls, _p) =
        policy_trash(FakeRecycleBinPolicy::failing(PortError::Io("x".into())));
    assert_eq!(unknown.preflight(sample), Err(TrashError::BinUnavailable));
    let (ok, ok_calls, _p) = policy_trash(FakeRecycleBinPolicy::with(enabled(Some(1))));
    assert_eq!(ok.preflight(sample), Ok(()));

    assert_eq!(
        calls.load(Ordering::SeqCst)
            + unknown_calls.load(Ordering::SeqCst)
            + ok_calls.load(Ordering::SeqCst),
        0
    );
}

// ---------------------------------------------------------------- ayudas para los reales

/// Contexto sin usuario con todas las anclas bajo una carpeta inexistente de la unidad del
/// `TempDir`: el `TempDir` queda fuera de toda raiz protegida (regla por defecto).
fn context_for(root: &Path) -> ProtectionContext {
    let drive = root.to_string_lossy().chars().next().unwrap_or('C');
    let base = format!(r"{drive}:\pcopt-fake-system");
    let raw = RawProtectionContext {
        windows_dir: Some(format!(r"{base}\Windows")),
        program_files: vec![format!(r"{base}\Program Files")],
        program_data: Some(format!(r"{base}\ProgramData")),
        users_root: Some(format!(r"{base}\Users")),
        user: None,
        app_dirs: vec![format!(r"{base}\app")],
    };
    ProtectionContext::from_raw(raw).unwrap()
}

fn temp_root() -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let root = WindowsFileSystem.canonicalize(dir.path()).unwrap();
    (dir, root)
}

fn protection(root: &Path) -> PathProtection<'static, WindowsFileSystem> {
    PathProtection::new(&WindowsFileSystem, Ok(context_for(root)))
}

/// `prepare_delete` real sobre `path` (debe quedar `Allowed`).
fn prepare(
    protection: &PathProtection<'_, WindowsFileSystem>,
    path: &Path,
) -> crate::domain::protection::service::DeleteTarget {
    let checked = match protection.check(path) {
        crate::domain::protection::decision::ProtectionDecision::Allowed(checked) => checked,
        other => panic!("{} deberia ser elegible: {other:?}", path.display()),
    };
    match protection.prepare_delete(&checked) {
        Ok(target) => target,
        Err(decision) => panic!("prepare_delete rechazo {}: {decision:?}", path.display()),
    }
}

// ---------------------------------------------------------------- reales (ignorados)

#[test]
#[ignore = "mueve archivos de prueba creados en un tempdir a la Papelera real; correr a mano con --ignored"]
fn real_trash_should_move_a_file_and_remove_it_from_its_path() {
    let (_dir, root) = temp_root();
    let file = root.join("pcopt-test-moved.txt");
    fs::write(&file, b"pcopt-test").unwrap();
    let protection = protection(&root);
    let target = prepare(&protection, &file);

    let trash = WindowsTrash::new().unwrap();
    assert_eq!(trash.move_to_trash(&target), Ok(()));

    assert!(!file.exists(), "el archivo debia salir de su ruta");
}

#[test]
#[ignore = "mueve archivos de prueba creados en un tempdir a la Papelera real; correr a mano con --ignored"]
fn real_trash_should_move_a_file_while_its_ancestors_are_pinned() {
    let (_dir, root) = temp_root();
    let nested = root.join("pcopt-test-a").join("pcopt-test-b");
    fs::create_dir_all(&nested).unwrap();
    let file = nested.join("pcopt-test-pinned.txt");
    fs::write(&file, b"pcopt-test").unwrap();
    let protection = protection(&root);
    let target = prepare(&protection, &file);

    // Con la fijacion viva el ancestro no se puede renombrar...
    assert!(fs::rename(&nested, root.join("pcopt-test-renamed")).is_err());
    // ...pero el shell si puede mover el archivo.
    let trash = WindowsTrash::new().unwrap();
    assert_eq!(trash.move_to_trash(&target), Ok(()));
    assert!(!file.exists());
    assert!(nested.exists());
}

#[test]
#[ignore = "mueve archivos de prueba creados en un tempdir a la Papelera real; correr a mano con --ignored"]
fn real_trash_should_report_in_use_and_leave_a_file_opened_without_share_delete() {
    let (_dir, root) = temp_root();
    let file = root.join("pcopt-test-inuse.txt");
    fs::write(&file, b"pcopt-test").unwrap();
    let protection = protection(&root);
    let target = prepare(&protection, &file);
    let _open = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ_WRITE)
        .open(&file)
        .unwrap();

    let trash = WindowsTrash::new().unwrap();
    assert_eq!(trash.move_to_trash(&target), Err(TrashError::InUse));

    assert!(file.exists(), "el archivo en uso debia quedar en su lugar");
}

#[test]
#[ignore = "mueve archivos de prueba creados en un tempdir a la Papelera real; correr a mano con --ignored"]
fn real_trash_should_move_the_junction_and_leave_its_destination_untouched() {
    let (_dir, root) = temp_root();
    let destination = root.join("pcopt-test-destination");
    fs::create_dir(&destination).unwrap();
    let inner = destination.join("pcopt-test-inner.txt");
    fs::write(&inner, b"pcopt-test").unwrap();
    let swapped = root.join("pcopt-test-swapped.txt");
    fs::write(&swapped, b"pcopt-test").unwrap();
    let protection = protection(&root);
    let target = prepare(&protection, &swapped);

    // Ventana residual: despues de la re-verificacion, el archivo se reemplaza por una junction.
    fs::remove_file(&swapped).unwrap();
    make_junction(&swapped, &destination);

    let trash = WindowsTrash::new().unwrap();
    let result = trash.move_to_trash(&target);

    assert_eq!(result, Ok(()));
    assert!(!swapped.exists(), "la junction debia salir de su ruta");
    assert!(
        destination.exists(),
        "el destino de la junction se conserva"
    );
    assert!(inner.exists(), "el contenido del destino queda intacto");
}

#[test]
#[ignore = "mueve archivos de prueba creados en un tempdir a la Papelera real; correr a mano con --ignored"]
fn real_trash_should_move_a_file_with_a_path_over_260_characters_or_report_path_too_long() {
    let (_dir, root) = temp_root();
    let mut deep = root.clone();
    while deep.to_string_lossy().len() < 280 {
        deep.push("pcopt-test-long-segment-0123456789");
    }
    fs::create_dir_all(&deep).unwrap();
    let file = deep.join("pcopt-test-long.txt");
    fs::write(&file, b"pcopt-test").unwrap();
    let protection = protection(&root);
    let target = prepare(&protection, &file);
    assert!(target.path().to_string_lossy().len() > 260);

    let trash = WindowsTrash::new().unwrap();
    let result = trash.move_to_trash(&target);

    match result {
        Ok(()) => assert!(!file.exists(), "Ok implica que el archivo salio de su ruta"),
        Err(ref error) => {
            assert_eq!(*error, TrashError::PathTooLong);
            assert!(file.exists(), "PathTooLong implica que sigue en su lugar");
        }
    }
    eprintln!("RESULTADO ruta larga: {result:?}");
}

#[test]
#[ignore = "requiere privilegio para crear symlinks (modo desarrollador o administrador)"]
fn real_trash_should_move_a_file_symlink_and_leave_its_destination_untouched() {
    let (_dir, root) = temp_root();
    let destination = root.join("pcopt-test-real.txt");
    fs::write(&destination, b"pcopt-test").unwrap();
    let link = root.join("pcopt-test-link.txt");
    std::os::windows::fs::symlink_file(&destination, &link)
        .unwrap_or_else(|error| panic!("sin privilegio para crear symlinks: {error}"));
    let protection = protection(&root);
    // `prepare_delete` rechaza un enlace: se arma el objetivo sobre un archivo comun y se
    // reemplaza en la ventana residual por el symlink.
    let swapped = root.join("pcopt-test-swapped-link.txt");
    fs::write(&swapped, b"pcopt-test").unwrap();
    let target = prepare(&protection, &swapped);
    fs::remove_file(&swapped).unwrap();
    std::os::windows::fs::symlink_file(&destination, &swapped).unwrap();

    let trash = WindowsTrash::new().unwrap();
    assert_eq!(trash.move_to_trash(&target), Ok(()));

    assert!(!swapped.exists());
    assert!(destination.exists(), "el destino del symlink queda intacto");
    assert!(link.exists());
}

/// Compuerta D-12 (la corre el orquestador con el dueño; NO correr sin deshabilitar la
/// Papelera de C:). Con la Papelera de `C:` deshabilitada ("No mover archivos a la Papelera")
/// el archivo DEBE seguir en su lugar y el resultado DEBE ser `BinUnavailable`, sin dialogos.
#[test]
#[ignore = "requiere la Papelera de C: deshabilitada por el dueño"]
fn real_trash_should_refuse_with_bin_unavailable_and_keep_the_file_when_the_recycle_bin_is_disabled()
 {
    let (_dir, root) = temp_root();
    let file = root.join("pcopt-test-nuke.txt");
    fs::write(&file, b"pcopt-test").unwrap();
    let protection = protection(&root);
    let target = prepare(&protection, &file);

    let trash = WindowsTrash::new().unwrap();
    let result = trash.move_to_trash(&target);
    let survived = file.exists();
    eprintln!("RESULTADO D-12: {result:?}, archivo sigue en su lugar: {survived}");

    assert!(
        survived,
        "D-12: el archivo desaparecio (borrado permanente?)"
    );
    assert_eq!(result, Err(TrashError::BinUnavailable));
}

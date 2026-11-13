//! Tests del adaptador real. Todo ocurre dentro de un `TempDir`; nada toca rutas del usuario.

use std::fs::{self, OpenOptions};
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;
use windows::Win32::Storage::FileSystem::GetShortPathNameW;
use windows::core::PCWSTR;

use super::*;
use crate::domain::model::EpochMillis;
use crate::domain::ports::fs_contract::Scenario;
use crate::domain::ports::{EntryKind, FileSystem, PortError};

const ERROR_PRIVILEGE_NOT_HELD: i32 = 1314;

/// `WindowsFileSystem` que mantiene vivo su `TempDir` (para usarlo como `Scenario::fs`).
struct TempFs {
    fs: WindowsFileSystem,
    _dir: TempDir,
}

impl FileSystem for TempFs {
    fn canonicalize(&self, path: &Path) -> Result<PathBuf, PortError> {
        self.fs.canonicalize(path)
    }
    fn metadata(&self, path: &Path) -> Result<FileMeta, PortError> {
        self.fs.metadata(path)
    }
    fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>, PortError> {
        self.fs.read_dir(path)
    }
    fn pin_ancestors(&self, path: &Path) -> Result<Box<dyn AncestorPin>, PortError> {
        self.fs.pin_ancestors(path)
    }
}

/// Crea una junction `link` -> `target` con `mklink /J` (no requiere privilegios).
fn make_junction(link: &Path, target: &Path) {
    let status = Command::new("cmd")
        .arg("/C")
        .arg("mklink")
        .arg("/J")
        .arg(link)
        .arg(target)
        .output()
        .unwrap();
    assert!(
        status.status.success(),
        "mklink /J fallo: {}",
        String::from_utf8_lossy(&status.stdout)
    );
}

fn temp_root() -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let root = WindowsFileSystem.canonicalize(dir.path()).unwrap();
    (dir, root)
}

fn real_scenario() -> Scenario<TempFs> {
    let (dir, root) = temp_root();
    let file = root.join("file.txt");
    fs::write(&file, b"hello").unwrap();
    let dir_path = root.join("dir");
    fs::create_dir(&dir_path).unwrap();
    fs::write(dir_path.join("inner.txt"), b"x").unwrap();
    let target = root.join("target");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("secret.txt"), b"s").unwrap();
    let junction = root.join("junction");
    make_junction(&junction, &target);
    let hard_a = root.join("hard_a.dat");
    fs::write(&hard_a, b"h").unwrap();
    let hard_b = root.join("hard_b.dat");
    fs::hard_link(&hard_a, &hard_b).unwrap();
    Scenario {
        fs: TempFs {
            fs: WindowsFileSystem,
            _dir: dir,
        },
        file,
        dir: dir_path.clone(),
        dir_child: dir_path.join("inner.txt"),
        target_child: target.join("secret.txt"),
        target,
        junction,
        hard_a,
        hard_b,
        missing: root.join("missing"),
        root,
    }
}

mod real_fs_contract {
    use super::*;
    crate::domain::ports::fs_contract::fs_contract_tests!(real_scenario());
}

// --- 9.2 metadata ---

#[test]
fn metadata_should_report_size_and_modification_time_of_a_file() {
    let (_dir, root) = temp_root();
    let path = root.join("a.bin");
    fs::write(&path, vec![0u8; 1234]).unwrap();
    let meta = WindowsFileSystem.metadata(&path).unwrap();
    assert_eq!(meta.size_bytes, 1234);
    let modified = meta.modified.unwrap();
    assert!(modified > EpochMillis(1_600_000_000_000));
    let empty = root.join("empty");
    fs::write(&empty, b"").unwrap();
    assert_eq!(WindowsFileSystem.metadata(&empty).unwrap().size_bytes, 0);
}

#[test]
fn metadata_should_work_on_a_file_opened_exclusively() {
    let (_dir, root) = temp_root();
    let path = root.join("locked.dat");
    fs::write(&path, b"abc").unwrap();
    let _exclusive = OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .open(&path)
        .unwrap();
    // Nadie mas puede abrirlo para leer, pero consultar atributos sigue siendo posible.
    assert!(fs::read(&path).is_err());
    let meta = WindowsFileSystem.metadata(&path).unwrap();
    assert_eq!(meta.size_bytes, 3);
    assert_eq!(meta.kind, EntryKind::File);
}

#[test]
fn metadata_should_count_a_hard_link_made_after_the_first_query() {
    let (_dir, root) = temp_root();
    let a = root.join("a.dat");
    fs::write(&a, b"1").unwrap();
    assert_eq!(WindowsFileSystem.metadata(&a).unwrap().hard_links, Some(1));
    fs::hard_link(&a, root.join("b.dat")).unwrap();
    assert_eq!(WindowsFileSystem.metadata(&a).unwrap().hard_links, Some(2));
}

#[test]
fn metadata_should_change_identity_when_the_file_is_recreated() {
    let (_dir, root) = temp_root();
    let path = root.join("a.tmp");
    fs::write(&path, b"1").unwrap();
    let before = WindowsFileSystem.metadata(&path).unwrap().identity;
    fs::remove_file(&path).unwrap();
    fs::write(&path, b"1").unwrap();
    let after = WindowsFileSystem.metadata(&path).unwrap().identity;
    assert!(before.is_some() && after.is_some());
    assert_ne!(before, after);
}

#[test]
fn metadata_should_keep_identity_when_the_file_is_renamed() {
    let (_dir, root) = temp_root();
    let a = root.join("a.tmp");
    let b = root.join("b.tmp");
    fs::write(&a, b"1").unwrap();
    let before = WindowsFileSystem.metadata(&a).unwrap().identity;
    fs::rename(&a, &b).unwrap();
    assert_eq!(WindowsFileSystem.metadata(&b).unwrap().identity, before);
}

#[test]
fn metadata_should_not_follow_a_junction_and_report_its_own_identity() {
    let (_dir, root) = temp_root();
    let target = root.join("t");
    fs::create_dir(&target).unwrap();
    let link = root.join("j");
    make_junction(&link, &target);
    let link_meta = WindowsFileSystem.metadata(&link).unwrap();
    assert_eq!(link_meta.kind, EntryKind::ReparsePoint);
    assert!(!link_meta.cloud_placeholder);
    assert_ne!(
        link_meta.identity,
        WindowsFileSystem.metadata(&target).unwrap().identity
    );
}

#[test]
fn metadata_should_reject_a_relative_path() {
    assert!(matches!(
        WindowsFileSystem.metadata(Path::new(r"relative\a.txt")),
        Err(PortError::Unsupported(_))
    ));
}

// --- funciones puras del adaptador ---

#[test]
fn classify_should_map_attributes_to_kind_and_cloud_flag() {
    const DIR: u32 = 0x10;
    const REPARSE: u32 = 0x400;
    const RECALL_ON_DATA_ACCESS: u32 = 0x40_0000;
    const RECALL_ON_OPEN: u32 = 0x4_0000;
    const OFFLINE: u32 = 0x1000;
    assert_eq!(classify(0x20, None), (EntryKind::File, false));
    assert_eq!(classify(DIR, None), (EntryKind::Directory, false));
    assert_eq!(
        classify(RECALL_ON_DATA_ACCESS, None),
        (EntryKind::File, true)
    );
    assert_eq!(
        classify(DIR | RECALL_ON_OPEN, None),
        (EntryKind::Directory, true)
    );
    assert_eq!(classify(OFFLINE, None), (EntryKind::File, true));
    // Symlink / junction: reparse sin nube.
    assert_eq!(
        classify(REPARSE, Some(0xA000_000C)),
        (EntryKind::ReparsePoint, false)
    );
    assert_eq!(
        classify(DIR | REPARSE, Some(0xA000_0003)),
        (EntryKind::ReparsePoint, false)
    );
    // Reparse de proveedor de nube, aunque este hidratado (sin atributos de recall).
    assert_eq!(
        classify(REPARSE, Some(0x9000_001A)),
        (EntryKind::ReparsePoint, true)
    );
    assert_eq!(
        classify(REPARSE, Some(0x9000_901A)),
        (EntryKind::ReparsePoint, true)
    );
    // Un reparse del que no se pudo leer el tag sigue siendo reparse.
    assert_eq!(classify(REPARSE, None), (EntryKind::ReparsePoint, false));
}

#[test]
fn filetime_should_convert_to_epoch_millis() {
    assert_eq!(filetime_to_millis(0), None);
    assert_eq!(filetime_to_millis(-5), None);
    assert_eq!(
        filetime_to_millis(116_444_736_000_000_000),
        Some(EpochMillis(0))
    );
    assert_eq!(
        filetime_to_millis(116_444_736_000_000_000 + 10_000 * 1500),
        Some(EpochMillis(1500))
    );
    // Anterior a 1970: no representable de forma util => None (fail-safe, solo informativo).
    assert_eq!(filetime_to_millis(116_444_735_000_000_000), None);
}

#[test]
fn map_io_error_should_translate_win32_codes() {
    let p = Path::new(r"C:\x");
    let map = |code: i32| map_io_error(p, &io::Error::from_raw_os_error(code));
    assert!(matches!(map(2), PortError::NotFound(_)));
    assert!(matches!(map(3), PortError::NotFound(_)));
    assert!(matches!(map(5), PortError::PermissionDenied(_)));
    assert!(matches!(map(32), PortError::InUse(_)));
    assert!(matches!(map(33), PortError::InUse(_)));
    assert!(matches!(map(1), PortError::Unsupported(_)));
    assert!(matches!(map(50), PortError::Unsupported(_)));
    assert!(matches!(map(87), PortError::Unsupported(_)));
    assert!(matches!(map(1234), PortError::Io(_)));
    let no_code = io::Error::other("sin codigo");
    assert!(matches!(map_io_error(p, &no_code), PortError::Io(_)));
    // El detalle incluye la ruta y el codigo.
    let PortError::NotFound(detail) = map(2) else {
        unreachable!("2 es NotFound")
    };
    assert!(detail.contains(r"C:\x") && detail.contains('2'));
}

#[test]
fn metadata_should_map_access_denied_to_permission_denied() {
    let (_dir, root) = temp_root();
    let path = root.join("denied.txt");
    fs::write(&path, b"1").unwrap();
    // ACL denegada en el propio archivo del tempdir: niega leer atributos a Everyone.
    let output = Command::new("icacls")
        .arg(&path)
        .args(["/deny", "*S-1-1-0:(RA)"])
        .output()
        .unwrap();
    if !output.status.success() {
        eprintln!("icacls no pudo crear la ACL: caso omitido");
        return;
    }
    let result = WindowsFileSystem.metadata(&path);
    // Restaura el acceso para que el TempDir se pueda borrar.
    let _ = Command::new("icacls")
        .arg(&path)
        .args(["/remove:d", "*S-1-1-0"])
        .output();
    match result {
        Err(error) => assert!(matches!(error, PortError::PermissionDenied(_)), "{error:?}"),
        // `FILE_FLAG_BACKUP_SEMANTICS` (necesario para abrir directorios) hace que un token con
        // `SeBackupPrivilege` (administrador elevado, como la app) ignore la ACL: no hay
        // denegacion que mapear en esa sesion. El mapeo 5 -> PermissionDenied queda cubierto
        // por `map_io_error_should_translate_win32_codes`.
        Ok(_) => eprintln!("el token ignora la ACL (SeBackupPrivilege): caso omitido"),
    }
}

// --- 9.3 canonicalize ---

#[test]
fn canonicalize_should_resolve_dot_dot_segments() {
    let (_dir, root) = temp_root();
    fs::create_dir(root.join("sub")).unwrap();
    fs::write(root.join("f.txt"), b"1").unwrap();
    let through = root.join("sub").join("..").join("f.txt");
    assert_eq!(
        WindowsFileSystem.canonicalize(&through).unwrap(),
        root.join("f.txt")
    );
}

#[test]
fn canonicalize_should_return_the_on_disk_casing() {
    let (_dir, root) = temp_root();
    fs::write(root.join("Mixed.TXT"), b"1").unwrap();
    let upper = root.join("MIXED.txt");
    assert_eq!(
        WindowsFileSystem.canonicalize(&upper).unwrap(),
        root.join("Mixed.TXT")
    );
}

#[test]
fn canonicalize_should_expand_a_short_name_to_the_long_one() {
    let (_dir, root) = temp_root();
    let long_dir = root.join("Carpeta Con Nombre Largo");
    fs::create_dir(&long_dir).unwrap();
    fs::write(long_dir.join("archivo largo.txt"), b"1").unwrap();
    let short_dir = short_path(&long_dir);
    if short_dir.file_name() == long_dir.file_name() {
        eprintln!("el volumen no genera nombres 8.3: caso omitido");
        return;
    }
    let via_short = short_dir.join("archivo largo.txt");
    assert_eq!(
        WindowsFileSystem.canonicalize(&via_short).unwrap(),
        long_dir.join("archivo largo.txt")
    );
    // Tambien el ultimo componente.
    let short_file = short_path(&long_dir.join("archivo largo.txt"));
    assert_eq!(
        WindowsFileSystem.canonicalize(&short_file).unwrap(),
        long_dir.join("archivo largo.txt")
    );
}

fn short_path(path: &Path) -> PathBuf {
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut buf = vec![0u16; 1024];
    // SAFETY: `wide` esta terminado en nulo y vive durante la llamada; `buf` es un buffer
    // escribible de 1024 `u16` cuya longitud recibe la API como slice.
    let len = unsafe { GetShortPathNameW(PCWSTR(wide.as_ptr()), Some(&mut buf)) } as usize;
    assert!(len > 0 && len < buf.len(), "GetShortPathNameW fallo");
    PathBuf::from(String::from_utf16(&buf[..len]).unwrap())
}

#[test]
fn canonicalize_should_reject_a_relative_path() {
    assert!(matches!(
        WindowsFileSystem.canonicalize(Path::new("a.txt")),
        Err(PortError::Unsupported(_))
    ));
}

#[test]
fn canonicalize_should_reject_a_path_whose_real_form_has_no_drive_letter() {
    assert!(matches!(
        drive_path(Path::new(r"\\?\UNC\server\share\a.txt")),
        Err(PortError::Unsupported(_))
    ));
    assert!(matches!(
        drive_path(Path::new(r"\\?\Volume{1234}\a.txt")),
        Err(PortError::Unsupported(_))
    ));
    assert_eq!(
        drive_path(Path::new(r"\\?\C:\a\b.txt")).unwrap(),
        PathBuf::from(r"C:\a\b.txt")
    );
}

// --- 9.4 read_dir ---

#[test]
fn read_dir_should_report_kinds_sizes_and_no_identity() {
    let (_dir, root) = temp_root();
    fs::write(root.join("a.txt"), b"abcd").unwrap();
    fs::create_dir(root.join("sub")).unwrap();
    let mut entries = WindowsFileSystem.read_dir(&root).unwrap();
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].path, root.join("a.txt"));
    assert_eq!(entries[0].meta.kind, EntryKind::File);
    assert_eq!(entries[0].meta.size_bytes, 4);
    assert_eq!(entries[1].meta.kind, EntryKind::Directory);
    assert!(entries.iter().all(|e| e.meta.identity.is_none()));
    assert!(entries.iter().all(|e| e.meta.hard_links.is_none()));
}

#[test]
fn read_dir_should_list_nothing_for_an_empty_directory() {
    let (_dir, root) = temp_root();
    assert!(WindowsFileSystem.read_dir(&root).unwrap().is_empty());
}

#[test]
fn read_dir_should_fail_for_a_file_and_for_a_missing_path() {
    let (_dir, root) = temp_root();
    fs::write(root.join("f"), b"1").unwrap();
    assert!(WindowsFileSystem.read_dir(&root.join("f")).is_err());
    assert!(matches!(
        WindowsFileSystem.read_dir(&root.join("nope")),
        Err(PortError::NotFound(_))
    ));
}

#[test]
fn read_dir_should_report_a_junction_as_reparse_point_without_listing_its_target() {
    let (_dir, root) = temp_root();
    let target = root.join("t");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("x"), b"1").unwrap();
    let link = root.join("j");
    make_junction(&link, &target);
    let entries = WindowsFileSystem.read_dir(&root).unwrap();
    let junction = entries.iter().find(|e| e.path == link).unwrap();
    assert_eq!(junction.meta.kind, EntryKind::ReparsePoint);
    assert!(WindowsFileSystem.read_dir(&link).unwrap().is_empty());
}

/// Requiere `SeCreateSymbolicLinkPrivilege` (modo desarrollador o administrador): con un
/// token sin ese privilegio crear el symlink falla con `ERROR_PRIVILEGE_NOT_HELD` (1314).
/// Se corre a mano: `cargo test -- --ignored symlink`.
#[test]
#[ignore = "requiere privilegio para crear symlinks (modo desarrollador o administrador)"]
fn symlink_to_a_file_should_be_reported_as_reparse_point() {
    let (_dir, root) = temp_root();
    let file = root.join("real.txt");
    fs::write(&file, b"1").unwrap();
    let link = root.join("link.txt");
    match std::os::windows::fs::symlink_file(&file, &link) {
        Ok(()) => {}
        Err(error) if error.raw_os_error() == Some(ERROR_PRIVILEGE_NOT_HELD) => {
            panic!("sin privilegio para crear symlinks: {error}")
        }
        Err(error) => panic!("no se pudo crear el symlink: {error}"),
    }
    assert_eq!(
        WindowsFileSystem.metadata(&link).unwrap().kind,
        EntryKind::ReparsePoint
    );
    let entries = WindowsFileSystem.read_dir(&root).unwrap();
    let entry = entries.iter().find(|e| e.path == link).unwrap();
    assert_eq!(entry.meta.kind, EntryKind::ReparsePoint);
    assert_eq!(WindowsFileSystem.canonicalize(&link).unwrap(), link);
}

// --- 9.5 pin_ancestors ---

fn nested_file(root: &Path) -> PathBuf {
    fs::create_dir_all(root.join("a").join("b")).unwrap();
    let file = root.join("a").join("b").join("f.txt");
    fs::write(&file, b"1").unwrap();
    file
}

#[test]
fn pin_ancestors_should_block_renaming_an_ancestor_until_dropped() {
    let (_dir, root) = temp_root();
    let file = nested_file(&root);
    let pin = WindowsFileSystem.pin_ancestors(&file).unwrap();
    assert!(fs::rename(root.join("a"), root.join("z")).is_err());
    assert!(fs::rename(root.join("a").join("b"), root.join("a").join("c")).is_err());
    assert!(fs::remove_dir_all(root.join("a")).is_err());
    drop(pin);
    fs::rename(root.join("a"), root.join("z")).unwrap();
}

#[test]
fn pin_ancestors_should_still_allow_moving_the_target_itself() {
    let (_dir, root) = temp_root();
    let file = nested_file(&root);
    let _pin = WindowsFileSystem.pin_ancestors(&file).unwrap();
    fs::rename(&file, file.with_file_name("g.txt")).unwrap();
    assert!(file.with_file_name("g.txt").exists());
}

#[test]
fn pin_ancestors_should_fail_on_a_junction_ancestor_leaving_nothing_pinned() {
    let (_dir, root) = temp_root();
    let real = root.join("real");
    fs::create_dir_all(real.join("inner")).unwrap();
    fs::write(real.join("inner").join("f.txt"), b"1").unwrap();
    let outer = root.join("outer");
    fs::create_dir(&outer).unwrap();
    let link = outer.join("link");
    make_junction(&link, &real);
    let through = link.join("inner").join("f.txt");
    assert!(WindowsFileSystem.pin_ancestors(&through).is_err());
    // Nada quedo fijado: el ancestro `outer` (anterior a la junction) se puede renombrar.
    fs::rename(&outer, root.join("outer2")).unwrap();
}

#[test]
fn pin_ancestors_should_fail_for_missing_ancestors_files_and_relative_paths() {
    let (_dir, root) = temp_root();
    let file = nested_file(&root);
    assert!(matches!(
        WindowsFileSystem.pin_ancestors(&root.join("nope").join("f.txt")),
        Err(PortError::NotFound(_))
    ));
    // Un archivo como ancestro no es directorio.
    assert!(WindowsFileSystem.pin_ancestors(&file.join("x")).is_err());
    assert!(matches!(
        WindowsFileSystem.pin_ancestors(Path::new(r"a\b.txt")),
        Err(PortError::Unsupported(_))
    ));
}

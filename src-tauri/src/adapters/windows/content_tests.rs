//! Tests del adaptador real de contenido. Todo ocurre dentro de un `TempDir`; nada toca rutas del
//! usuario. Los que dependen de privilegios o de la nube real van `#[ignore]`.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::windows::fs::OpenOptionsExt;
use std::path::PathBuf;

use tempfile::TempDir;

use super::*;
use crate::adapters::windows::fs::WindowsFileSystem;
use crate::adapters::windows::fs::tests::make_junction;
use crate::domain::ports::content_contract::{self as contract, SAMPLE_LEN, Scenario};
use crate::domain::ports::{EntryKind, FileSystem};

fn temp_root() -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let root = WindowsFileSystem.canonicalize(dir.path()).unwrap();
    (dir, root)
}

fn sample_file(root: &Path, name: &str) -> PathBuf {
    let path = root.join(name);
    fs::write(&path, contract::sample_bytes(SAMPLE_LEN)).unwrap();
    path
}

#[test]
fn windows_reader_should_satisfy_the_shared_content_contract() {
    let (_dir, root) = temp_root();
    let scenario = Scenario {
        reader: WindowsContentReader,
        file: sample_file(&root, "sample.bin"),
        missing: root.join("missing.bin"),
    };
    contract::run_all(&scenario);
}

#[test]
fn meta_should_match_the_metadata_of_the_file_system_adapter() {
    let (_dir, root) = temp_root();
    let path = sample_file(&root, "a.bin");
    let by_path = WindowsFileSystem.metadata(&path).unwrap();
    let by_handle = WindowsContentReader.open(&path).unwrap().meta().unwrap();
    assert_eq!(by_handle, by_path);
    assert_eq!(by_handle.kind, EntryKind::File);
    assert_eq!(by_handle.size_bytes, SAMPLE_LEN as u64);
    assert!(by_handle.identity.is_some());
    assert!(by_handle.modified.is_some());
}

#[test]
fn meta_should_reflect_a_change_made_while_the_file_is_open() {
    let (_dir, root) = temp_root();
    let path = sample_file(&root, "growing.bin");
    let handle = WindowsContentReader.open(&path).unwrap();
    let before = handle.meta().unwrap();
    let mut other = OpenOptions::new().append(true).open(&path).unwrap();
    other.write_all(&[7u8; 4096]).unwrap();
    other.flush().unwrap();
    drop(other);
    let after = handle.meta().unwrap();
    assert_eq!(after.size_bytes, before.size_bytes + 4096);
    assert_eq!(after.identity, before.identity);
}

#[test]
fn read_at_should_read_a_range_that_other_writers_have_not_touched() {
    let (_dir, root) = temp_root();
    let path = sample_file(&root, "a.bin");
    let mut handle = WindowsContentReader.open(&path).unwrap();
    let expected = contract::sample_bytes(SAMPLE_LEN);
    let mut buf = vec![0u8; 1_000];
    assert_eq!(handle.read_at(150_000, &mut buf).unwrap(), 1_000);
    assert_eq!(buf, expected[150_000..151_000]);
    // Lecturas fuera de orden: el offset es explicito, no depende de la posicion anterior.
    assert_eq!(handle.read_at(0, &mut buf).unwrap(), 1_000);
    assert_eq!(buf, expected[..1_000]);
}

#[test]
fn another_process_should_write_rename_and_delete_the_file_while_it_is_open() {
    let (_dir, root) = temp_root();
    let path = sample_file(&root, "busy.bin");
    let _handle = WindowsContentReader.open(&path).unwrap();
    // Escribir.
    let mut writer = OpenOptions::new().append(true).open(&path).unwrap();
    writer.write_all(b"x").unwrap();
    drop(writer);
    // Renombrar.
    let renamed = root.join("renamed.bin");
    fs::rename(&path, &renamed).unwrap();
    // Borrar (solo el archivo de este TempDir).
    fs::remove_file(&renamed).unwrap();
}

#[test]
fn open_should_report_in_use_when_another_handle_does_not_share_read() {
    let (_dir, root) = temp_root();
    let path = sample_file(&root, "locked.bin");
    let _exclusive = OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .open(&path)
        .unwrap();
    assert!(matches!(
        WindowsContentReader.open(&path),
        Err(PortError::InUse(_))
    ));
}

#[test]
fn open_should_report_not_found_and_reject_relative_paths() {
    let (_dir, root) = temp_root();
    assert!(matches!(
        WindowsContentReader.open(&root.join("nope.bin")),
        Err(PortError::NotFound(_))
    ));
    assert!(matches!(
        WindowsContentReader.open(Path::new("relative.bin")),
        Err(PortError::Unsupported(_))
    ));
}

#[test]
fn open_should_open_a_junction_as_the_link_itself_not_its_target() {
    let (_dir, root) = temp_root();
    let target = root.join("target");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("secret.bin"), b"secret").unwrap();
    let junction = root.join("junction");
    make_junction(&junction, &target);
    let link_meta = WindowsContentReader
        .open(&junction)
        .unwrap()
        .meta()
        .unwrap();
    assert_eq!(link_meta.kind, EntryKind::ReparsePoint);
    let target_meta = WindowsContentReader.open(&target).unwrap().meta().unwrap();
    assert_eq!(target_meta.kind, EntryKind::Directory);
    assert_ne!(link_meta.identity, target_meta.identity);
    // La lectura del enlace no devuelve el contenido del destino.
    let mut handle = WindowsContentReader.open(&junction).unwrap();
    let mut buf = [0u8; 6];
    assert!(handle.read_at(0, &mut buf).is_err());
}

#[test]
fn open_should_report_a_directory_as_a_directory_so_the_caller_can_reject_it() {
    let (_dir, root) = temp_root();
    let dir = root.join("folder");
    fs::create_dir(&dir).unwrap();
    let meta = WindowsContentReader.open(&dir).unwrap().meta().unwrap();
    assert_eq!(meta.kind, EntryKind::Directory);
}

#[test]
fn hard_links_should_share_the_identity_and_report_two_names() {
    let (_dir, root) = temp_root();
    let a = sample_file(&root, "a.bin");
    let b = root.join("b.bin");
    fs::hard_link(&a, &b).unwrap();
    let meta_a = WindowsContentReader.open(&a).unwrap().meta().unwrap();
    let meta_b = WindowsContentReader.open(&b).unwrap().meta().unwrap();
    assert_eq!(meta_a.identity, meta_b.identity);
    assert_eq!(meta_a.hard_links, Some(2));
    assert_eq!(meta_b.hard_links, Some(2));
    let other = sample_file(&root, "c.bin");
    let meta_c = WindowsContentReader.open(&other).unwrap().meta().unwrap();
    assert_ne!(meta_c.identity, meta_a.identity);
}

/// Se corre a mano: `cargo test -- --ignored symlink`.
#[test]
#[ignore = "requiere SeCreateSymbolicLinkPrivilege (modo desarrollador o administrador)"]
fn open_should_open_a_file_symlink_as_the_link_itself() {
    let (_dir, root) = temp_root();
    let target = sample_file(&root, "target.bin");
    let link = root.join("link.bin");
    std::os::windows::fs::symlink_file(&target, &link).unwrap();
    let link_meta = WindowsContentReader.open(&link).unwrap().meta().unwrap();
    assert_eq!(link_meta.kind, EntryKind::ReparsePoint);
    let target_meta = WindowsContentReader.open(&target).unwrap().meta().unwrap();
    assert_ne!(link_meta.identity, target_meta.identity);
}

/// Se corre a mano con `PCOPT_CLOUD_FILE=<ruta de un archivo solo en la nube>`: verifica que
/// `open` + `meta()` + una lectura corta no hidratan el archivo (no debe descargarlo).
#[test]
#[ignore = "requiere una carpeta de OneDrive con un archivo solo en la nube; no debe descargarlo"]
fn open_should_not_hydrate_a_cloud_only_file() {
    let Some(raw) = std::env::var_os("PCOPT_CLOUD_FILE") else {
        panic!("definir PCOPT_CLOUD_FILE con la ruta de un archivo que solo este en la nube");
    };
    let path = PathBuf::from(raw);
    let before = fs::metadata(&path).unwrap();
    let handle = WindowsContentReader.open(&path);
    // Con `FILE_FLAG_OPEN_NO_RECALL` la apertura puede fallar o abrir el marcador, pero nunca
    // descarga: si abre, el metadato lo informa como nube.
    if let Ok(handle) = handle {
        let meta = handle.meta().unwrap();
        assert!(
            meta.cloud_placeholder,
            "el archivo no parece estar solo en la nube"
        );
    }
    let after = fs::metadata(&path).unwrap();
    use std::os::windows::fs::MetadataExt;
    assert_eq!(before.file_attributes(), after.file_attributes());
}

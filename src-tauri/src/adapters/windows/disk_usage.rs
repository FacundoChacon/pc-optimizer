//! Analisis de disco REAL (C-07) pero solo sobre un `TempDir`: `scan_folders` con
//! `WindowsFileSystem` y `WindowsVolumeSpace`. Nada toca carpetas del usuario ni el disco entero.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;

use tempfile::TempDir;

use super::{WindowsFileSystem, WindowsVolumeSpace};
use crate::adapters::windows::fs::tests::make_junction;
use crate::domain::disk_usage::model::{Access, FolderScanResults, IssueReason, RootKind};
use crate::domain::disk_usage::scanner::{
    FolderScanObserver, FolderScanOutcome, RootChoice, scan_folders,
};
use crate::domain::ports::FileSystem;
use crate::domain::protection::context::ProtectionContext;
use crate::domain::protection::path::normalize_lexical;
use crate::domain::protection::test_support::raw_for;

const MIB: u64 = 1024 * 1024;

struct Silent;

impl FolderScanObserver for Silent {
    fn walking(&mut self, _files_seen: u64, _dirs_seen: u64, _bytes_seen: u64) {}
    fn saving(&mut self) {}
}

/// Contexto cuya unidad del sistema es la del `TempDir` y cuyo usuario es el dueno de la ruta
/// temporal (si cuelga de `\Users\<nombre>`), para que la raiz del test sea una carpeta propia.
fn context_for(root: &Path) -> ProtectionContext {
    let text = root.to_string_lossy().into_owned();
    let drive = text.chars().next().unwrap_or('C').to_ascii_uppercase();
    let user = text
        .split('\\')
        .skip_while(|part| !part.eq_ignore_ascii_case("Users"))
        .nth(1)
        .filter(|name| !name.is_empty())
        .unwrap_or("ana")
        .to_string();
    match ProtectionContext::from_raw(raw_for(drive, &user)) {
        Ok(ctx) => ctx,
        Err(error) => panic!("contexto de prueba invalido: {error}"),
    }
}

fn temp_root() -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let root = WindowsFileSystem.canonicalize(dir.path()).unwrap();
    (dir, root)
}

fn write(path: &Path, size: u64) {
    fs::write(path, vec![7u8; usize::try_from(size).unwrap()]).unwrap();
}

fn scan(root: &Path) -> FolderScanResults {
    let normalized = normalize_lexical(&root.to_string_lossy()).expect("temp path normalizes");
    match scan_folders(
        RootChoice::Picked(normalized),
        Ok(context_for(root)),
        &WindowsFileSystem,
        &WindowsVolumeSpace,
        &AtomicBool::new(false),
        &mut Silent,
    ) {
        Ok(FolderScanOutcome::Completed(results)) => results,
        other => panic!("se esperaba Completed: {other:?}"),
    }
}

fn find<'r>(
    results: &'r FolderScanResults,
    root: &Path,
    relative: &str,
) -> &'r crate::domain::disk_usage::model::StoredFolder {
    let wanted = if relative.is_empty() {
        root.to_string_lossy().into_owned()
    } else {
        root.join(relative).to_string_lossy().into_owned()
    };
    match results
        .folders
        .iter()
        .find(|f| f.path.eq_ignore_ascii_case(&wanted))
    {
        Some(folder) => folder,
        None => panic!(
            "no hay carpeta {wanted}: {:?}",
            results.folders.iter().map(|f| &f.path).collect::<Vec<_>>()
        ),
    }
}

/// Quita siempre la denegacion de listado (tambien si el test falla) para poder borrar el `TempDir`.
struct AclGuard(PathBuf);

impl Drop for AclGuard {
    fn drop(&mut self) {
        let _ = Command::new("icacls")
            .arg(&self.0)
            .args(["/remove:d", "*S-1-1-0"])
            .output();
    }
}

#[test]
fn real_scan_should_sum_a_known_tree_and_skip_the_junction() {
    let (_dir, root) = temp_root();
    fs::create_dir_all(root.join("a").join("b")).unwrap();
    write(&root.join("a").join("x.bin"), 4 * MIB);
    write(&root.join("a").join("b").join("y.bin"), 2 * MIB);
    write(&root.join("z.bin"), MIB);
    let outside = TempDir::new().unwrap();
    write(&outside.path().join("big.bin"), 8 * MIB);
    make_junction(&root.join("link-outside"), outside.path());

    let results = scan(&root);
    assert_eq!(results.root_kind, RootKind::Picked);
    assert_eq!(results.counters.total_bytes, 7 * MIB);
    assert_eq!(results.counters.file_count, 3);
    assert_eq!(results.counters.dir_count, 2);
    assert_eq!(results.counters.reparse_skipped, 1);
    let top = find(&results, &root, "");
    assert_eq!((top.size_bytes, top.direct_bytes), (7 * MIB, MIB));
    let a = find(&results, &root, "a");
    assert_eq!((a.size_bytes, a.direct_bytes), (6 * MIB, 4 * MIB));
    assert!(
        results
            .folders
            .iter()
            .all(|f| !f.path.to_ascii_lowercase().contains("link-outside")),
        "la junction no se recorre"
    );
    assert_eq!(results.large_files[0].size_bytes, 4 * MIB);
    assert!(results.volume.is_some_and(|v| v.total_bytes > 0));
    // El analisis no escribio nada en el arbol.
    assert_eq!(fs::read(root.join("z.bin")).unwrap().len() as u64, MIB);
}

#[test]
fn real_scan_should_mark_a_folder_denied_by_acl_as_inaccessible() {
    let (_dir, root) = temp_root();
    let locked = root.join("locked");
    fs::create_dir(&locked).unwrap();
    write(&locked.join("secret.bin"), MIB);
    write(&root.join("open.bin"), 2 * MIB);
    let _guard = AclGuard(locked.clone());
    let output = Command::new("icacls")
        .arg(&locked)
        .args(["/deny", "*S-1-1-0:(RD)"])
        .output()
        .unwrap();
    if !output.status.success() {
        eprintln!("icacls no pudo crear la ACL: caso omitido");
        return;
    }
    let results = scan(&root);
    if results.counters.inaccessible_dirs == 0 {
        // Un token con `SeBackupPrivilege` ignora la ACL: el listado funciono y no hay
        // denegacion que informar.
        eprintln!("el token ignora la ACL: caso omitido");
        return;
    }
    let denied = find(&results, &root, "locked");
    assert_eq!(denied.access, Access::Inaccessible);
    assert_eq!(denied.size_bytes, 0);
    assert_eq!(find(&results, &root, "").access, Access::Incomplete);
    assert_eq!(find(&results, &root, "").size_bytes, 2 * MIB);
    assert_eq!(results.counters.inaccessible_dirs, 1);
    assert_eq!(results.issues.len(), 1);
    assert_eq!(results.issues[0].reason, IssueReason::AccessDenied);
}

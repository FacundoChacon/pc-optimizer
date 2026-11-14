use std::path::{Path, PathBuf};

use super::*;
use crate::domain::model::EpochMillis;
use crate::domain::ports::fakes::{FakeFileSystem, FsOp};
use crate::domain::ports::{FileIdentity, FileMeta, PortError};
use crate::domain::protection::context::ContextError;
use crate::domain::protection::service::PathProtection;
use crate::domain::protection::test_support::ctx_c_ana;

const TEMP: &str = r"C:\Windows\Temp";
const MODIFIED: EpochMillis = EpochMillis(1_700_000_000_000);

fn id(n: u128) -> FileIdentity {
    FileIdentity {
        volume_serial: 7,
        file_id: n,
    }
}

fn base_fs() -> FakeFileSystem {
    FakeFileSystem::new()
        .with_dir(r"C:\Windows")
        .with_dir(TEMP)
        .with_dir(r"C:\Users")
        .with_dir(r"C:\Users\ana")
        .with_dir(r"C:\Users\luis")
        .with_dir(r"C:\Users\luis\AppData")
        .with_dir(r"C:\Users\luis\AppData\Local")
        .with_dir(r"C:\Users\luis\AppData\Local\Temp")
        .with_file(r"C:\Users\luis\AppData\Local\Temp\x.tmp", 5, Some(MODIFIED))
        .with_file(r"C:\Windows\Temp\x.tmp", 2_000_000, Some(MODIFIED))
        .with_identity(r"C:\Windows\Temp\x.tmp", id(42))
        .canonical_as_is()
}

/// Entrada tal como la entrega `read_dir` del padre.
fn entry_of(fs: &FakeFileSystem, path: &str) -> DirEntry {
    let parent = Path::new(path).parent().expect("padre");
    match fs
        .read_dir(parent)
        .expect("listado")
        .into_iter()
        .find(|e| e.path == Path::new(path))
    {
        Some(entry) => entry,
        None => panic!("{path} no esta en el fake"),
    }
}

fn verify(fs: &FakeFileSystem, path: &str) -> JunkFinding {
    let entry = entry_of(fs, path);
    // Las llamadas del listado de preparacion no cuentan: se verifica solo lo que hace `check`.
    let protection = PathProtection::new(fs, Ok(ctx_c_ana()));
    let session = protection.session();
    verify_candidate(&session, JunkCategory::TempWindows, &entry)
}

fn skipped(finding: &JunkFinding) -> Option<&str> {
    match &finding.state {
        FindingState::Skipped(code) => Some(code),
        FindingState::Eligible(_) => None,
    }
}

#[test]
fn verify_should_record_an_allowed_file_as_eligible_with_real_path_size_and_identity() {
    let fs = base_fs();
    let finding = verify(&fs, r"C:\Windows\Temp\x.tmp");
    assert_eq!(finding.category, JunkCategory::TempWindows);
    assert_eq!(finding.path, r"C:\Windows\Temp\x.tmp");
    assert_eq!(finding.size_bytes, 2_000_000);
    assert_eq!(finding.modified_at, Some(MODIFIED));
    assert_eq!(finding.state, FindingState::Eligible(id(42)));
    assert!(finding.is_eligible());
}

#[test]
fn verify_should_use_the_long_real_path_when_the_listing_returns_a_short_name() {
    let fs = base_fs()
        .with_file(r"C:\Windows\Temp\LONGFI~1.TMP", 9, Some(MODIFIED))
        .with_canonical(
            r"C:\Windows\Temp\LONGFI~1.TMP",
            r"C:\Windows\Temp\long file name.tmp",
        );
    let finding = verify(&fs, r"C:\Windows\Temp\LONGFI~1.TMP");
    assert_eq!(finding.path, r"C:\Windows\Temp\long file name.tmp");
    assert!(finding.is_eligible());
}

#[test]
fn verify_should_skip_a_file_with_several_hard_links_keeping_its_listed_size() {
    let fs = base_fs()
        .with_file(r"C:\Windows\Temp\h.dll", 123, Some(MODIFIED))
        .with_hard_links(r"C:\Windows\Temp\h.dll", 2);
    let finding = verify(&fs, r"C:\Windows\Temp\h.dll");
    assert_eq!(skipped(&finding), Some("multiple_hard_links"));
    assert_eq!(finding.path, r"C:\Windows\Temp\h.dll");
    assert_eq!(finding.size_bytes, 123);
    assert!(!finding.is_eligible());
}

#[test]
fn verify_should_skip_a_cloud_placeholder_with_the_cloud_code() {
    let fs = base_fs().with_cloud_placeholder(r"C:\Windows\Temp\nube.docx", 77);
    let finding = verify(&fs, r"C:\Windows\Temp\nube.docx");
    assert_eq!(skipped(&finding), Some(SKIPPED_CLOUD));
}

#[test]
fn verify_should_skip_with_the_resolution_reason_when_metadata_fails() {
    let fs = base_fs().fail_on_op(
        r"C:\Windows\Temp\x.tmp",
        FsOp::Metadata,
        PortError::PermissionDenied("x".into()),
    );
    let entry = entry_of(&fs, r"C:\Windows\Temp\x.tmp");
    let protection = PathProtection::new(&fs, Ok(ctx_c_ana()));
    let finding = verify_candidate(&protection.session(), JunkCategory::TempWindows, &entry);
    assert_eq!(skipped(&finding), Some("unresolvable_permission_denied"));
}

#[test]
fn verify_should_skip_other_users_profiles_and_protected_roots_with_stable_codes() {
    let fs = base_fs().with_dir(r"C:\Windows\System32").with_file(
        r"C:\Windows\System32\k.dll",
        1,
        Some(MODIFIED),
    );
    let other = verify(&fs, r"C:\Users\luis\AppData\Local\Temp\x.tmp");
    assert_eq!(skipped(&other), Some("other_user_profile"));
    let system = verify(&fs, r"C:\Windows\System32\k.dll");
    assert_eq!(skipped(&system), Some("windows_dir"));
}

#[test]
fn verify_should_skip_when_the_context_is_unavailable() {
    let fs = base_fs();
    let entry = entry_of(&fs, r"C:\Windows\Temp\x.tmp");
    let protection = PathProtection::new(&fs, Err(ContextError::MissingRequired("windows_dir")));
    let finding = verify_candidate(&protection.session(), JunkCategory::TempWindows, &entry);
    assert_eq!(skipped(&finding), Some("context_unavailable"));
}

#[test]
fn verify_should_never_make_a_directory_eligible_even_when_the_rules_allow_it() {
    // Carrera: el listado vio un archivo y al verificar ya es una carpeta.
    let fs = base_fs().with_dir(r"C:\Windows\Temp\ahora-carpeta");
    let stale = DirEntry {
        path: PathBuf::from(r"C:\Windows\Temp\ahora-carpeta"),
        meta: FileMeta {
            kind: EntryKind::File,
            size_bytes: 10,
            modified: Some(MODIFIED),
            cloud_placeholder: false,
            identity: None,
            hard_links: None,
        },
    };
    let protection = PathProtection::new(&fs, Ok(ctx_c_ana()));
    let finding = verify_candidate(&protection.session(), JunkCategory::TempWindows, &stale);
    assert_eq!(skipped(&finding), Some(RESOLVED_PATH_MISMATCH));
    assert_eq!(finding.size_bytes, 10);
}

#[test]
fn reparse_point_finding_should_be_skipped_without_touching_the_target() {
    let finding = reparse_point_finding(JunkCategory::TempUser, Path::new(r"C:\Windows\Temp\j"));
    assert_eq!(finding.category, JunkCategory::TempUser);
    assert_eq!(finding.path, r"C:\Windows\Temp\j");
    assert_eq!(finding.size_bytes, 0);
    assert_eq!(finding.modified_at, None);
    assert_eq!(skipped(&finding), Some(REPARSE_POINT));
}

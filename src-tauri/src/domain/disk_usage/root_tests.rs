use super::*;
use crate::domain::ports::fakes::FakeFileSystem;
use crate::domain::protection::context::ContextError;
use crate::domain::protection::decision::{PathRejection, ProtectedReason, ResolveFailure};
use crate::domain::protection::path::normalize_lexical;
use crate::domain::protection::test_support::{ctx_c_ana, ctx_c_no_user};

fn fs() -> FakeFileSystem {
    FakeFileSystem::new()
        .canonical_as_is()
        .with_dir(r"C:\")
        .with_dir(r"C:\Users")
        .with_dir(r"C:\Users\ana")
        .with_dir(r"C:\Users\ana\Downloads")
        .with_dir(r"C:\Users\bob")
        .with_reparse_point(r"C:\Users\ana\Swapped")
        .with_file(r"C:\Users\ana\file.txt", 1, None)
}

fn picked(raw: &str) -> RootChoice {
    RootChoice::Picked(normalize_lexical(raw).expect("valid path"))
}

#[test]
fn rejection_code_should_use_plain_codes_for_missing_and_non_directory_roots() {
    use ProtectedReason as R;
    assert_eq!(
        rejection_code(R::Unresolvable(ResolveFailure::NotADirectory)),
        "not_a_directory"
    );
    assert_eq!(
        rejection_code(R::Unresolvable(ResolveFailure::NotFound)),
        "not_found"
    );
}

#[test]
fn rejection_code_should_keep_the_protection_codes_for_everything_else() {
    use ProtectedReason as R;
    let cases = [
        (R::ReparsePoint, "reparse_point"),
        (R::ReparsePointInPath, "reparse_point_in_path"),
        (R::OtherUserProfile, "other_user_profile"),
        (R::UnsupportedVolume, "unsupported_volume"),
        (R::VolumeSystemEntry, "volume_system_entry"),
        (R::ContextUnavailable, "context_unavailable"),
        (R::Unparseable(PathRejection::NetworkPath), "network_path"),
        (
            R::Unresolvable(ResolveFailure::PermissionDenied),
            "unresolvable_permission_denied",
        ),
    ];
    for (reason, code) in cases {
        assert_eq!(rejection_code(reason), code);
    }
}

#[test]
fn validate_root_should_resolve_the_system_drive_from_the_context_not_from_the_caller() {
    let root = validate_root(&fs(), &Ok(ctx_c_ana()), &RootChoice::SystemDrive).unwrap();
    assert_eq!(root.to_string(), r"C:\");
    assert_eq!(root.kind, RootKind::SystemDrive);
}

#[test]
fn validate_root_should_accept_an_own_folder_and_return_the_validated_path() {
    let root = validate_root(&fs(), &Ok(ctx_c_ana()), &picked(r"C:\Users\ana\Downloads")).unwrap();
    assert_eq!(root.to_string(), r"C:\Users\ana\Downloads");
    assert_eq!(root.kind, RootKind::Picked);
}

#[test]
fn validate_root_should_reject_with_the_protection_reason() {
    let reject = |raw: &str| {
        validate_root(&fs(), &Ok(ctx_c_ana()), &picked(raw)).map(|root| root.to_string())
    };
    assert_eq!(
        reject(r"C:\Users\ana\Swapped"),
        Err(FolderScanError::RootRejected(ProtectedReason::ReparsePoint))
    );
    assert_eq!(
        reject(r"C:\Users\bob"),
        Err(FolderScanError::RootRejected(
            ProtectedReason::OtherUserProfile
        ))
    );
    assert_eq!(
        reject(r"D:\Datos"),
        Err(FolderScanError::RootRejected(
            ProtectedReason::UnsupportedVolume
        ))
    );
    assert_eq!(
        reject(r"C:\Users\ana\file.txt"),
        Err(FolderScanError::RootRejected(
            ProtectedReason::Unresolvable(ResolveFailure::NotADirectory)
        ))
    );
}

#[test]
fn validate_root_should_fail_closed_without_a_context_and_in_over_the_shoulder() {
    let broken = validate_root(
        &fs(),
        &Err(ContextError::MissingRequired("windows_dir")),
        &RootChoice::SystemDrive,
    )
    .map(|root| root.to_string());
    assert_eq!(
        broken,
        Err(FolderScanError::RootRejected(
            ProtectedReason::ContextUnavailable
        ))
    );
    let restricted = validate_root(
        &fs(),
        &Ok(ctx_c_no_user()),
        &picked(r"C:\Users\ana\Downloads"),
    )
    .map(|root| root.to_string());
    assert_eq!(
        restricted,
        Err(FolderScanError::RootRejected(
            ProtectedReason::OtherUserProfile
        ))
    );
}

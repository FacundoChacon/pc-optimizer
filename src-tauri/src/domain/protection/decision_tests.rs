use super::*;
use crate::domain::model::CleanupOutcome;
use crate::domain::ports::{EntryKind, FileIdentity, PortError};
use crate::domain::protection::path::{PathRejection, normalize_lexical};

fn all_reasons() -> Vec<(ProtectedReason, &'static str)> {
    use ProtectedReason::*;
    vec![
        (Unparseable(PathRejection::Empty), "empty_path"),
        (Unparseable(PathRejection::NonUnicode), "non_unicode_path"),
        (Unparseable(PathRejection::NotAbsolute), "not_absolute"),
        (Unparseable(PathRejection::NetworkPath), "network_path"),
        (Unparseable(PathRejection::DevicePath), "device_path"),
        (
            Unparseable(PathRejection::AlternateDataStream),
            "alternate_data_stream",
        ),
        (
            Unparseable(PathRejection::InvalidCharacter),
            "invalid_character",
        ),
        (
            Unparseable(PathRejection::ReservedDeviceName),
            "reserved_device_name",
        ),
        (
            Unparseable(PathRejection::TrailingDotOrSpace),
            "trailing_dot_or_space",
        ),
        (Unparseable(PathRejection::EscapesRoot), "escapes_root"),
        (
            Unparseable(PathRejection::VerbatimNotNormalized),
            "verbatim_not_normalized",
        ),
        (ContextUnavailable, "context_unavailable"),
        (UnsupportedVolume, "unsupported_volume"),
        (VolumeRoot, "volume_root"),
        (VolumeSystemEntry, "volume_system_entry"),
        (Anchor(AnchorKind::UsersRoot), "anchor"),
        (Anchor(AnchorKind::Temp), "anchor"),
        (Anchor(AnchorKind::ExceptionRoot), "anchor"),
        (OtherUserProfile, "other_user_profile"),
        (WindowsDir, "windows_dir"),
        (ProgramFiles, "program_files"),
        (ProgramData, "program_data"),
        (BrowserProfile(Browser::Chrome), "browser_profile"),
        (BrowserProfile(Browser::Firefox), "browser_profile"),
        (AppDir, "app_dir"),
        (ReparsePointInPath, "reparse_point_in_path"),
        (ReparsePoint, "reparse_point"),
        (MultipleHardLinks, "multiple_hard_links"),
        (ResolvedPathMismatch, "resolved_path_mismatch"),
        (IdentityChanged, "identity_changed"),
        (
            Unresolvable(ResolveFailure::NotFound),
            "unresolvable_not_found",
        ),
        (
            Unresolvable(ResolveFailure::PermissionDenied),
            "unresolvable_permission_denied",
        ),
        (Unresolvable(ResolveFailure::InUse), "unresolvable_in_use"),
        (
            Unresolvable(ResolveFailure::Unsupported),
            "unresolvable_unsupported",
        ),
        (Unresolvable(ResolveFailure::Io), "unresolvable_io"),
        (
            Unresolvable(ResolveFailure::NotADirectory),
            "unresolvable_not_a_directory",
        ),
        (
            Unresolvable(ResolveFailure::MissingIdentity),
            "unresolvable_missing_identity",
        ),
        (
            Unresolvable(ResolveFailure::PinFailed),
            "unresolvable_pin_failed",
        ),
    ]
}

#[test]
fn code_should_match_the_stable_table() {
    for (reason, expected) in all_reasons() {
        assert_eq!(reason.code(), expected, "caso {reason:?}");
    }
}

#[test]
fn code_should_be_unique_per_distinct_reason_except_parameterized_ones() {
    let mut seen = std::collections::BTreeMap::new();
    for (reason, code) in all_reasons() {
        let family = match reason {
            ProtectedReason::Anchor(_) => "anchor".to_string(),
            ProtectedReason::BrowserProfile(_) => "browser_profile".to_string(),
            other => format!("{other:?}"),
        };
        if let Some(previous) = seen.insert(code, family.clone()) {
            assert_eq!(previous, family, "el codigo {code} colisiona");
        }
    }
}

#[test]
fn code_should_be_snake_case_ascii() {
    for (reason, _) in all_reasons() {
        let code = reason.code();
        assert!(
            !code.is_empty() && code.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
            "codigo invalido {code:?} para {reason:?}"
        );
    }
}

#[test]
fn anchor_kinds_should_classify_content_vs_structural() {
    use AnchorKind::*;
    for kind in [Temp, KnownFolder, ExceptionRoot] {
        assert_eq!(kind.class(), AnchorClass::Content, "caso {kind:?}");
    }
    for kind in [
        UsersRoot,
        UserProfile,
        LocalAppData,
        RoamingAppData,
        WindowsDir,
        ProgramFiles,
        ProgramData,
        AppDir,
        AncestorOfProtectedRoot,
    ] {
        assert_eq!(kind.class(), AnchorClass::Structural, "caso {kind:?}");
    }
}

#[test]
fn resolve_failure_should_map_every_port_error() {
    let cases = [
        (PortError::NotFound("x".into()), ResolveFailure::NotFound),
        (
            PortError::PermissionDenied("x".into()),
            ResolveFailure::PermissionDenied,
        ),
        (PortError::InUse("x".into()), ResolveFailure::InUse),
        (
            PortError::Unsupported("x".into()),
            ResolveFailure::Unsupported,
        ),
        (PortError::Io("x".into()), ResolveFailure::Io),
    ];
    for (error, expected) in cases {
        assert_eq!(ResolveFailure::from(&error), expected, "caso {error:?}");
    }
}

fn checked() -> CheckedPath {
    CheckedPath {
        path: normalize_lexical(r"C:\Windows\Temp\a.tmp").unwrap(),
        kind: EntryKind::File,
        size_bytes: 3,
        identity: FileIdentity {
            volume_serial: 1,
            file_id: 2,
        },
        rule: AllowRule::Exception(ExceptionId::WindowsTemp),
    }
}

#[test]
fn skip_outcome_should_map_each_decision_to_the_cleanup_log_result() {
    let path = normalize_lexical(r"C:\Users\ana\OneDrive\v.mp4").unwrap();
    assert_eq!(
        ProtectionDecision::Protected(ProtectedReason::WindowsDir).skip_outcome(),
        Some(CleanupOutcome::SkippedProtected)
    );
    assert_eq!(
        ProtectionDecision::Protected(ProtectedReason::Unresolvable(ResolveFailure::Io))
            .skip_outcome(),
        Some(CleanupOutcome::SkippedProtected)
    );
    assert_eq!(
        ProtectionDecision::SkippedCloud { path }.skip_outcome(),
        Some(CleanupOutcome::SkippedCloud)
    );
    assert_eq!(ProtectionDecision::Allowed(checked()).skip_outcome(), None);
}

#[test]
fn is_allowed_should_be_true_only_for_allowed() {
    let path = normalize_lexical(r"C:\a").unwrap();
    assert!(ProtectionDecision::Allowed(checked()).is_allowed());
    assert!(!ProtectionDecision::SkippedCloud { path }.is_allowed());
    assert!(!ProtectionDecision::Protected(ProtectedReason::VolumeRoot).is_allowed());
}

#[test]
fn rule_verdict_should_convert_to_the_matching_reason_or_rule() {
    assert!(RuleVerdict::Allowed(AllowRule::Default).is_allowed());
    assert!(!RuleVerdict::Protected(ProtectedReason::AppDir).is_allowed());
}

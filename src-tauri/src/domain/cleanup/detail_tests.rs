use super::*;
use crate::domain::model::CleanupOutcome as O;
use crate::domain::ports::TrashError;
use crate::domain::protection::decision::{
    AnchorKind, CheckedPath, ProtectedReason, ProtectionDecision, ResolveFailure,
};
use crate::domain::protection::path::normalize_lexical;

fn protected(reason: ProtectedReason) -> ProtectionDecision {
    ProtectionDecision::Protected(reason)
}

#[test]
fn outcome_for_decision_should_map_every_protection_case_of_the_d06_table() {
    let cloud = ProtectionDecision::SkippedCloud {
        path: normalize_lexical(r"C:\Users\ana\OneDrive\a.tmp").unwrap(),
    };
    let table = [
        (
            protected(ProtectedReason::IdentityChanged),
            O::SkippedProtected,
            "identity_changed",
        ),
        (
            protected(ProtectedReason::ReparsePoint),
            O::SkippedProtected,
            "reparse_point",
        ),
        (
            protected(ProtectedReason::MultipleHardLinks),
            O::SkippedProtected,
            "multiple_hard_links",
        ),
        (
            protected(ProtectedReason::OtherUserProfile),
            O::SkippedProtected,
            "other_user_profile",
        ),
        (
            protected(ProtectedReason::ContextUnavailable),
            O::SkippedProtected,
            "context_unavailable",
        ),
        (
            protected(ProtectedReason::Anchor(AnchorKind::Temp)),
            O::SkippedProtected,
            "anchor",
        ),
        (
            protected(ProtectedReason::Unresolvable(ResolveFailure::NotFound)),
            O::SkippedProtected,
            "unresolvable_not_found",
        ),
        (
            protected(ProtectedReason::Unresolvable(ResolveFailure::PinFailed)),
            O::SkippedProtected,
            "unresolvable_pin_failed",
        ),
        // Un archivo en uso no es "protegido": se omite como bloqueado (D-06).
        (
            protected(ProtectedReason::Unresolvable(ResolveFailure::InUse)),
            O::SkippedLocked,
            "unresolvable_in_use",
        ),
        (cloud, O::SkippedCloud, "skipped_cloud"),
    ];
    for (decision, outcome, code) in table {
        assert_eq!(
            outcome_for_decision(&decision),
            Some((outcome, code)),
            "{decision:?}"
        );
    }
}

#[test]
fn outcome_for_decision_should_give_nothing_for_an_allowed_path() {
    let checked = CheckedPath {
        path: normalize_lexical(r"C:\Users\ana\AppData\Local\Temp\a.tmp").unwrap(),
        kind: crate::domain::ports::EntryKind::File,
        size_bytes: 1,
        modified: None,
        identity: crate::domain::ports::FileIdentity {
            volume_serial: 1,
            file_id: 1,
        },
        rule: crate::domain::protection::decision::AllowRule::Default,
    };
    assert_eq!(
        outcome_for_decision(&ProtectionDecision::Allowed(checked)),
        None
    );
}

#[test]
fn outcome_for_trash_should_map_every_error_of_the_d06_table() {
    let table = [
        (TrashError::InUse, O::SkippedLocked, "in_use"),
        (
            TrashError::BinUnavailable,
            O::Error,
            "recycle_bin_unavailable",
        ),
        (TrashError::TooLarge, O::Error, "recycle_bin_too_small"),
        (TrashError::PathTooLong, O::Error, "path_too_long"),
        (TrashError::PermissionDenied, O::Error, "permission_denied"),
        (TrashError::NotFound, O::Error, "not_found"),
        (
            TrashError::BackendUnavailable,
            O::Error,
            "trash_unavailable",
        ),
        (
            TrashError::Other("io_0x80270021".into()),
            O::Error,
            "io_0x80270021",
        ),
    ];
    for (error, outcome, code) in table {
        assert_eq!(
            outcome_for_trash(&error),
            (outcome, code.to_string()),
            "{error:?}"
        );
    }
}

#[test]
fn outcome_for_trash_should_fall_back_to_io_for_codes_that_are_not_stable_ascii() {
    for raw in [
        "",
        r"C:\Users\ana\a.tmp",
        "Acceso denegado",
        "io 0x1",
        "IO_0X1",
    ] {
        assert_eq!(
            outcome_for_trash(&TrashError::Other(raw.into())),
            (O::Error, "io".to_string()),
            "{raw:?}"
        );
    }
}

#[test]
fn every_detail_code_should_be_ascii_snake_case_without_backslashes() {
    for code in ALL_CODES {
        assert!(!code.is_empty());
        assert!(
            code.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'),
            "{code}"
        );
    }
    let mut unique = ALL_CODES.to_vec();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), ALL_CODES.len(), "codigos repetidos");
}

#[test]
fn blocked_detail_should_name_the_mismatch_reason() {
    use crate::domain::privileges::MismatchReason;
    assert_eq!(
        blocked_detail(MismatchReason::DifferentUser),
        "blocked_different_user"
    );
    assert_eq!(
        blocked_detail(MismatchReason::Undetermined),
        "blocked_undetermined"
    );
}

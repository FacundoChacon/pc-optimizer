use super::*;
use crate::domain::duplicates::model::RequestError;
use crate::domain::ports::PortError;

#[test]
fn root_request_should_parse_the_system_drive_without_a_pick_id() {
    assert_eq!(
        RootRequest::parse("system_drive", None),
        Ok(RootRequest::SystemDrive)
    );
}

#[test]
fn root_request_should_parse_a_picked_root_with_its_id() {
    let parsed = RootRequest::parse("picked", Some(7));
    assert_eq!(
        parsed,
        Ok(RootRequest::Picked(PickId::new(7).expect("valid id")))
    );
    let max = RootRequest::parse("picked", Some((1 << 53) - 1));
    assert!(matches!(max, Ok(RootRequest::Picked(_))));
}

#[test]
fn root_request_should_reject_unknown_kinds_and_bad_pick_ids() {
    let invalid = |result: Result<RootRequest, RequestError>| {
        assert!(matches!(result, Err(RequestError::InvalidArgument(_))));
    };
    invalid(RootRequest::parse("everything", None));
    invalid(RootRequest::parse("picked", None));
    invalid(RootRequest::parse("picked", Some(-1)));
    invalid(RootRequest::parse("picked", Some(1 << 53)));
    invalid(RootRequest::parse("system_drive", Some(3)));
}

#[test]
fn stable_codes_should_roundtrip_and_reject_unknown_values() {
    for kind in RootKind::ALL.iter().copied() {
        assert_eq!(RootKind::parse(kind.as_str()), Some(kind));
    }
    for access in Access::ALL.iter().copied() {
        assert_eq!(Access::parse(access.as_str()), Some(access));
    }
    for reason in OpaqueReason::ALL.iter().copied() {
        assert_eq!(OpaqueReason::parse(reason.as_str()), Some(reason));
    }
    for reason in IssueReason::ALL.iter().copied() {
        assert_eq!(IssueReason::parse(reason.as_str()), Some(reason));
    }
    assert_eq!(Access::parse("partial"), None);
    assert_eq!(OpaqueReason::parse("appdata"), None);
    assert_eq!(IssueReason::parse("timeout"), None);
    assert_eq!(RootKind::parse("other"), None);
    assert_eq!(OpaqueReason::RecycleBin.as_str(), "recycle_bin");
    assert_eq!(
        OpaqueReason::SystemVolumeInformation.as_str(),
        "system_volume_information"
    );
}

#[test]
fn issue_reason_should_come_from_the_port_error_kind() {
    let cases = [
        (
            PortError::PermissionDenied("x".into()),
            IssueReason::AccessDenied,
        ),
        (PortError::NotFound("x".into()), IssueReason::NotFound),
        (PortError::Io("x".into()), IssueReason::IoError),
        (PortError::InUse("x".into()), IssueReason::IoError),
        (PortError::Unsupported("x".into()), IssueReason::IoError),
    ];
    for (error, reason) in cases {
        assert_eq!(IssueReason::from_port_error(&error), reason);
    }
}

#[test]
fn limits_should_match_the_approved_design() {
    assert_eq!(MAX_STORED_FOLDERS, 20_000);
    assert_eq!(MAX_LARGE_FILES, 500);
    assert_eq!(MAX_ISSUES, 50);
}

#[test]
fn sort_orders_should_parse_their_ipc_names() {
    assert_eq!(FolderSort::parse("size"), Some(FolderSort::Size));
    assert_eq!(FolderSort::parse("name"), Some(FolderSort::Name));
    assert_eq!(FolderSort::parse("date"), None);
    assert_eq!(LargeFileSort::parse("newest"), Some(LargeFileSort::Newest));
    assert_eq!(LargeFileSort::parse("oldest"), Some(LargeFileSort::Oldest));
    assert_eq!(LargeFileSort::parse("size"), Some(LargeFileSort::Size));
    assert_eq!(LargeFileSort::parse("name"), None);
}

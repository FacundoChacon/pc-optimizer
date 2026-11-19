use super::*;

fn keys(list: &[&str]) -> Option<Vec<String>> {
    Some(list.iter().map(|k| (*k).to_string()).collect())
}

#[test]
fn min_size_should_accept_only_the_four_closed_options() {
    for bytes in [102_400, 1_048_576, 10_485_760, 104_857_600] {
        assert_eq!(MinSize::new(bytes).map(MinSize::bytes), Some(bytes));
    }
    for bytes in [0, 5, 102_399, 1_048_577, 1_000_000, u64::MAX] {
        assert_eq!(MinSize::new(bytes), None, "{bytes}");
    }
}

#[test]
fn min_size_should_default_to_one_mebibyte() {
    assert_eq!(MinSize::default().bytes(), 1_048_576);
    let request = DuplicateScanRequest::new(None, None).unwrap();
    assert_eq!(request.min_size(), MinSize::MIB_1);
}

#[test]
fn request_should_select_the_six_folders_by_default() {
    let request = DuplicateScanRequest::new(None, None).unwrap();
    assert_eq!(request.roots(), ContentFolder::ALL);
}

#[test]
fn request_should_select_only_the_requested_folders_in_stable_order() {
    let only = DuplicateScanRequest::new(keys(&["downloads"]), None).unwrap();
    assert_eq!(only.roots(), [ContentFolder::Downloads]);
    let two = DuplicateScanRequest::new(keys(&["videos", "desktop"]), Some(10_485_760)).unwrap();
    assert_eq!(two.roots(), [ContentFolder::Desktop, ContentFolder::Videos]);
    assert_eq!(two.min_size(), MinSize::MIB_10);
}

#[test]
fn request_should_reject_unknown_repeated_or_empty_folder_lists() {
    for list in [
        keys(&["downloads", "appdata"]),
        keys(&["downloads", "downloads"]),
        keys(&[]),
        keys(&["Downloads"]),
        keys(&[""]),
    ] {
        assert!(
            matches!(
                DuplicateScanRequest::new(list.clone(), None),
                Err(RequestError::InvalidArgument(_))
            ),
            "{list:?}"
        );
    }
}

#[test]
fn request_should_reject_a_min_size_outside_the_list() {
    for bytes in [5, 0, 2_097_152] {
        assert!(
            matches!(
                DuplicateScanRequest::new(None, Some(bytes)),
                Err(RequestError::InvalidArgument(_))
            ),
            "{bytes}"
        );
    }
}

#[test]
fn root_status_should_round_trip_with_its_reason() {
    for status in [
        RootStatus::Done,
        RootStatus::NotSelected,
        RootStatus::Nested,
        RootStatus::Unavailable(RootReason::UnsupportedVolume),
        RootStatus::Unavailable(RootReason::Cloud),
    ] {
        let reason = status.reason().map(RootReason::as_str);
        assert_eq!(RootStatus::parse(status.as_str(), reason), Some(status));
    }
}

#[test]
fn root_status_should_reject_inconsistent_status_and_reason() {
    assert_eq!(RootStatus::parse("done", Some("cloud")), None);
    assert_eq!(RootStatus::parse("unavailable", None), None);
    assert_eq!(RootStatus::parse("unavailable", Some("weird")), None);
    assert_eq!(RootStatus::parse("gone", None), None);
}

#[test]
fn reclaimable_bytes_should_be_size_times_copies_minus_one() {
    let file = |path: &str, id: u128| DuplicateFile {
        root: ContentFolder::Downloads,
        path: path.into(),
        modified: None,
        identity: FileIdentity {
            volume_serial: 1,
            file_id: id,
        },
    };
    let two = DuplicateGroup {
        size_bytes: 4 << 20,
        blake3: "0".repeat(64),
        files: vec![file("a", 1), file("b", 2)],
    };
    let three = DuplicateGroup {
        size_bytes: 10 << 20,
        blake3: "1".repeat(64),
        files: vec![file("a", 1), file("b", 2), file("c", 3)],
    };
    assert_eq!(two.reclaimable_bytes(), 4 << 20);
    assert_eq!(three.reclaimable_bytes(), 20 << 20);
}

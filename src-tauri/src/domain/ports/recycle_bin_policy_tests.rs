//! Tests de la decision pura de politica de la Papelera (design D-12 B).

use super::*;

const MB: u64 = 1024 * 1024;

fn raw() -> RawBinPolicy {
    RawBinPolicy::default()
}

#[test]
fn evaluate_policy_should_report_enabled_without_limit_when_nothing_is_set() {
    assert_eq!(
        evaluate_policy(&raw()),
        BinAvailability::Enabled {
            max_capacity_bytes: None
        }
    );
}

#[test]
fn evaluate_policy_should_disable_on_each_source_set_to_one() {
    let cases = [
        (
            RawBinPolicy {
                nuke_on_delete_volume: Some(1),
                ..raw()
            },
            DisabledReason::NukeOnDeleteVolume,
        ),
        (
            RawBinPolicy {
                nuke_on_delete_global: Some(1),
                ..raw()
            },
            DisabledReason::NukeOnDeleteGlobal,
        ),
        (
            RawBinPolicy {
                no_recycle_files_user: Some(1),
                ..raw()
            },
            DisabledReason::PolicyUser,
        ),
        (
            RawBinPolicy {
                no_recycle_files_machine: Some(1),
                ..raw()
            },
            DisabledReason::PolicyMachine,
        ),
    ];
    for (policy, reason) in cases {
        assert_eq!(
            evaluate_policy(&policy),
            BinAvailability::Disabled(reason),
            "{policy:?}"
        );
    }
}

#[test]
fn evaluate_policy_should_treat_any_nonzero_value_as_disabled_and_zero_as_enabled() {
    let nonzero = RawBinPolicy {
        nuke_on_delete_volume: Some(2),
        ..raw()
    };
    assert!(matches!(
        evaluate_policy(&nonzero),
        BinAvailability::Disabled(_)
    ));
    let zero = RawBinPolicy {
        nuke_on_delete_volume: Some(0),
        nuke_on_delete_global: Some(0),
        no_recycle_files_user: Some(0),
        no_recycle_files_machine: Some(0),
        max_capacity_mb: None,
    };
    assert!(matches!(
        evaluate_policy(&zero),
        BinAvailability::Enabled { .. }
    ));
}

#[test]
fn evaluate_policy_should_convert_the_capacity_from_megabytes_to_bytes() {
    let policy = RawBinPolicy {
        max_capacity_mb: Some(24_907),
        ..raw()
    };
    assert_eq!(
        evaluate_policy(&policy),
        BinAvailability::Enabled {
            max_capacity_bytes: Some(24_907 * MB)
        }
    );
}

#[test]
fn evaluate_policy_should_let_a_disabling_source_win_over_the_capacity() {
    let policy = RawBinPolicy {
        max_capacity_mb: Some(100),
        no_recycle_files_machine: Some(1),
        ..raw()
    };
    assert_eq!(
        evaluate_policy(&policy),
        BinAvailability::Disabled(DisabledReason::PolicyMachine)
    );
}

#[test]
fn check_move_should_accept_a_file_within_the_capacity_or_when_it_is_unknown() {
    let limited = BinAvailability::Enabled {
        max_capacity_bytes: Some(10 * MB),
    };
    assert_eq!(check_move(&limited, 10 * MB), Ok(()));
    assert_eq!(check_move(&limited, 0), Ok(()));
    let unknown = BinAvailability::Enabled {
        max_capacity_bytes: None,
    };
    assert_eq!(check_move(&unknown, u64::MAX), Ok(()));
}

#[test]
fn check_move_should_refuse_a_file_over_the_capacity_as_too_large() {
    let limited = BinAvailability::Enabled {
        max_capacity_bytes: Some(10 * MB),
    };
    assert_eq!(check_move(&limited, 10 * MB + 1), Err(TrashError::TooLarge));
    let zero = BinAvailability::Enabled {
        max_capacity_bytes: Some(0),
    };
    assert_eq!(check_move(&zero, 1), Err(TrashError::TooLarge));
}

#[test]
fn check_move_should_refuse_when_disabled_as_bin_unavailable() {
    let disabled = BinAvailability::Disabled(DisabledReason::PolicyUser);
    assert_eq!(check_move(&disabled, 0), Err(TrashError::BinUnavailable));
}

#[test]
fn availability_check_should_fail_closed_when_the_policy_cannot_be_read() {
    use std::path::Path;
    let unreadable = FailingPolicy;
    assert_eq!(
        check_for_move(&unreadable, Path::new(r"C:\x.tmp"), 1),
        Err(TrashError::BinUnavailable)
    );
}

struct FailingPolicy;

impl RecycleBinPolicy for FailingPolicy {
    fn availability(&self, _path: &std::path::Path) -> Result<BinAvailability, PortError> {
        Err(PortError::Io("registry read failed".into()))
    }
}

//! Tests de `prepare_empty` y `run_empty_recycle_bin` con dobles: nada toca la Papelera real.

use std::path::PathBuf;

use super::*;
use crate::domain::ports::EmptyError;
use crate::domain::ports::fakes::{AfterEmpty, FakeRecycleBin, FakeRecycleBinEmptier, FixedClock};
use crate::domain::privileges::{MismatchReason, UserMatch};
use crate::domain::recycle_bin::outcome::EmptyStatus;

const NOW: EpochMillis = EpochMillis(1_800_000_000_000);
const TTL: i64 = 2 * 60 * 1000;

fn matched() -> PrivilegeContext {
    PrivilegeContext::new(UserMatch::Match, true)
}

fn c_root() -> Option<VolumeRoot> {
    VolumeRoot::from_drive_letter('C')
}

fn bin_12() -> FakeRecycleBin {
    FakeRecycleBin::with_usage(12, 340_000_000)
}

fn prepared(bin: &FakeRecycleBin) -> PreparedEmpty {
    prepare_empty(&matched(), c_root(), bin, NOW, 1).unwrap()
}

fn acknowledged(bin: &FakeRecycleBin, token: u64) -> AcknowledgedEmpty {
    prepared(bin).acknowledge(NOW, token).unwrap()
}

fn run(
    ack: AcknowledgedEmpty,
    token: u64,
    privileges: &PrivilegeContext,
    bin: &FakeRecycleBin,
    emptier: &FakeRecycleBinEmptier,
    now: EpochMillis,
) -> Result<EmptyOutcome, EmptyFlowError> {
    run_empty_recycle_bin(ack, token, privileges, bin, emptier, &FixedClock(now))
}

// --- prepare_empty ---

#[test]
fn prepare_should_measure_exactly_the_system_root_and_keep_what_it_measured() {
    let bin = bin_12();
    let request = prepare_empty(&matched(), c_root(), &bin, NOW, 7).unwrap();
    assert_eq!(request.id(), 7);
    assert_eq!(request.usage().items, 12);
    assert_eq!(request.usage().bytes, 340_000_000);
    assert_eq!(request.created_at(), NOW);
    assert_eq!(request.expires_at(), EpochMillis(NOW.0 + TTL));
    assert_eq!(bin.queried(), vec![PathBuf::from("C:\\")]);
}

#[test]
fn prepare_should_use_the_given_drive_not_a_fixed_letter() {
    let bin = FakeRecycleBin::with_usage(1, 10);
    let root = VolumeRoot::from_drive_letter('d');
    prepare_empty(&matched(), root, &bin, NOW, 1).unwrap();
    assert_eq!(bin.queried(), vec![PathBuf::from("D:\\")]);
}

#[test]
fn prepare_should_block_in_mismatch_without_measuring() {
    for reason in [MismatchReason::DifferentUser, MismatchReason::Undetermined] {
        let bin = bin_12();
        let privileges = PrivilegeContext::new(UserMatch::Mismatch(reason), true);
        let error = prepare_empty(&privileges, c_root(), &bin, NOW, 1).unwrap_err();
        assert_eq!(error, EmptyFlowError::Blocked(reason));
        assert_eq!(bin.queried(), Vec::<PathBuf>::new(), "{reason:?}");
    }
}

#[test]
fn prepare_should_be_unavailable_without_a_system_drive_and_without_measuring() {
    let bin = bin_12();
    assert_eq!(
        prepare_empty(&matched(), None, &bin, NOW, 1).unwrap_err(),
        EmptyFlowError::Unavailable
    );
    assert_eq!(bin.queried(), Vec::<PathBuf>::new());
}

#[test]
fn prepare_should_be_unavailable_when_the_measure_fails() {
    let bin = FakeRecycleBin::failing(PortError::PermissionDenied("bin".into()));
    assert_eq!(
        prepare_empty(&matched(), c_root(), &bin, NOW, 1).unwrap_err(),
        EmptyFlowError::Unavailable
    );
}

#[test]
fn prepare_should_report_an_already_empty_bin() {
    let bin = FakeRecycleBin::with_usage(0, 0);
    assert_eq!(
        prepare_empty(&matched(), c_root(), &bin, NOW, 1).unwrap_err(),
        EmptyFlowError::AlreadyEmpty
    );
}

// --- run_empty_recycle_bin: rechazos (el vaciador no se llama) ---

#[test]
fn run_should_block_in_mismatch_without_measuring_or_emptying() {
    for reason in [MismatchReason::DifferentUser, MismatchReason::Undetermined] {
        let bin = bin_12();
        let ack = acknowledged(&bin, 5);
        let emptier = bin.emptier(AfterEmpty::Empty);
        let before_queries = bin.queried().len();
        let privileges = PrivilegeContext::new(UserMatch::Mismatch(reason), true);
        let error = run(ack, 5, &privileges, &bin, &emptier, NOW).unwrap_err();
        assert_eq!(error, EmptyFlowError::Blocked(reason));
        assert_eq!(bin.queried().len(), before_queries, "no mide");
        assert_eq!(emptier.calls(), Vec::<PathBuf>::new());
    }
}

#[test]
fn run_should_reject_a_wrong_token_as_not_found() {
    let bin = bin_12();
    let ack = acknowledged(&bin, 5);
    let emptier = bin.emptier(AfterEmpty::Empty);
    assert_eq!(
        run(ack, 6, &matched(), &bin, &emptier, NOW).unwrap_err(),
        EmptyFlowError::NotFound
    );
    assert_eq!(emptier.calls(), Vec::<PathBuf>::new());
}

#[test]
fn run_should_reject_an_expired_request() {
    let bin = bin_12();
    let ack = acknowledged(&bin, 5);
    let emptier = bin.emptier(AfterEmpty::Empty);
    let late = EpochMillis(NOW.0 + TTL + 1);
    assert_eq!(
        run(ack, 5, &matched(), &bin, &emptier, late).unwrap_err(),
        EmptyFlowError::Expired
    );
    assert_eq!(emptier.calls(), Vec::<PathBuf>::new());
}

#[test]
fn run_should_reject_with_changed_when_one_item_more_or_less_or_other_bytes() {
    for (items, bytes) in [
        (13, 340_000_000),
        (11, 340_000_000),
        (12, 340_000_001),
        (12, 339_999_999),
        (0, 0),
    ] {
        let bin = bin_12();
        let ack = acknowledged(&bin, 5);
        bin.set_usage(items, bytes);
        let emptier = bin.emptier(AfterEmpty::Empty);
        assert_eq!(
            run(ack, 5, &matched(), &bin, &emptier, NOW).unwrap_err(),
            EmptyFlowError::Changed,
            "({items}, {bytes})"
        );
        assert_eq!(emptier.calls(), Vec::<PathBuf>::new(), "({items}, {bytes})");
    }
}

#[test]
fn run_should_be_unavailable_when_the_remeasure_fails() {
    let bin = bin_12();
    let ack = acknowledged(&bin, 5);
    bin.set_failing(PortError::Io("x".into()));
    let emptier = bin.emptier(AfterEmpty::Empty);
    assert_eq!(
        run(ack, 5, &matched(), &bin, &emptier, NOW).unwrap_err(),
        EmptyFlowError::Unavailable
    );
    assert_eq!(emptier.calls(), Vec::<PathBuf>::new());
}

// --- run_empty_recycle_bin: camino feliz ---

#[test]
fn run_should_call_the_emptier_once_with_the_request_root_then_measure_and_classify() {
    let bin = bin_12();
    let ack = acknowledged(&bin, 5);
    let emptier = bin.emptier(AfterEmpty::Empty);
    let finished = EpochMillis(NOW.0 + 900);
    let outcome = run(ack, 5, &matched(), &bin, &emptier, finished).unwrap();
    assert_eq!(emptier.calls(), vec![PathBuf::from("C:\\")]);
    assert_eq!(outcome.result.status, EmptyStatus::Emptied);
    assert_eq!(outcome.result.freed_bytes, 340_000_000);
    assert_eq!(outcome.before.items, 12);
    assert_eq!(outcome.finished_at, finished);
    // Medicion de la solicitud, re-medicion y medicion posterior.
    assert_eq!(bin.queried().len(), 3);
}

#[test]
fn run_should_report_a_partial_and_a_failed_outcome_from_the_post_measure() {
    let bin = bin_12();
    let ack = acknowledged(&bin, 5);
    let emptier = bin.emptier(AfterEmpty::Leaves {
        items: 2,
        bytes: 40_000_000,
    });
    let outcome = run(ack, 5, &matched(), &bin, &emptier, NOW).unwrap();
    assert_eq!(outcome.result.status, EmptyStatus::Partial);
    assert_eq!(outcome.result.freed_bytes, 300_000_000);

    let bin = bin_12();
    let ack = acknowledged(&bin, 5);
    let emptier = bin
        .emptier(AfterEmpty::Unchanged)
        .failing_with(EmptyError::PermissionDenied);
    let outcome = run(ack, 5, &matched(), &bin, &emptier, NOW).unwrap();
    assert_eq!(outcome.result.status, EmptyStatus::Failed);
    assert_eq!(outcome.result.detail.as_deref(), Some("permission_denied"));
    assert_eq!(emptier.calls().len(), 1);
}

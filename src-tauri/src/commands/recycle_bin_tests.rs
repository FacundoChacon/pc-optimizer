//! Tests de los comandos de vaciado con dobles: nada toca la Papelera real ni el disco.

use std::cell::Cell;
use std::path::PathBuf;

use serde_json::{Value, json};

use super::*;
use crate::domain::model::{CleanupLogEntry, CleanupOutcome, EpochMillis, NewCleanupLogEntry};
use crate::domain::ports::fakes::{
    AfterEmpty, FakeRecycleBin, FakeRecycleBinEmptier, InMemoryStore,
};
use crate::domain::ports::{CleanupLogStore, EmptyError, RecycleBinEmptier, StoreError};
use crate::domain::privileges::{MismatchReason, UserMatch};
use crate::domain::recycle_bin::root::VolumeRoot;

const T0: i64 = 1_800_000_000_000;
const TTL: i64 = 2 * 60 * 1000;

struct TestClock(Cell<i64>);

impl Clock for TestClock {
    fn now(&self) -> EpochMillis {
        EpochMillis(self.0.get())
    }
}

fn matched() -> PrivilegeContext {
    PrivilegeContext::new(UserMatch::Match, true)
}

fn mismatched(reason: MismatchReason) -> PrivilegeContext {
    PrivilegeContext::new(UserMatch::Mismatch(reason), true)
}

fn c_root() -> Option<VolumeRoot> {
    VolumeRoot::from_drive_letter('C')
}

fn kind(error: &AppError) -> String {
    serde_json::to_value(error).unwrap()["kind"]
        .as_str()
        .unwrap()
        .to_owned()
}

struct Env {
    store: InMemoryStore,
    bin: FakeRecycleBin,
    emptier: FakeRecycleBinEmptier,
    clock: TestClock,
    requests: EmptyRequests,
    active: ActiveJob,
}

impl Env {
    fn new(after: AfterEmpty) -> Self {
        let bin = FakeRecycleBin::with_usage(12, 340_000_000);
        let emptier = bin.emptier(after);
        Self {
            store: InMemoryStore::new(),
            bin,
            emptier,
            clock: TestClock(Cell::new(T0)),
            requests: EmptyRequests::default(),
            active: ActiveJob::default(),
        }
    }

    fn ports(
        &self,
    ) -> EmptyPorts<'_, InMemoryStore, FakeRecycleBin, FakeRecycleBinEmptier, TestClock> {
        EmptyPorts {
            store: &self.store,
            bin: &self.bin,
            emptier: &self.emptier,
            clock: &self.clock,
            requests: &self.requests,
            active: &self.active,
        }
    }

    fn prepare(&self) -> EmptyRequestDto {
        prepare_empty_run(&self.ports(), &matched(), c_root()).unwrap()
    }

    fn acknowledge(&self, request_id: u64) -> EmptyAckDto {
        acknowledge_empty_run(&self.ports(), &matched(), safe(request_id)).unwrap()
    }

    /// Prepara y reconoce: devuelve `(id, token)`.
    fn ready(&self) -> (i64, i64) {
        let request = self.prepare();
        let ack = self.acknowledge(request.request_id);
        (safe(request.request_id), safe(ack.confirm_token))
    }

    fn empty(&self, id: i64, token: i64) -> Result<EmptyResultDto, AppError> {
        empty_run(&self.ports(), &matched(), id, token)
    }

    fn log(&self) -> Vec<CleanupLogEntry> {
        self.store.recent_cleanup_log(50).unwrap()
    }

    fn emptier_calls(&self) -> Vec<PathBuf> {
        self.emptier.calls()
    }
}

fn safe(id: u64) -> i64 {
    i64::try_from(id).unwrap()
}

// --- prepare ---

#[test]
fn prepare_should_answer_the_exact_dto_without_paths_and_count_ids_from_one() {
    let env = Env::new(AfterEmpty::Empty);
    let first = env.prepare();
    assert_eq!(
        serde_json::to_value(&first).unwrap(),
        json!({
            "requestId": 1,
            "itemCount": 12,
            "sizeBytes": 340_000_000,
            "createdAt": T0,
            "expiresAt": T0 + TTL,
        })
    );
    assert_eq!(env.prepare().request_id, 2);
    assert_eq!(env.bin.queried(), vec![PathBuf::from("C:\\"); 2]);
    assert_eq!(env.emptier_calls(), Vec::<PathBuf>::new());
}

#[test]
fn prepare_should_reject_without_consuming_in_mismatch_or_while_another_job_runs() {
    let env = Env::new(AfterEmpty::Empty);
    let (id, token) = env.ready();
    for reason in [MismatchReason::DifferentUser, MismatchReason::Undetermined] {
        let error = prepare_empty_run(&env.ports(), &mismatched(reason), c_root()).unwrap_err();
        assert_eq!(kind(&error), "recycle_bin_blocked");
        assert!(error.to_string().ends_with(reason.as_str()));
    }
    assert_eq!(env.bin.queried().len(), 1, "en mismatch no se mide");
    let scan = env.active.begin_scan().unwrap();
    let error = prepare_empty_run(&env.ports(), &matched(), c_root()).unwrap_err();
    assert_eq!(kind(&error), "scan_in_progress");
    drop(scan);
    // La solicitud reconocida sigue vigente.
    assert!(env.empty(id, token).is_ok());
}

#[test]
fn prepare_should_answer_unavailable_empty_and_measure_failures() {
    let env = Env::new(AfterEmpty::Empty);
    let none = prepare_empty_run(&env.ports(), &matched(), None).unwrap_err();
    assert_eq!(kind(&none), "recycle_bin_unavailable");

    env.bin.set_usage(0, 0);
    let empty = prepare_empty_run(&env.ports(), &matched(), c_root()).unwrap_err();
    assert_eq!(kind(&empty), "recycle_bin_already_empty");

    env.bin
        .set_failing(crate::domain::ports::PortError::Io("x".into()));
    let failing = prepare_empty_run(&env.ports(), &matched(), c_root()).unwrap_err();
    assert_eq!(kind(&failing), "recycle_bin_unavailable");
    assert!(env.log().is_empty(), "un rechazo no se registra");
}

#[test]
fn prepare_should_replace_the_previous_request_in_any_stage() {
    let env = Env::new(AfterEmpty::Empty);
    let (first_id, first_token) = env.ready();
    let second = env.prepare();
    assert_eq!(
        kind(&env.empty(first_id, first_token).unwrap_err()),
        "empty_request_not_found"
    );
    // La segunda quedo consumida por el intento anterior (el slot se vacia siempre).
    assert_eq!(
        kind(&env.empty(safe(second.request_id), 1).unwrap_err()),
        "empty_request_not_found"
    );
}

// --- acknowledge ---

#[test]
fn acknowledge_should_answer_the_exact_dto_with_a_token_distinct_from_the_id() {
    let env = Env::new(AfterEmpty::Empty);
    let request = env.prepare();
    let ack = env.acknowledge(request.request_id);
    assert_ne!(ack.confirm_token, ack.request_id);
    let value = serde_json::to_value(&ack).unwrap();
    assert_eq!(value["requestId"], 1);
    assert_eq!(value["confirmToken"], ack.confirm_token);
    assert_eq!(value["itemCount"], 12);
    assert_eq!(value["sizeBytes"], 340_000_000);
    assert_eq!(value["expiresAt"], T0 + TTL);
    assert_eq!(value.as_object().unwrap().len(), 5);
    // Los tokens de dos solicitudes distintas tampoco se repiten.
    let other = env.prepare();
    assert_ne!(
        env.acknowledge(other.request_id).confirm_token,
        ack.confirm_token
    );
}

#[test]
fn acknowledge_should_fail_for_unknown_twice_and_expired_requests() {
    let env = Env::new(AfterEmpty::Empty);
    let unknown = acknowledge_empty_run(&env.ports(), &matched(), 9).unwrap_err();
    assert_eq!(kind(&unknown), "empty_request_not_found");

    let request = env.prepare();
    env.acknowledge(request.request_id);
    let twice =
        acknowledge_empty_run(&env.ports(), &matched(), safe(request.request_id)).unwrap_err();
    assert_eq!(kind(&twice), "empty_request_not_found");

    let late = env.prepare();
    env.clock.0.set(T0 + TTL + 1);
    let expired =
        acknowledge_empty_run(&env.ports(), &matched(), safe(late.request_id)).unwrap_err();
    assert_eq!(kind(&expired), "empty_request_expired");
}

#[test]
fn acknowledge_should_reject_without_consuming_in_mismatch_or_while_busy() {
    let env = Env::new(AfterEmpty::Empty);
    let request = env.prepare();
    let id = safe(request.request_id);
    let blocked =
        acknowledge_empty_run(&env.ports(), &mismatched(MismatchReason::Undetermined), id)
            .unwrap_err();
    assert_eq!(kind(&blocked), "recycle_bin_blocked");
    let cleanup = env
        .active
        .begin_cleanup(crate::domain::cleanup::model::PreviewId(1))
        .unwrap();
    let busy = acknowledge_empty_run(&env.ports(), &matched(), id).unwrap_err();
    assert_eq!(kind(&busy), "cleanup_in_progress");
    drop(cleanup);
    assert!(acknowledge_empty_run(&env.ports(), &matched(), id).is_ok());
}

// --- empty: camino feliz y resultados ---

#[test]
fn empty_should_answer_the_exact_dto_log_one_row_and_call_the_emptier_once() {
    let env = Env::new(AfterEmpty::Empty);
    let (id, token) = env.ready();
    env.clock.0.set(T0 + 1_500);
    let result = env.empty(id, token).unwrap();
    assert_eq!(
        serde_json::to_value(&result).unwrap(),
        json!({
            "status": "emptied",
            "itemsBefore": 12,
            "bytesBefore": 340_000_000,
            "itemsAfter": 0,
            "bytesAfter": 0,
            "freedBytes": 340_000_000,
            "detail": null,
            "logged": true,
            "finishedAt": T0 + 1_500,
        })
    );
    assert_eq!(env.emptier_calls(), vec![PathBuf::from("C:\\")]);
    let log = env.log();
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].category, "recycle_bin");
    assert_eq!(log[0].path, "C:\\");
    assert_eq!(log[0].size_bytes, 340_000_000);
    assert_eq!(log[0].outcome, CleanupOutcome::RecycleBinEmptied);
    assert_eq!(log[0].scan_id, None);
    assert_eq!(log[0].error_detail, None);
    assert!(env.active.ensure_idle().is_ok(), "el lugar se libera");
    // De un solo uso.
    assert_eq!(
        kind(&env.empty(id, token).unwrap_err()),
        "empty_request_not_found"
    );
    assert_eq!(env.emptier_calls().len(), 1);
}

#[test]
fn empty_should_report_partial_failed_and_unverified_results_with_their_log_rows() {
    let partial = Env::new(AfterEmpty::Leaves {
        items: 2,
        bytes: 40_000_000,
    });
    let (id, token) = partial.ready();
    let result = partial.empty(id, token).unwrap();
    assert_eq!(result.status, "partial");
    assert_eq!(result.items_after, Some(2));
    assert_eq!(result.freed_bytes, 300_000_000);
    assert_eq!(result.detail.as_deref(), Some("partial"));
    assert_eq!(partial.log()[0].outcome, CleanupOutcome::RecycleBinEmptied);

    let failed = Env::new(AfterEmpty::Unchanged);
    let (id, token) = failed.ready();
    let failed_env = Env {
        emptier: failed
            .bin
            .emptier(AfterEmpty::Unchanged)
            .failing_with(EmptyError::PermissionDenied),
        ..failed
    };
    let result = failed_env.empty(id, token).unwrap();
    assert_eq!(result.status, "failed");
    assert_eq!(result.freed_bytes, 0);
    assert_eq!(result.detail.as_deref(), Some("permission_denied"));
    let log = failed_env.log();
    assert_eq!(log[0].outcome, CleanupOutcome::Error);
    assert_eq!(log[0].size_bytes, 0);

    let unverified = Env::new(AfterEmpty::MeasureFails(
        crate::domain::ports::PortError::Io("x".into()),
    ));
    let (id, token) = unverified.ready();
    let result = unverified.empty(id, token).unwrap();
    assert_eq!(result.status, "unverified");
    assert_eq!(result.items_after, None);
    assert_eq!(result.bytes_after, None);
    assert_eq!(
        unverified.log()[0].error_detail.as_deref(),
        Some("unverified")
    );
}

// --- empty: rechazos ---

#[test]
fn empty_should_reject_each_invalid_step_without_calling_the_emptier_or_logging() {
    // Sin preparar.
    let env = Env::new(AfterEmpty::Empty);
    assert_eq!(
        kind(&env.empty(1, 1).unwrap_err()),
        "empty_request_not_found"
    );

    // Preparada pero sin reconocer.
    let request = env.prepare();
    assert_eq!(
        kind(&env.empty(safe(request.request_id), 1).unwrap_err()),
        "empty_not_acknowledged"
    );

    // Token equivocado.
    let (id, token) = env.ready();
    assert_eq!(
        kind(&env.empty(id, token + 1).unwrap_err()),
        "empty_request_not_found"
    );

    // Vencida.
    let (id, token) = env.ready();
    env.clock.0.set(T0 + TTL + 1);
    assert_eq!(
        kind(&env.empty(id, token).unwrap_err()),
        "empty_request_expired"
    );

    assert_eq!(env.emptier_calls(), Vec::<PathBuf>::new());
    assert!(env.log().is_empty());
}

#[test]
fn empty_should_reject_a_changed_bin_consuming_the_request_without_emptying() {
    for (items, bytes) in [(13, 340_000_000), (12, 340_000_001), (11, 340_000_000)] {
        let env = Env::new(AfterEmpty::Empty);
        let (id, token) = env.ready();
        env.bin.set_usage(items, bytes);
        assert_eq!(
            kind(&env.empty(id, token).unwrap_err()),
            "recycle_bin_changed",
            "({items}, {bytes})"
        );
        assert_eq!(env.emptier_calls(), Vec::<PathBuf>::new());
        assert!(env.log().is_empty());
        // La solicitud ya se consumio: hay que volver a preparar.
        assert_eq!(
            kind(&env.empty(id, token).unwrap_err()),
            "empty_request_not_found"
        );
    }
}

#[test]
fn empty_should_be_unavailable_when_the_remeasure_fails() {
    let env = Env::new(AfterEmpty::Empty);
    let (id, token) = env.ready();
    env.bin
        .set_failing(crate::domain::ports::PortError::Io("x".into()));
    assert_eq!(
        kind(&env.empty(id, token).unwrap_err()),
        "recycle_bin_unavailable"
    );
    assert_eq!(env.emptier_calls(), Vec::<PathBuf>::new());
}

#[test]
fn every_command_should_reject_ids_outside_the_js_safe_range_without_consuming() {
    let env = Env::new(AfterEmpty::Empty);
    let (id, token) = env.ready();
    let too_big = (1_i64 << 53) + 1;
    for bad in [-1, too_big, i64::MIN] {
        let ack = acknowledge_empty_run(&env.ports(), &matched(), bad).unwrap_err();
        assert_eq!(kind(&ack), "invalid_argument", "{bad}");
        assert_eq!(
            kind(&env.empty(bad, token).unwrap_err()),
            "invalid_argument",
            "{bad}"
        );
        assert_eq!(
            kind(&env.empty(id, bad).unwrap_err()),
            "invalid_argument",
            "{bad}"
        );
    }
    assert!(env.empty(id, token).is_ok(), "la solicitud sigue vigente");
}

#[test]
fn empty_should_reject_without_consuming_in_mismatch_or_while_another_job_runs() {
    let env = Env::new(AfterEmpty::Empty);
    let (id, token) = env.ready();
    for reason in [MismatchReason::DifferentUser, MismatchReason::Undetermined] {
        let error = empty_run(&env.ports(), &mismatched(reason), id, token).unwrap_err();
        assert_eq!(kind(&error), "recycle_bin_blocked");
    }
    let scan = env.active.begin_scan().unwrap();
    assert_eq!(kind(&env.empty(id, token).unwrap_err()), "scan_in_progress");
    drop(scan);
    let cleanup = env
        .active
        .begin_cleanup(crate::domain::cleanup::model::PreviewId(1))
        .unwrap();
    assert_eq!(
        kind(&env.empty(id, token).unwrap_err()),
        "cleanup_in_progress"
    );
    drop(cleanup);
    let other_empty = env.active.begin_empty().unwrap();
    assert_eq!(
        kind(&env.empty(id, token).unwrap_err()),
        "recycle_bin_empty_in_progress"
    );
    drop(other_empty);
    assert_eq!(env.emptier_calls(), Vec::<PathBuf>::new());
    // Nada se consumio: ahora si vacia.
    assert!(env.empty(id, token).is_ok());
}

// --- exclusividad durante el vaciado ---

struct ProbeEmptier<'a> {
    active: &'a ActiveJob,
    seen: std::cell::RefCell<Vec<String>>,
}

impl RecycleBinEmptier for ProbeEmptier<'_> {
    fn empty(&self, _root: &VolumeRoot) -> Result<(), EmptyError> {
        let mut seen = self.seen.borrow_mut();
        seen.push(kind(&self.active.ensure_idle().unwrap_err()));
        seen.push(kind(&self.active.begin_scan().unwrap_err()));
        seen.push(kind(
            &self
                .active
                .begin_cleanup(crate::domain::cleanup::model::PreviewId(1))
                .unwrap_err(),
        ));
        seen.push(kind(&self.active.begin_empty().unwrap_err()));
        Ok(())
    }
}

#[test]
fn empty_should_hold_the_active_slot_while_the_shell_runs() {
    let env = Env::new(AfterEmpty::Unchanged);
    let (id, token) = env.ready();
    let probe = ProbeEmptier {
        active: &env.active,
        seen: Default::default(),
    };
    let ports = EmptyPorts {
        store: &env.store,
        bin: &env.bin,
        emptier: &probe,
        clock: &env.clock,
        requests: &env.requests,
        active: &env.active,
    };
    empty_run(&ports, &matched(), id, token).unwrap();
    assert_eq!(
        *probe.seen.borrow(),
        vec!["recycle_bin_empty_in_progress"; 4]
    );
    assert!(env.active.ensure_idle().is_ok());
}

// --- registro que falla despues de vaciar ---

/// Un log que siempre falla al escribir.
struct FailingLog;

impl CleanupLogStore for FailingLog {
    fn append_cleanup_log(&self, _entries: &[NewCleanupLogEntry]) -> Result<(), StoreError> {
        Err(StoreError::Unavailable("disk full".into()))
    }

    fn recent_cleanup_log(&self, _limit: u32) -> Result<Vec<CleanupLogEntry>, StoreError> {
        Ok(Vec::new())
    }

    fn scan_cleaned_at(
        &self,
        _scan_id: crate::domain::model::ScanId,
        _since: EpochMillis,
    ) -> Result<Option<EpochMillis>, StoreError> {
        Ok(None)
    }
}

#[test]
fn empty_should_return_the_real_result_with_logged_false_when_the_append_fails() {
    let env = Env::new(AfterEmpty::Empty);
    let (id, token) = env.ready();
    let ports = EmptyPorts {
        store: &FailingLog,
        bin: &env.bin,
        emptier: &env.emptier,
        clock: &env.clock,
        requests: &env.requests,
        active: &env.active,
    };
    let result = empty_run(&ports, &matched(), id, token).unwrap();
    assert_eq!(result.status, "emptied");
    assert!(!result.logged);
    assert_eq!(result.freed_bytes, 340_000_000);
    assert_eq!(env.emptier_calls().len(), 1);
}

#[test]
fn dtos_should_never_serialize_paths_sids_or_accounts() {
    let env = Env::new(AfterEmpty::Empty);
    let request = env.prepare();
    let ack = env.acknowledge(request.request_id);
    let result = env
        .empty(safe(ack.request_id), safe(ack.confirm_token))
        .unwrap();
    for value in [
        serde_json::to_value(&request).unwrap(),
        serde_json::to_value(&ack).unwrap(),
        serde_json::to_value(&result).unwrap(),
    ] {
        let text = value.to_string();
        assert!(!text.contains("C:"), "{text}");
        assert!(!text.contains("S-1-"), "{text}");
        assert!(!matches!(&value, Value::Null));
    }
}

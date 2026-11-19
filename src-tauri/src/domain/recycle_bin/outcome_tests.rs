use super::*;

const BEFORE: RecycleBinUsage = RecycleBinUsage {
    items: 12,
    bytes: 340_000_000,
};

fn after(items: u64, bytes: u64) -> Result<RecycleBinUsage, PortError> {
    Ok(RecycleBinUsage { items, bytes })
}

fn measure_failed() -> Result<RecycleBinUsage, PortError> {
    Err(PortError::Io("measure".to_owned()))
}

struct Row {
    name: &'static str,
    call: Result<(), EmptyError>,
    after: Result<RecycleBinUsage, PortError>,
    expected: EmptyResult,
}

fn result(
    status: EmptyStatus,
    items_after: Option<u64>,
    bytes_after: Option<u64>,
    freed_bytes: u64,
    detail: Option<&str>,
) -> EmptyResult {
    EmptyResult {
        status,
        items_after,
        bytes_after,
        freed_bytes,
        detail: detail.map(str::to_owned),
    }
}

#[test]
fn classify_empty_should_follow_every_row_of_the_d08_table() {
    let rows = [
        Row {
            name: "vacia con Ok",
            call: Ok(()),
            after: after(0, 0),
            expected: result(EmptyStatus::Emptied, Some(0), Some(0), 340_000_000, None),
        },
        Row {
            name: "vacia aunque el shell devolvio E_UNEXPECTED",
            call: Err(EmptyError::Other("io_0x8000ffff".to_owned())),
            after: after(0, 0),
            expected: result(EmptyStatus::Emptied, Some(0), Some(0), 340_000_000, None),
        },
        Row {
            name: "parcial: 300 MB liberados y 2 restantes",
            call: Ok(()),
            after: after(2, 40_000_000),
            expected: result(
                EmptyStatus::Partial,
                Some(2),
                Some(40_000_000),
                300_000_000,
                Some("partial"),
            ),
        },
        Row {
            name: "parcial con error de la llamada",
            call: Err(EmptyError::PermissionDenied),
            after: after(11, 339_999_999),
            expected: result(
                EmptyStatus::Partial,
                Some(11),
                Some(339_999_999),
                1,
                Some("partial"),
            ),
        },
        Row {
            name: "sin cambios con Ok",
            call: Ok(()),
            after: after(12, 340_000_000),
            expected: result(
                EmptyStatus::Failed,
                Some(12),
                Some(340_000_000),
                0,
                Some("not_emptied"),
            ),
        },
        Row {
            name: "sin cambios con acceso denegado",
            call: Err(EmptyError::PermissionDenied),
            after: after(12, 340_000_000),
            expected: result(
                EmptyStatus::Failed,
                Some(12),
                Some(340_000_000),
                0,
                Some("permission_denied"),
            ),
        },
        Row {
            name: "crecio con Ok: liberado 0 saturado",
            call: Ok(()),
            after: after(13, 400_000_000),
            expected: result(
                EmptyStatus::Failed,
                Some(13),
                Some(400_000_000),
                0,
                Some("not_emptied"),
            ),
        },
        Row {
            name: "medicion posterior falla con Ok",
            call: Ok(()),
            after: measure_failed(),
            expected: result(
                EmptyStatus::Unverified,
                None,
                None,
                340_000_000,
                Some("unverified"),
            ),
        },
        Row {
            name: "medicion posterior falla y la llamada tambien",
            call: Err(EmptyError::BackendUnavailable),
            after: measure_failed(),
            expected: result(
                EmptyStatus::Failed,
                None,
                None,
                0,
                Some("empty_unavailable"),
            ),
        },
        Row {
            name: "mecanismo inutilizable sin cambios",
            call: Err(EmptyError::BackendUnavailable),
            after: after(12, 340_000_000),
            expected: result(
                EmptyStatus::Failed,
                Some(12),
                Some(340_000_000),
                0,
                Some("empty_unavailable"),
            ),
        },
    ];
    for row in rows {
        assert_eq!(
            classify_empty(BEFORE, row.call, row.after),
            row.expected,
            "{}",
            row.name
        );
    }
}

#[test]
fn status_should_expose_stable_codes() {
    assert_eq!(EmptyStatus::Emptied.as_str(), "emptied");
    assert_eq!(EmptyStatus::Partial.as_str(), "partial");
    assert_eq!(EmptyStatus::Failed.as_str(), "failed");
    assert_eq!(EmptyStatus::Unverified.as_str(), "unverified");
}

fn outcome(result: EmptyResult) -> EmptyOutcome {
    EmptyOutcome {
        root: VolumeRoot::from_drive_letter('C').unwrap(),
        before: BEFORE,
        result,
        finished_at: EpochMillis(1_800_000_000_500),
    }
}

fn expected_entry(
    outcome: CleanupOutcome,
    size_bytes: u64,
    detail: Option<&str>,
) -> NewCleanupLogEntry {
    NewCleanupLogEntry {
        ts: EpochMillis(1_800_000_000_500),
        category: "recycle_bin".to_owned(),
        path: "C:\\".to_owned(),
        size_bytes,
        outcome,
        scan_id: None,
        error_detail: detail.map(str::to_owned),
    }
}

#[test]
fn log_entry_should_record_each_status_with_the_d09_shape() {
    let cases = [
        (
            classify_empty(BEFORE, Ok(()), after(0, 0)),
            expected_entry(CleanupOutcome::RecycleBinEmptied, 340_000_000, None),
        ),
        (
            classify_empty(BEFORE, Ok(()), after(2, 40_000_000)),
            expected_entry(
                CleanupOutcome::RecycleBinEmptied,
                300_000_000,
                Some("partial"),
            ),
        ),
        (
            classify_empty(BEFORE, Ok(()), measure_failed()),
            expected_entry(
                CleanupOutcome::RecycleBinEmptied,
                340_000_000,
                Some("unverified"),
            ),
        ),
        (
            classify_empty(
                BEFORE,
                Err(EmptyError::PermissionDenied),
                after(12, 340_000_000),
            ),
            expected_entry(CleanupOutcome::Error, 0, Some("permission_denied")),
        ),
        (
            classify_empty(BEFORE, Ok(()), after(12, 340_000_000)),
            expected_entry(CleanupOutcome::Error, 0, Some("not_emptied")),
        ),
    ];
    for (result, expected) in cases {
        assert_eq!(outcome(result).log_entry(), expected);
    }
}

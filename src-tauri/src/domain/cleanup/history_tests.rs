use super::*;
use crate::domain::model::ScanId;
use crate::domain::model::{CleanupLogEntry, CleanupOutcome, EpochMillis};
use crate::domain::ports::CleanupRunKey;
use crate::domain::privileges::{MismatchReason, PrivilegeContext, UserMatch};

fn categories(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

#[test]
fn classify_should_call_a_run_of_whitelist_categories_junk() {
    let kind = RunKind::classify(
        &categories(&["browser_cache_edge", "temp_user"]),
        CleanupRunKey::Scan(ScanId(7)),
    );

    assert_eq!(kind, RunKind::Junk);
}

#[test]
fn classify_should_call_a_run_of_only_duplicates_duplicates() {
    let kind = RunKind::classify(&categories(&["duplicates"]), CleanupRunKey::Scan(ScanId(3)));

    assert_eq!(kind, RunKind::Duplicates);
}

#[test]
fn classify_should_call_a_single_row_without_scan_of_the_recycle_bin_an_empty_run() {
    let kind = RunKind::classify(&categories(&["recycle_bin"]), CleanupRunKey::Entry(12));

    assert_eq!(kind, RunKind::RecycleBinEmpty);
}

#[test]
fn classify_should_call_a_run_that_mixes_a_whitelist_category_with_an_unknown_one_other() {
    let kind = RunKind::classify(
        &categories(&["temp_user", "custom_rule"]),
        CleanupRunKey::Scan(ScanId(7)),
    );

    assert_eq!(kind, RunKind::Other);
}

#[test]
fn classify_should_call_a_scan_run_with_the_recycle_bin_category_junk() {
    let kind = RunKind::classify(
        &categories(&["recycle_bin"]),
        CleanupRunKey::Scan(ScanId(7)),
    );

    assert_eq!(kind, RunKind::Junk);
}

#[test]
fn classify_should_call_duplicates_mixed_with_junk_and_an_empty_category_list_other() {
    let mixed = RunKind::classify(
        &categories(&["duplicates", "temp_user"]),
        CleanupRunKey::Scan(ScanId(7)),
    );
    let empty = RunKind::classify(&[], CleanupRunKey::Scan(ScanId(7)));

    assert_eq!(mixed, RunKind::Other);
    assert_eq!(empty, RunKind::Other);
}

#[test]
fn as_str_should_give_the_stable_codes() {
    let codes: Vec<&str> = [
        RunKind::Junk,
        RunKind::Duplicates,
        RunKind::RecycleBinEmpty,
        RunKind::Other,
    ]
    .into_iter()
    .map(RunKind::as_str)
    .collect();

    assert_eq!(codes, ["junk", "duplicates", "recycle_bin_empty", "other"]);
}

fn log_row(id: i64, path: &str, size_bytes: u64, outcome: CleanupOutcome) -> CleanupLogEntry {
    CleanupLogEntry {
        id,
        ts: EpochMillis(1_000 + id),
        category: "temp_user".into(),
        path: path.into(),
        size_bytes,
        outcome,
        scan_id: Some(ScanId(7)),
        error_detail: (outcome == CleanupOutcome::Error).then(|| "access_denied".to_owned()),
    }
}

fn two_rows() -> Vec<CleanupLogEntry> {
    vec![
        log_row(
            1,
            r"C:\Users\ana\AppData\Local\Temp\a.tmp",
            100,
            CleanupOutcome::MovedToTrash,
        ),
        log_row(
            2,
            r"C:\Users\ana\AppData\Local\Temp\b.tmp",
            200,
            CleanupOutcome::Error,
        ),
    ]
}

#[test]
fn redact_paths_should_hide_every_path_for_a_different_user_and_keep_the_rest() {
    let context = PrivilegeContext::new(UserMatch::Mismatch(MismatchReason::DifferentUser), true);

    let (hidden, items) = redact_paths(&context, two_rows());

    assert!(hidden);
    assert_eq!(items.len(), 2);
    assert!(items.iter().all(|item| item.path.is_none()));
    assert_eq!(items[0].size_bytes, 100);
    assert_eq!(items[0].outcome, CleanupOutcome::MovedToTrash);
    assert_eq!(items[1].size_bytes, 200);
    assert_eq!(items[1].error_detail.as_deref(), Some("access_denied"));
}

#[test]
fn redact_paths_should_hide_paths_when_the_mismatch_is_undetermined() {
    let (hidden, items) = redact_paths(&PrivilegeContext::undetermined(), two_rows());

    assert!(hidden);
    assert!(items.iter().all(|item| item.path.is_none()));
}

#[test]
fn redact_paths_should_keep_paths_when_the_user_matches() {
    let context = PrivilegeContext::new(UserMatch::Match, true);

    let (hidden, items) = redact_paths(&context, two_rows());

    assert!(!hidden);
    assert_eq!(
        items[0].path.as_deref(),
        Some(r"C:\Users\ana\AppData\Local\Temp\a.tmp")
    );
    assert_eq!(
        items[1].path.as_deref(),
        Some(r"C:\Users\ana\AppData\Local\Temp\b.tmp")
    );
}

#[test]
fn redact_paths_should_report_hidden_even_for_an_empty_page() {
    let (hidden, items) = redact_paths(&PrivilegeContext::undetermined(), Vec::new());

    assert!(hidden);
    assert!(items.is_empty());
}

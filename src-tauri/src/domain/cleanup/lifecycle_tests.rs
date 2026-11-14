use super::*;
use crate::domain::cleanup::model::PREVIEW_TTL_MS;
use crate::domain::cleanup::preview::{PreviewInput, build_preview};
use crate::domain::junk::category::JunkCategory;
use crate::domain::junk::model::JunkScanSummary;
use crate::domain::model::{EpochMillis, Scan, ScanId, ScanKind, ScanStatus, ScanTrigger};
use crate::domain::privileges::{PrivilegeContext, UserMatch};

const CREATED: i64 = 1_000_000;

fn preview(id: u64) -> CleanupPreview {
    let summary = JunkScanSummary {
        scan: Scan {
            id: ScanId(1),
            kind: ScanKind::Junk,
            trigger: ScanTrigger::Manual,
            status: ScanStatus::Done,
            started_at: EpochMillis(1),
            finished_at: Some(EpochMillis(2)),
        },
        categories: vec![],
    };
    let privileges = PrivilegeContext::new(UserMatch::Match, false);
    match build_preview(PreviewInput {
        requested_scan: ScanId(1),
        last_done: Some(&summary),
        cleaned_at: None,
        categories: &[JunkCategory::TempUser],
        candidates: vec![],
        privileges: &privileges,
        now: EpochMillis(CREATED),
        id: PreviewId(id),
    }) {
        Ok(preview) => preview,
        Err(error) => panic!("preview de prueba: {error:?}"),
    }
}

fn at(offset: i64) -> EpochMillis {
    EpochMillis(CREATED + offset)
}

#[test]
fn take_should_return_the_stored_preview_within_its_lifetime() {
    let mut slot = PreviewSlot::new();
    slot.store(preview(1));
    let taken = slot.take(PreviewId(1), at(60_000)).unwrap();
    assert_eq!(taken.id(), PreviewId(1));
    assert_eq!(taken.scan_id(), ScanId(1));
}

#[test]
fn take_should_still_work_exactly_at_the_ttl_and_fail_one_millisecond_later() {
    let mut slot = PreviewSlot::new();
    slot.store(preview(1));
    assert!(slot.take(PreviewId(1), at(PREVIEW_TTL_MS)).is_ok());
    slot.store(preview(2));
    assert_eq!(
        slot.take(PreviewId(2), at(PREVIEW_TTL_MS + 1)).unwrap_err(),
        SlotError::PreviewExpired
    );
}

#[test]
fn take_should_consume_an_expired_preview() {
    let mut slot = PreviewSlot::new();
    slot.store(preview(1));
    let expired = at(11 * 60 * 1000);
    assert_eq!(
        slot.take(PreviewId(1), expired).unwrap_err(),
        SlotError::PreviewExpired
    );
    assert_eq!(
        slot.take(PreviewId(1), at(0)).unwrap_err(),
        SlotError::PreviewNotFound
    );
}

#[test]
fn take_should_fail_for_an_empty_slot() {
    let mut slot = PreviewSlot::new();
    assert_eq!(
        slot.take(PreviewId(1), at(0)).unwrap_err(),
        SlotError::PreviewNotFound
    );
}

#[test]
fn store_should_replace_the_previous_preview_and_invalidate_its_id() {
    let mut slot = PreviewSlot::new();
    slot.store(preview(1));
    slot.store(preview(2));
    assert_eq!(
        slot.take(PreviewId(1), at(0)).unwrap_err(),
        SlotError::PreviewNotFound
    );
    assert_eq!(slot.take(PreviewId(2), at(0)).unwrap().id(), PreviewId(2));
}

#[test]
fn take_should_not_consume_the_stored_preview_when_the_id_is_unknown() {
    let mut slot = PreviewSlot::new();
    slot.store(preview(4));
    assert_eq!(
        slot.take(PreviewId(99), at(0)).unwrap_err(),
        SlotError::PreviewNotFound
    );
    assert!(slot.take(PreviewId(4), at(0)).is_ok());
}

#[test]
fn take_should_never_return_the_same_preview_twice() {
    let mut slot = PreviewSlot::new();
    slot.store(preview(1));
    assert!(slot.take(PreviewId(1), at(0)).is_ok());
    assert_eq!(
        slot.take(PreviewId(1), at(0)).unwrap_err(),
        SlotError::PreviewNotFound
    );
}

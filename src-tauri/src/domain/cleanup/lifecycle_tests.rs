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

// ---- C-11: un solo preview activo de cualquier tipo ----

fn dup_preview(id: u64) -> DuplicateCleanupPreview {
    use crate::domain::duplicates::decision::Decision;
    use crate::domain::duplicates::model::{
        DuplicateFile, DuplicateScanSummary, MinSize, ScanCounters, StoredDuplicateFile,
        StoredDuplicateGroup,
    };
    use crate::domain::duplicates::preview::{DuplicatePreviewInput, build_duplicate_preview};
    use crate::domain::ports::FileIdentity;
    use crate::domain::protection::context::ContentFolder;

    let file = |file_id: i64| StoredDuplicateFile {
        id: file_id,
        file: DuplicateFile {
            root: ContentFolder::Downloads,
            path: format!(r"C:\Users\ana\Downloads\f{file_id}.bin"),
            modified: Some(EpochMillis(1)),
            identity: FileIdentity {
                volume_serial: 1,
                file_id: file_id as u128,
            },
        },
        kept: false,
    };
    let groups = [StoredDuplicateGroup {
        id: 4,
        size_bytes: 4 << 20,
        blake3: "ab".repeat(32),
        resolved_at: None,
        files: vec![file(41), file(42)],
    }];
    let summary = DuplicateScanSummary {
        scan: Scan {
            id: ScanId(1),
            kind: ScanKind::Duplicates,
            trigger: ScanTrigger::Manual,
            status: ScanStatus::Done,
            started_at: EpochMillis(1),
            finished_at: Some(EpochMillis(2)),
        },
        min_size: MinSize::MIB_1,
        roots: Vec::new(),
        counters: ScanCounters::default(),
        group_count: 1,
        duplicate_file_count: 2,
        reclaimable_bytes: 0,
        open_group_count: 1,
    };
    build_duplicate_preview(DuplicatePreviewInput {
        requested_scan: ScanId(1),
        last_done: Some(&summary),
        decisions: &[Decision {
            group_id: 4,
            keep_file_ids: vec![41],
        }],
        groups: &groups,
        now: EpochMillis(CREATED),
        id: PreviewId(id),
    })
    .unwrap()
}

#[test]
fn take_duplicates_should_return_the_stored_duplicates_preview() {
    let mut slot = PreviewSlot::new();
    slot.store_duplicates(dup_preview(5));
    let taken = slot.take_duplicates(PreviewId(5), at(60_000)).unwrap();
    assert_eq!(taken.preview().id(), PreviewId(5));
    assert_eq!(taken.group_count(), 1);
    // De un solo uso.
    assert_eq!(
        slot.take_duplicates(PreviewId(5), at(0)).unwrap_err(),
        SlotError::PreviewNotFound
    );
}

#[test]
fn storing_one_kind_should_replace_the_other_and_invalidate_its_id() {
    let mut slot = PreviewSlot::new();
    slot.store(preview(1));
    slot.store_duplicates(dup_preview(2));
    assert_eq!(
        slot.take(PreviewId(1), at(0)).unwrap_err(),
        SlotError::PreviewNotFound
    );
    assert!(slot.take_duplicates(PreviewId(2), at(0)).is_ok());

    slot.store_duplicates(dup_preview(3));
    slot.store(preview(4));
    assert_eq!(
        slot.take_duplicates(PreviewId(3), at(0)).unwrap_err(),
        SlotError::PreviewNotFound
    );
    assert!(slot.take(PreviewId(4), at(0)).is_ok());
}

#[test]
fn taking_with_the_wrong_kind_should_fail_and_keep_the_preview() {
    let mut slot = PreviewSlot::new();
    slot.store_duplicates(dup_preview(7));
    assert_eq!(
        slot.take(PreviewId(7), at(0)).unwrap_err(),
        SlotError::PreviewNotFound
    );
    // El preview de duplicados sigue disponible.
    assert!(slot.take_duplicates(PreviewId(7), at(0)).is_ok());

    slot.store(preview(8));
    assert_eq!(
        slot.take_duplicates(PreviewId(8), at(0)).unwrap_err(),
        SlotError::PreviewNotFound
    );
    assert!(slot.take(PreviewId(8), at(0)).is_ok());
}

#[test]
fn a_duplicates_preview_should_expire_after_10_minutes_like_the_junk_one() {
    let mut slot = PreviewSlot::new();
    slot.store_duplicates(dup_preview(1));
    assert!(
        slot.take_duplicates(PreviewId(1), at(PREVIEW_TTL_MS))
            .is_ok()
    );
    slot.store_duplicates(dup_preview(2));
    assert_eq!(
        slot.take_duplicates(PreviewId(2), at(PREVIEW_TTL_MS + 1))
            .unwrap_err(),
        SlotError::PreviewExpired
    );
    assert_eq!(
        slot.take_duplicates(PreviewId(2), at(0)).unwrap_err(),
        SlotError::PreviewNotFound,
        "el vencido queda consumido"
    );
}

use std::collections::BTreeSet;

use super::*;
use crate::domain::duplicates::model::{DuplicateFile, MinSize, ScanCounters, StoredDuplicateFile};
use crate::domain::model::{Scan, ScanKind, ScanStatus, ScanTrigger};
use crate::domain::protection::context::ContentFolder;

const NOW: EpochMillis = EpochMillis(1_800_000_000_000);
const HOUR: i64 = 3_600_000;
const SCAN: ScanId = ScanId(7);
const MIB: u64 = 1 << 20;

fn ago(hours: i64) -> Option<EpochMillis> {
    Some(EpochMillis(NOW.0 - hours * HOUR))
}

fn summary(scan: ScanId) -> DuplicateScanSummary {
    DuplicateScanSummary {
        scan: Scan {
            id: scan,
            kind: ScanKind::Duplicates,
            trigger: ScanTrigger::Manual,
            status: ScanStatus::Done,
            started_at: EpochMillis(1),
            finished_at: Some(EpochMillis(2)),
        },
        min_size: MinSize::MIB_1,
        roots: Vec::new(),
        counters: ScanCounters::default(),
        group_count: 3,
        duplicate_file_count: 7,
        reclaimable_bytes: 0,
        open_group_count: 3,
    }
}

fn file(id: i64, name: &str, hours_old: i64) -> StoredDuplicateFile {
    StoredDuplicateFile {
        id,
        file: DuplicateFile {
            root: ContentFolder::Downloads,
            path: format!(r"C:\Users\ana\Downloads\{name}"),
            modified: ago(hours_old),
            identity: FileIdentity {
                volume_serial: 1,
                file_id: id as u128,
            },
        },
        kept: false,
    }
}

fn group(id: i64, size: u64, files: Vec<StoredDuplicateFile>) -> StoredDuplicateGroup {
    StoredDuplicateGroup {
        id,
        size_bytes: size,
        blake3: format!("{id:02x}").repeat(32),
        resolved_at: None,
        files,
    }
}

fn groups() -> Vec<StoredDuplicateGroup> {
    vec![
        group(
            4,
            4 * MIB,
            vec![file(41, "a1.bin", 72), file(42, "a2.bin", 72)],
        ),
        group(
            9,
            10 * MIB,
            vec![
                file(91, "b1.bin", 72),
                file(92, "b2.bin", 72),
                file(93, "b3.bin", 72),
            ],
        ),
    ]
}

fn decide(group_id: i64, keep: &[i64]) -> Decision {
    Decision {
        group_id,
        keep_file_ids: keep.to_vec(),
    }
}

fn build(
    decisions: &[Decision],
    groups: &[StoredDuplicateGroup],
    last: Option<&DuplicateScanSummary>,
) -> Result<DuplicateCleanupPreview, DuplicatePreviewError> {
    build_duplicate_preview(DuplicatePreviewInput {
        requested_scan: SCAN,
        last_done: last,
        decisions,
        groups,
        now: NOW,
        id: PreviewId(3),
    })
}

fn ok(decisions: &[Decision], groups: &[StoredDuplicateGroup]) -> DuplicateCleanupPreview {
    build(decisions, groups, Some(&summary(SCAN))).unwrap()
}

#[test]
fn build_should_create_one_item_per_copy_to_delete_with_the_persisted_row() {
    let groups = groups();
    let preview = ok(&[decide(4, &[41]), decide(9, &[92])], &groups);
    let core = preview.preview();
    assert_eq!(core.id(), PreviewId(3));
    assert_eq!(core.scan_id(), SCAN);
    assert_eq!(core.created_at(), NOW);
    let items = core.items();
    assert_eq!(
        items.iter().map(|i| i.finding_id).collect::<Vec<_>>(),
        [42, 91, 93]
    );
    let first = &items[0];
    assert_eq!(first.category, CleanupCategory::Duplicates);
    assert_eq!(first.path, r"C:\Users\ana\Downloads\a2.bin");
    assert_eq!(first.size_bytes, 4 * MIB);
    assert_eq!(first.modified_at, ago(72));
    assert_eq!(first.identity.file_id, 42);
}

#[test]
fn build_should_total_groups_copies_and_bytes() {
    let groups = groups();
    // Un grupo con una copia a eliminar de 4 MiB y otro con dos de 10 MiB.
    let preview = ok(&[decide(4, &[41]), decide(9, &[92])], &groups);
    assert_eq!(preview.group_count(), 2);
    assert_eq!(preview.preview().total_items(), 3);
    assert_eq!(preview.preview().total_bytes(), 24 * MIB);
    let [category] = preview.preview().categories() else {
        panic!("una sola categoria");
    };
    assert_eq!(category.category, CleanupCategory::Duplicates);
    assert_eq!(category.status, PreviewCategoryStatus::Included);
    assert_eq!((category.recent_count, category.recent_bytes), (0, 0));
}

#[test]
fn build_should_include_one_item_when_keeping_two_of_three() {
    let groups = groups();
    let preview = ok(&[decide(9, &[91, 93])], &groups);
    assert_eq!(
        preview
            .preview()
            .items()
            .iter()
            .map(|i| i.finding_id)
            .collect::<Vec<_>>(),
        [92]
    );
}

#[test]
fn build_should_order_items_by_decision_then_path() {
    let groups = groups();
    let by_decision = ok(&[decide(9, &[93]), decide(4, &[42])], &groups);
    assert_eq!(
        by_decision
            .preview()
            .items()
            .iter()
            .map(|i| i.finding_id)
            .collect::<Vec<_>>(),
        [91, 92, 41]
    );
}

#[test]
fn build_should_exclude_a_copy_written_within_24_hours_and_report_it() {
    let mut groups = groups();
    groups[0].files[1] = file(42, "a2.bin", 2);
    groups[1].files[2] = file(93, "b3.bin", 23);
    let preview = ok(&[decide(4, &[41]), decide(9, &[91])], &groups);
    // 42 (2 h) y 93 (23 h) son recientes; solo 92 se mueve.
    assert_eq!(
        preview
            .preview()
            .items()
            .iter()
            .map(|i| i.finding_id)
            .collect::<Vec<_>>(),
        [92]
    );
    let category = &preview.preview().categories()[0];
    assert_eq!(category.item_count, 1);
    assert_eq!(category.size_bytes, 10 * MIB);
    assert_eq!(category.recent_count, 2);
    assert_eq!(category.recent_bytes, 14 * MIB);
    // El grupo 4 sigue en el preview aunque no tenga ningun item.
    assert_eq!(preview.group_count(), 2);
    assert!(preview.plan().groups[0].item_file_ids.is_empty());
}

#[test]
fn build_should_treat_a_copy_without_a_date_as_recent() {
    let mut groups = groups();
    groups[0].files[1].file.modified = None;
    let preview = ok(&[decide(4, &[41])], &groups);
    assert!(preview.preview().items().is_empty());
    assert_eq!(preview.preview().categories()[0].recent_count, 1);
}

#[test]
fn build_should_fail_when_the_scan_is_not_the_last_done_duplicates_scan() {
    let groups = groups();
    let decisions = [decide(4, &[41])];
    assert_eq!(
        build(&decisions, &groups, None).err(),
        Some(DuplicatePreviewError::ScanNotFound)
    );
    assert_eq!(
        build(&decisions, &groups, Some(&summary(ScanId(8)))).err(),
        Some(DuplicatePreviewError::ScanNotFound)
    );
}

#[test]
fn build_should_reject_an_attempt_to_delete_every_copy() {
    let groups = groups();
    assert_eq!(
        build(&[decide(9, &[])], &groups, Some(&summary(SCAN))).err(),
        Some(DuplicatePreviewError::KeepRequired)
    );
}

#[test]
fn build_should_map_the_decision_errors() {
    let groups = groups();
    let last = summary(SCAN);
    // Conservar todas, una copia ajena, un grupo repetido y una lista vacia.
    for (decisions, label) in [
        (vec![decide(4, &[41, 42])], "conservar todas"),
        (vec![decide(4, &[91])], "copia ajena"),
        (vec![decide(4, &[41]), decide(4, &[42])], "grupo repetido"),
        (vec![], "decisiones vacias"),
    ] {
        assert!(
            matches!(
                build(&decisions, &groups, Some(&last)),
                Err(DuplicatePreviewError::InvalidArgument(_))
            ),
            "{label}"
        );
    }
    assert_eq!(
        build(&[decide(99, &[41])], &groups, Some(&last)).err(),
        Some(DuplicatePreviewError::GroupNotFound)
    );
    let mut resolved = groups.clone();
    resolved[0].resolved_at = Some(EpochMillis(5_000));
    assert_eq!(
        build(&[decide(4, &[41])], &resolved, Some(&last)).err(),
        Some(DuplicatePreviewError::GroupResolved)
    );
}

#[test]
fn plan_should_describe_what_the_guard_must_verify() {
    let groups = groups();
    let preview = ok(&[decide(9, &[91, 93])], &groups);
    let [plan] = preview.plan().groups.as_slice() else {
        panic!("un grupo");
    };
    assert_eq!(plan.group_id, 9);
    assert_eq!(plan.blake3, "09".repeat(32));
    assert_eq!(plan.size_bytes, 10 * MIB);
    assert_eq!(plan.kept_file_ids, [91, 93]);
    assert_eq!(plan.item_file_ids, [92]);
    assert_eq!(
        plan.kept
            .iter()
            .map(|k| k.path.as_str())
            .collect::<Vec<_>>(),
        [
            r"C:\Users\ana\Downloads\b1.bin",
            r"C:\Users\ana\Downloads\b3.bin"
        ]
    );
    assert_eq!(plan.kept[0].identity.file_id, 91);
    assert_eq!(plan.kept[0].modified, ago(72));
    assert_eq!(plan.kept[0].size_bytes, 10 * MIB);
}

#[test]
fn resolutions_should_cover_only_the_groups_with_a_processed_item() {
    let groups = groups();
    let preview = ok(&[decide(4, &[41]), decide(9, &[92])], &groups);
    let plan = preview.plan();
    let processed = |ids: &[i64]| ids.iter().copied().collect::<BTreeSet<i64>>();

    let (all, ids) = plan.resolutions_for(&processed(&[42, 91, 93]));
    assert_eq!(ids, [4, 9]);
    assert_eq!(
        all,
        [
            GroupResolution {
                group_id: 4,
                kept_file_ids: vec![41]
            },
            GroupResolution {
                group_id: 9,
                kept_file_ids: vec![92]
            }
        ]
    );
    // Cancelacion antes del segundo grupo: solo el primero queda resuelto.
    let (first, ids) = plan.resolutions_for(&processed(&[42]));
    assert_eq!(ids, [4]);
    assert_eq!(first.len(), 1);
    // Un solo item procesado de un grupo con dos basta para resolverlo.
    let (partial, _) = plan.resolutions_for(&processed(&[93]));
    assert_eq!(partial[0].group_id, 9);
    // Nada procesado: nada se resuelve.
    assert_eq!(plan.resolutions_for(&processed(&[])), (vec![], vec![]));
}

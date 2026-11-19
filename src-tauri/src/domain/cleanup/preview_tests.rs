use super::*;
use crate::domain::cleanup::model::PreviewCategoryStatus;
use crate::domain::junk::category::JunkCategory as C;
use crate::domain::junk::model::{FindingState, JunkFinding, JunkScanSummary, StoredFinding};
use crate::domain::model::{EpochMillis, Scan, ScanId, ScanKind, ScanStatus, ScanTrigger};
use crate::domain::ports::FileIdentity;
use crate::domain::privileges::{MismatchReason, PrivilegeContext, UserMatch};

const HOUR: i64 = 3_600_000;
const NOW: i64 = 100 * 24 * HOUR;
const SCAN: ScanId = ScanId(7);

fn matched() -> PrivilegeContext {
    PrivilegeContext::new(UserMatch::Match, false)
}

fn mismatch(reason: MismatchReason) -> PrivilegeContext {
    PrivilegeContext::new(UserMatch::Mismatch(reason), false)
}

fn summary(id: ScanId) -> JunkScanSummary {
    JunkScanSummary {
        scan: Scan {
            id,
            kind: ScanKind::Junk,
            trigger: ScanTrigger::Manual,
            status: ScanStatus::Done,
            started_at: EpochMillis(1_000),
            finished_at: Some(EpochMillis(2_000)),
        },
        categories: vec![],
    }
}

fn stored(id: i64, category: C, path: &str, size: u64, age_hours: Option<i64>) -> StoredFinding {
    StoredFinding {
        id,
        finding: JunkFinding {
            category,
            path: path.to_string(),
            size_bytes: size,
            modified_at: age_hours.map(|h| EpochMillis(NOW - h * HOUR)),
            state: FindingState::Eligible(FileIdentity {
                volume_serial: 0x8000_0000_dead_beef,
                file_id: u128::from(id as u64) | (1u128 << 100),
            }),
        },
    }
}

fn old(id: i64, category: C, path: &str, size: u64) -> StoredFinding {
    stored(id, category, path, size, Some(72))
}

struct Case {
    last_done: Option<JunkScanSummary>,
    requested: ScanId,
    cleaned_at: Option<EpochMillis>,
    categories: Vec<C>,
    candidates: Vec<StoredFinding>,
    privileges: PrivilegeContext,
    now: i64,
}

impl Case {
    fn new(categories: &[C], candidates: Vec<StoredFinding>) -> Self {
        Self {
            last_done: Some(summary(SCAN)),
            requested: SCAN,
            cleaned_at: None,
            categories: categories.to_vec(),
            candidates,
            privileges: matched(),
            now: NOW,
        }
    }

    fn build(&self) -> Result<CleanupPreview, PreviewError> {
        build_preview(PreviewInput {
            requested_scan: self.requested,
            last_done: self.last_done.as_ref(),
            cleaned_at: self.cleaned_at,
            categories: &self.categories,
            candidates: self.candidates.clone(),
            privileges: &self.privileges,
            now: EpochMillis(self.now),
            id: PreviewId(5),
        })
    }

    fn ok(&self) -> CleanupPreview {
        match self.build() {
            Ok(preview) => preview,
            Err(error) => panic!("se esperaba un preview: {error:?}"),
        }
    }
}

fn category_of(preview: &CleanupPreview, category: C) -> &PreviewCategory {
    preview
        .categories()
        .iter()
        .find(|c| c.category == category)
        .unwrap_or_else(|| panic!("falta {category:?}"))
}

// ---- totales y seleccion ----

#[test]
fn build_preview_should_total_the_eligible_items_of_the_requested_category() {
    let case = Case::new(
        &[C::TempUser],
        vec![
            old(1, C::TempUser, r"C:\t\a", 100),
            old(2, C::TempUser, r"C:\t\b", 200),
            old(3, C::TempUser, r"C:\t\c", 300),
            // Un omitido nunca entra, aunque llegara en la lista.
            StoredFinding {
                id: 4,
                finding: JunkFinding {
                    category: C::TempUser,
                    path: r"C:\t\skipped".into(),
                    size_bytes: 999,
                    modified_at: None,
                    state: FindingState::Skipped("reparse_point".into()),
                },
            },
        ],
    );
    let preview = case.ok();
    let temp = category_of(&preview, C::TempUser);
    assert_eq!((temp.item_count, temp.size_bytes), (3, 600));
    assert_eq!((temp.recent_count, temp.recent_bytes), (0, 0));
    assert_eq!(temp.status, PreviewCategoryStatus::Included);
    assert_eq!((preview.total_items(), preview.total_bytes()), (3, 600));
    assert_eq!(preview.items().len(), 3);
    assert_eq!(preview.scan_id(), SCAN);
    assert_eq!(preview.id(), PreviewId(5));
}

#[test]
fn build_preview_should_only_include_the_requested_categories_in_whitelist_order() {
    let case = Case::new(
        &[C::WuCache, C::TempUser],
        vec![
            old(1, C::TempUser, r"C:\t\a", 10),
            old(2, C::BrowserCacheChrome, r"C:\c\a", 20),
            old(3, C::WuCache, r"C:\w\a", 30),
        ],
    );
    let preview = case.ok();
    let listed: Vec<C> = preview.categories().iter().map(|c| c.category).collect();
    assert_eq!(listed, vec![C::TempUser, C::WuCache]);
    let paths: Vec<&str> = preview.items().iter().map(|i| i.path.as_str()).collect();
    assert_eq!(paths, vec![r"C:\t\a", r"C:\w\a"]);
    assert_eq!((preview.total_items(), preview.total_bytes()), (2, 40));
}

#[test]
fn build_preview_should_keep_the_identity_date_and_size_of_each_item() {
    let case = Case::new(&[C::TempUser], vec![old(9, C::TempUser, r"C:\t\a", 42)]);
    let preview = case.ok();
    let item = &preview.items()[0];
    assert_eq!(item.finding_id, 9);
    assert_eq!(item.size_bytes, 42);
    assert_eq!(item.modified_at, Some(EpochMillis(NOW - 72 * HOUR)));
    assert_eq!(
        item.identity,
        FileIdentity {
            volume_serial: 0x8000_0000_dead_beef,
            file_id: 9 | (1u128 << 100),
        }
    );
}

#[test]
fn build_preview_should_allow_an_empty_preview_with_zero_totals() {
    let case = Case::new(&[C::TempUser, C::WuCache], vec![]);
    let preview = case.ok();
    assert!(preview.items().is_empty());
    assert_eq!((preview.total_items(), preview.total_bytes()), (0, 0));
    assert_eq!(preview.categories().len(), 2);
}

#[test]
fn build_preview_should_expire_ten_minutes_after_creation() {
    let preview = Case::new(&[C::TempUser], vec![]).ok();
    assert_eq!(preview.created_at(), EpochMillis(NOW));
    assert_eq!(preview.expires_at(), EpochMillis(NOW + 10 * 60 * 1000));
}

// ---- guarda de escritura reciente (D-04) ----

#[test]
fn build_preview_should_exclude_a_file_written_23_hours_59_minutes_ago_as_recent() {
    let mut recent = stored(2, C::WuCache, r"C:\w\recent", 70, None);
    recent.finding.modified_at = Some(EpochMillis(NOW - 24 * HOUR + 60_000));
    let case = Case::new(
        &[C::WuCache],
        vec![old(1, C::WuCache, r"C:\w\old", 30), recent],
    );
    let preview = case.ok();
    let wu = category_of(&preview, C::WuCache);
    assert_eq!((wu.item_count, wu.size_bytes), (1, 30));
    assert_eq!((wu.recent_count, wu.recent_bytes), (1, 70));
    assert_eq!(preview.items().len(), 1);
}

#[test]
fn build_preview_should_include_a_file_written_exactly_24_hours_ago() {
    let case = Case::new(
        &[C::WuCache],
        vec![stored(1, C::WuCache, r"C:\w\edge", 5, Some(24))],
    );
    let preview = case.ok();
    let wu = category_of(&preview, C::WuCache);
    assert_eq!((wu.item_count, wu.recent_count), (1, 0));
}

#[test]
fn build_preview_should_exclude_a_file_without_date_as_recent() {
    let case = Case::new(
        &[C::TempUser],
        vec![
            stored(1, C::TempUser, r"C:\t\nodate", 11, None),
            old(2, C::TempUser, r"C:\t\ok", 4),
        ],
    );
    let preview = case.ok();
    let temp = category_of(&preview, C::TempUser);
    assert_eq!((temp.item_count, temp.size_bytes), (1, 4));
    assert_eq!((temp.recent_count, temp.recent_bytes), (1, 11));
}

#[test]
fn build_preview_should_treat_a_future_date_as_recent() {
    let case = Case::new(
        &[C::TempUser],
        vec![stored(1, C::TempUser, r"C:\t\future", 3, Some(-5))],
    );
    let preview = case.ok();
    assert_eq!(category_of(&preview, C::TempUser).recent_count, 1);
}

#[test]
fn build_preview_should_apply_the_recent_guard_to_every_category() {
    let candidates: Vec<StoredFinding> = [
        C::TempUser,
        C::TempWindows,
        C::BrowserCacheChrome,
        C::WuCache,
        C::ThumbnailCache,
        C::CrashDumps,
    ]
    .into_iter()
    .enumerate()
    .map(|(n, category)| stored(n as i64 + 1, category, &format!(r"C:\x\{n}"), 1, Some(2)))
    .collect();
    let categories: Vec<C> = candidates.iter().map(|c| c.finding.category).collect();
    let preview = Case::new(&categories, candidates).ok();
    assert_eq!(preview.total_items(), 0);
    assert!(preview.categories().iter().all(|c| c.recent_count == 1));
}

// ---- privilegios (barrera 1) ----

#[test]
fn build_preview_should_block_per_user_categories_on_mismatch_and_keep_machine_ones() {
    for reason in [MismatchReason::DifferentUser, MismatchReason::Undetermined] {
        let mut case = Case::new(
            &[C::TempUser, C::TempWindows],
            vec![
                old(1, C::TempUser, r"C:\Users\ana\Temp\a", 10),
                old(2, C::TempWindows, r"C:\Windows\Temp\a", 20),
            ],
        );
        case.privileges = mismatch(reason);
        let preview = case.ok();
        let user = category_of(&preview, C::TempUser);
        assert_eq!(user.status, PreviewCategoryStatus::Blocked(reason));
        assert_eq!((user.item_count, user.size_bytes), (0, 0));
        assert_eq!(
            category_of(&preview, C::TempWindows).status,
            PreviewCategoryStatus::Included
        );
        assert_eq!((preview.total_items(), preview.total_bytes()), (1, 20));
        assert!(preview.items().iter().all(|i| i.category == C::TempWindows));
    }
}

#[test]
fn build_preview_should_keep_the_mixed_crash_dumps_category_included_on_mismatch() {
    let mut case = Case::new(
        &[C::CrashDumps],
        vec![old(1, C::CrashDumps, r"C:\Windows\Minidump\a.dmp", 8)],
    );
    case.privileges = mismatch(MismatchReason::DifferentUser);
    let preview = case.ok();
    assert_eq!(
        category_of(&preview, C::CrashDumps).status,
        PreviewCategoryStatus::Included
    );
    assert_eq!(preview.total_items(), 1);
}

#[test]
fn build_preview_should_not_count_recent_items_of_a_blocked_category() {
    let mut case = Case::new(
        &[C::TempUser],
        vec![stored(1, C::TempUser, r"C:\t\a", 10, Some(1))],
    );
    case.privileges = mismatch(MismatchReason::Undetermined);
    let preview = case.ok();
    let user = category_of(&preview, C::TempUser);
    assert_eq!((user.recent_count, user.recent_bytes), (0, 0));
}

// ---- errores ----

#[test]
fn build_preview_should_fail_with_scan_not_found_unless_it_is_the_last_done_scan() {
    let mut other = Case::new(&[C::TempUser], vec![]);
    other.requested = ScanId(3);
    assert_eq!(other.build().unwrap_err(), PreviewError::ScanNotFound);
    let mut none = Case::new(&[C::TempUser], vec![]);
    none.last_done = None;
    assert_eq!(none.build().unwrap_err(), PreviewError::ScanNotFound);
}

#[test]
fn build_preview_should_fail_with_scan_already_cleaned_when_the_log_says_so() {
    let mut case = Case::new(&[C::TempUser], vec![]);
    case.cleaned_at = Some(EpochMillis(NOW - 1));
    assert_eq!(case.build().unwrap_err(), PreviewError::ScanAlreadyCleaned);
}

#[test]
fn build_preview_should_reject_invalid_category_lists() {
    let invalid: Vec<Vec<C>> = vec![
        vec![],
        vec![C::TempUser, C::TempUser],
        vec![C::RecycleBin],
        vec![C::TempUser, C::OldLogs],
    ];
    for categories in invalid {
        let case = Case::new(&categories, vec![]);
        assert!(
            matches!(case.build(), Err(PreviewError::InvalidArgument(_))),
            "{categories:?}"
        );
    }
}

#[test]
fn validate_categories_should_accept_every_cleanable_category_once() {
    let cleanable: Vec<C> = C::ALL
        .into_iter()
        .filter(|c| !matches!(c, C::RecycleBin | C::OldLogs))
        .collect();
    assert_eq!(cleanable.len(), 9);
    assert_eq!(validate_categories(&cleanable), Ok(()));
    assert!(validate_categories(&[]).is_err());
}

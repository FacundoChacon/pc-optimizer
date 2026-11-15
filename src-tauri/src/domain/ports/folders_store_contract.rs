//! Bateria de contrato de `FolderStore` (solo tests): corre contra `InMemoryStore` y contra
//! `SqliteStore` sobre un archivo temporal.

use crate::domain::disk_usage::model::{
    Access, FolderCounters, FolderIssue, FolderScanResults, FolderSort, IssueReason, LargeFileSort,
    OpaqueReason, RootKind, StoredFolder, StoredLargeFile,
};
use crate::domain::duplicates::model::DuplicateScanResults;
use crate::domain::model::{
    EpochMillis, NewScan, SCAN_RETENTION, ScanId, ScanKind, ScanStatus, ScanTrigger,
};
use crate::domain::ports::VolumeSpaceInfo;
use crate::domain::ports::duplicates_store_contract as dup;
use crate::domain::ports::store::{Store, StoreError};

pub(crate) fn folder_scan<S: Store>(store: &S, started: i64) -> ScanId {
    store
        .create_scan(&NewScan {
            kind: ScanKind::Folders,
            trigger: ScanTrigger::Manual,
            started_at: EpochMillis(started),
        })
        .unwrap()
}

fn folder(parent: Option<usize>, path: &str, depth: u32, size: u64, direct: u64) -> StoredFolder {
    StoredFolder {
        parent,
        path: path.to_string(),
        depth,
        size_bytes: size,
        direct_bytes: direct,
        file_count: 3,
        dir_count: 1,
        access: Access::Complete,
        opaque: None,
        system: false,
        junk_hint: false,
    }
}

fn large(folder: usize, path: &str, size: u64, modified: Option<i64>) -> StoredLargeFile {
    StoredLargeFile {
        folder,
        path: path.to_string(),
        size_bytes: size,
        modified_at: modified.map(EpochMillis),
    }
}

/// Raiz `C:\` (100, 5 directos) con `Users` (60) -> `ana` (40) y `bob` (20, opaca), y `Windows` (30).
pub(crate) fn sample() -> FolderScanResults {
    let mut windows = folder(Some(0), r"C:\Windows", 1, 30, 30);
    windows.system = true;
    let mut bob = folder(Some(1), r"C:\Users\bob", 2, 20, 20);
    bob.opaque = Some(OpaqueReason::OtherUserProfile);
    bob.access = Access::Incomplete;
    FolderScanResults {
        root_kind: RootKind::SystemDrive,
        root_path: r"C:\".into(),
        volume: Some(VolumeSpaceInfo {
            total_bytes: 500,
            free_bytes: 120,
        }),
        counters: FolderCounters {
            total_bytes: 100,
            file_count: 9,
            dir_count: 4,
            inaccessible_dirs: 2,
            reparse_skipped: 1,
            cloud_files: 3,
            opaque_dirs: 1,
            folders_stored: 5,
        },
        folders_truncated: false,
        folders: vec![
            folder(None, r"C:\", 0, 100, 5),
            folder(Some(0), r"C:\Users", 1, 60, 0),
            folder(Some(1), r"C:\Users\ana", 2, 40, 40),
            bob,
            windows,
        ],
        large_files: vec![
            large(4, r"C:\Windows\big.dll", 30, Some(300)),
            large(2, r"C:\Users\ana\a.bin", 25, Some(100)),
            large(2, r"C:\Users\ana\b.bin", 5, None),
        ],
        issues: vec![FolderIssue {
            path: r"C:\Windows\Secret".into(),
            reason: IssueReason::AccessDenied,
        }],
    }
}

fn save<S: Store>(store: &S, scan: ScanId, results: &FolderScanResults) {
    store
        .complete_folder_scan(scan, results, EpochMillis(9_000))
        .unwrap();
}

fn child_paths<S: Store>(
    store: &S,
    scan: ScanId,
    folder: Option<i64>,
    sort: FolderSort,
) -> Vec<String> {
    store
        .list_folder_children(scan, folder, sort, 0, 200)
        .unwrap()
        .items
        .iter()
        .map(|row| row.path.clone())
        .collect()
}

pub(crate) fn contract_round_trip<S: Store>(make: impl Fn() -> S) {
    let store = make();
    assert_eq!(store.last_done_folder_scan().unwrap(), None);
    let scan = folder_scan(&store, 1_000);
    save(&store, scan, &sample());
    let summary = store.last_done_folder_scan().unwrap().unwrap();
    assert_eq!(summary.scan.id, scan);
    assert_eq!(summary.scan.status, ScanStatus::Done);
    assert_eq!(summary.scan.finished_at, Some(EpochMillis(9_000)));
    assert_eq!(summary.root_kind, RootKind::SystemDrive);
    assert_eq!(summary.root_path, r"C:\");
    assert_eq!(summary.volume, sample().volume);
    assert_eq!(summary.counters, sample().counters);
    assert!(!summary.folders_truncated);
    assert_eq!(summary.issues, sample().issues);
    let root_id = summary.root_folder_id.expect("root folder id");

    let page = store
        .list_folder_children(scan, None, FolderSort::Size, 0, 50)
        .unwrap();
    assert_eq!(page.folder.id, root_id);
    assert_eq!(page.folder.path, r"C:\");
    assert_eq!(page.folder.depth, 0);
    assert_eq!(page.folder.parent_id, None);
    assert_eq!(page.total, 2);
    let paths: Vec<&str> = page.items.iter().map(|r| r.path.as_str()).collect();
    assert_eq!(paths, vec![r"C:\Users", r"C:\Windows"]);
    // resto = 100 - 5 directos - (60 + 30) hijos guardados
    assert_eq!(page.remainder_bytes, 5);
    assert!(page.items[0].has_children);
    assert!(page.items[1].system && !page.items[1].has_children);
    assert_eq!(page.breadcrumb.len(), 1);
    assert_eq!(page.breadcrumb[0].id, root_id);
}

pub(crate) fn contract_opaque_folders_keep_their_flags_and_have_no_children<S: Store>(
    make: impl Fn() -> S,
) {
    let store = make();
    let scan = folder_scan(&store, 1_000);
    save(&store, scan, &sample());
    let users = store
        .list_folder_children(scan, None, FolderSort::Size, 0, 50)
        .unwrap()
        .items[0]
        .id;
    let page = store
        .list_folder_children(scan, Some(users), FolderSort::Size, 0, 50)
        .unwrap();
    let bob = page
        .items
        .iter()
        .find(|row| row.path.ends_with("bob"))
        .expect("bob row");
    assert_eq!(bob.opaque, Some(OpaqueReason::OtherUserProfile));
    assert_eq!(bob.access, Access::Incomplete);
    assert!(!bob.has_children);
    let opaque_page = store
        .list_folder_children(scan, Some(bob.id), FolderSort::Size, 0, 50)
        .unwrap();
    assert_eq!(opaque_page.total, 0);
    assert!(opaque_page.items.is_empty());
    assert_eq!(opaque_page.remainder_bytes, 0);
    assert_eq!(
        opaque_page
            .breadcrumb
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        vec![r"C:\", "Users", "bob"]
    );
}

pub(crate) fn contract_failed_save_is_atomic<S: Store>(make: impl Fn() -> S) {
    let store = make();
    let scan = folder_scan(&store, 1_000);
    let mut two_roots = sample();
    two_roots.folders[1].parent = None;
    two_roots.folders[1].depth = 0;
    let mut forward_parent = sample();
    forward_parent.folders[1].parent = Some(3);
    let mut foreign_file = sample();
    foreign_file.large_files[0].folder = 99;
    let mut too_many_issues = sample();
    too_many_issues.issues = (0..51)
        .map(|i| FolderIssue {
            path: format!(r"C:\x\{i}"),
            reason: IssueReason::IoError,
        })
        .collect();
    let mut duplicate_path = sample();
    duplicate_path.folders[2].path = r"C:\Users".into();
    let mut direct_over_total = sample();
    direct_over_total.folders[2].direct_bytes = 41;
    let mut wrong_depth = sample();
    wrong_depth.folders[2].depth = 5;
    let mut empty_file = sample();
    empty_file.large_files[1].size_bytes = 0;
    let mut stored_mismatch = sample();
    stored_mismatch.counters.folders_stored = 4;
    let mut empty = sample();
    empty.folders.clear();
    empty.large_files.clear();
    for (name, bad) in [
        ("two roots", two_roots),
        ("parent after child", forward_parent),
        ("file with unknown folder", foreign_file),
        ("51 issues", too_many_issues),
        ("repeated path", duplicate_path),
        ("direct over total", direct_over_total),
        ("depth does not follow parent", wrong_depth),
        ("empty file", empty_file),
        ("folders_stored mismatch", stored_mismatch),
        ("no folders", empty),
    ] {
        let result = store.complete_folder_scan(scan, &bad, EpochMillis(2_000));
        assert!(
            matches!(result, Err(StoreError::Constraint(_))),
            "{name}: {result:?}"
        );
        assert_eq!(
            store.get_scan(scan).unwrap().unwrap().status,
            ScanStatus::Running,
            "{name}"
        );
        assert_eq!(store.last_done_folder_scan().unwrap(), None, "{name}");
    }
    // Tras tantos rechazos, un resultado valido se guarda con normalidad.
    save(&store, scan, &sample());
    assert!(store.last_done_folder_scan().unwrap().is_some());
}

pub(crate) fn contract_complete_requires_a_running_folders_scan<S: Store>(make: impl Fn() -> S) {
    let store = make();
    let results = sample();
    assert!(matches!(
        store.complete_folder_scan(ScanId(999), &results, EpochMillis(2)),
        Err(StoreError::NotFound(_))
    ));
    let junk = store
        .create_scan(&NewScan {
            kind: ScanKind::Junk,
            trigger: ScanTrigger::Manual,
            started_at: EpochMillis(1),
        })
        .unwrap();
    assert!(matches!(
        store.complete_folder_scan(junk, &results, EpochMillis(2)),
        Err(StoreError::InvalidState(_))
    ));
    let scan = folder_scan(&store, 5);
    save(&store, scan, &results);
    assert!(matches!(
        store.complete_folder_scan(scan, &results, EpochMillis(3)),
        Err(StoreError::InvalidState(_))
    ));
}

pub(crate) fn contract_second_scan_purges_the_previous_tree_keeping_its_result<S: Store>(
    make: impl Fn() -> S,
) {
    let store = make();
    let first = folder_scan(&store, 1_000);
    save(&store, first, &sample());
    let first_summary = store.last_done_folder_scan().unwrap().unwrap();
    let second = folder_scan(&store, 2_000);
    let mut next = sample();
    next.root_path = r"C:\".into();
    next.large_files.truncate(1);
    next.issues.clear();
    save(&store, second, &next);

    assert_eq!(
        store.last_done_folder_scan().unwrap().unwrap().scan.id,
        second
    );
    assert_eq!(
        store
            .list_large_files(second, LargeFileSort::Size, 0, 50)
            .unwrap()
            .total,
        1
    );
    // El primero conserva su escaneo y su resultado, pero ya no sus filas.
    assert_eq!(
        store.get_scan(first).unwrap().unwrap().status,
        ScanStatus::Done
    );
    let kept = store.folder_scan_summary(first).unwrap().unwrap();
    assert_eq!(kept.counters, first_summary.counters);
    assert_eq!(kept.root_folder_id, None);
    assert!(kept.issues.is_empty());
    assert!(matches!(
        store.list_folder_children(first, None, FolderSort::Size, 0, 10),
        Err(StoreError::NotFound(_))
    ));
    assert_eq!(
        store
            .list_large_files(first, LargeFileSort::Size, 0, 10)
            .unwrap()
            .total,
        0
    );
    assert_eq!(
        store
            .get_folder(first, first_summary.root_folder_id.unwrap())
            .unwrap(),
        None
    );
}

pub(crate) fn contract_other_scan_kinds_are_untouched<S: Store>(make: impl Fn() -> S) {
    let store = make();
    let dup_scan = dup::dup_scan(&store, 500);
    let groups = vec![dup::group(4 << 20, 1, "a", 2, 100)];
    let dup_results: DuplicateScanResults = dup::results(groups);
    store
        .complete_duplicate_scan(dup_scan, &dup_results, EpochMillis(600))
        .unwrap();
    let scan = folder_scan(&store, 1_000);
    save(&store, scan, &sample());
    let second = folder_scan(&store, 2_000);
    save(&store, second, &sample());
    let summary = store.last_done_duplicate_scan().unwrap().unwrap();
    assert_eq!(summary.scan.id, dup_scan);
    assert_eq!(
        store
            .list_duplicate_groups(dup_scan, 0, 50, None, None)
            .unwrap()
            .total,
        1
    );
}

pub(crate) fn contract_last_done_ignores_unfinished_and_other_kinds<S: Store>(
    make: impl Fn() -> S,
) {
    let store = make();
    let running = folder_scan(&store, 1_000);
    assert_eq!(store.last_done_folder_scan().unwrap(), None);
    store
        .finish_scan(running, ScanStatus::Cancelled, EpochMillis(1_001))
        .unwrap();
    assert_eq!(store.last_done_folder_scan().unwrap(), None);
    let errored = folder_scan(&store, 1_100);
    store
        .finish_scan(errored, ScanStatus::Error, EpochMillis(1_101))
        .unwrap();
    let other = store
        .create_scan(&NewScan {
            kind: ScanKind::Junk,
            trigger: ScanTrigger::Manual,
            started_at: EpochMillis(1_200),
        })
        .unwrap();
    store
        .finish_scan(other, ScanStatus::Done, EpochMillis(1_201))
        .unwrap();
    assert_eq!(store.last_done_folder_scan().unwrap(), None);
    let done = folder_scan(&store, 1_300);
    save(&store, done, &sample());
    assert_eq!(
        store.last_done_folder_scan().unwrap().unwrap().scan.id,
        done
    );
}

fn sized_tree(sizes: [(&str, u64); 3]) -> FolderScanResults {
    let mut folders = vec![folder(None, r"C:\r", 0, 1_000, 0)];
    for (name, size) in sizes {
        folders.push(folder(Some(0), &format!(r"C:\r\{name}"), 1, size, size));
    }
    FolderScanResults {
        root_kind: RootKind::Picked,
        root_path: r"C:\r".into(),
        volume: None,
        counters: FolderCounters {
            total_bytes: 1_000,
            folders_stored: 4,
            ..FolderCounters::default()
        },
        folders_truncated: false,
        folders,
        large_files: vec![],
        issues: vec![],
    }
}

pub(crate) fn contract_children_sort_page_and_breadcrumb<S: Store>(make: impl Fn() -> S) {
    let store = make();
    let scan = folder_scan(&store, 1_000);
    save(
        &store,
        scan,
        &sized_tree([("beta", 10 << 20), ("Alfa", 30 << 20), ("gamma", 20 << 20)]),
    );
    let by_size = child_paths(&store, scan, None, FolderSort::Size);
    assert_eq!(by_size, vec![r"C:\r\Alfa", r"C:\r\gamma", r"C:\r\beta"]);
    let by_name = child_paths(&store, scan, None, FolderSort::Name);
    assert_eq!(by_name, vec![r"C:\r\Alfa", r"C:\r\beta", r"C:\r\gamma"]);
    let page = store
        .list_folder_children(scan, None, FolderSort::Size, 1, 1)
        .unwrap();
    assert_eq!(page.total, 3);
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].path, r"C:\r\gamma");
    let beyond = store
        .list_folder_children(scan, None, FolderSort::Size, 50, 10)
        .unwrap();
    assert_eq!((beyond.total, beyond.items.len()), (3, 0));
    let child_id = store
        .list_folder_children(scan, None, FolderSort::Size, 0, 1)
        .unwrap()
        .items[0]
        .id;
    let inside = store
        .list_folder_children(scan, Some(child_id), FolderSort::Size, 0, 10)
        .unwrap();
    assert_eq!(inside.folder.id, child_id);
    assert_eq!(
        inside
            .breadcrumb
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        vec![r"C:\r", "Alfa"]
    );
    assert_eq!(inside.total, 0);
    assert_eq!(inside.remainder_bytes, 0);
}

pub(crate) fn contract_equal_sizes_and_names_tie_break_by_id<S: Store>(make: impl Fn() -> S) {
    let store = make();
    let scan = folder_scan(&store, 1_000);
    save(&store, scan, &sized_tree([("b", 7), ("a", 7), ("B", 7)]));
    let by_size = child_paths(&store, scan, None, FolderSort::Size);
    assert_eq!(
        by_size,
        vec![r"C:\r\b", r"C:\r\a", r"C:\r\B"],
        "id ascendente"
    );
    let by_name = child_paths(&store, scan, None, FolderSort::Name);
    assert_eq!(
        by_name,
        vec![r"C:\r\a", r"C:\r\b", r"C:\r\B"],
        "plegado y luego id"
    );
}

pub(crate) fn contract_a_folder_of_another_scan_is_not_found<S: Store>(make: impl Fn() -> S) {
    let store = make();
    let first = folder_scan(&store, 1_000);
    save(&store, first, &sample());
    let first_root = store
        .last_done_folder_scan()
        .unwrap()
        .unwrap()
        .root_folder_id
        .unwrap();
    let second = folder_scan(&store, 2_000);
    save(&store, second, &sample());
    assert!(matches!(
        store.list_folder_children(second, Some(first_root), FolderSort::Size, 0, 10),
        Err(StoreError::NotFound(_))
    ));
    assert!(matches!(
        store.list_folder_children(second, Some(999_999), FolderSort::Size, 0, 10),
        Err(StoreError::NotFound(_))
    ));
    assert_eq!(store.get_folder(second, first_root).unwrap(), None);
    let own_root = store
        .last_done_folder_scan()
        .unwrap()
        .unwrap()
        .root_folder_id
        .unwrap();
    assert_eq!(
        store.get_folder(second, own_root).unwrap().unwrap().path,
        r"C:\"
    );
}

pub(crate) fn contract_large_files_sort_page_and_lookup<S: Store>(make: impl Fn() -> S) {
    let store = make();
    let scan = folder_scan(&store, 1_000);
    save(&store, scan, &sample());
    let paths = |sort: LargeFileSort, offset: u64, limit: u32| -> Vec<String> {
        store
            .list_large_files(scan, sort, offset, limit)
            .unwrap()
            .items
            .iter()
            .map(|f| f.path.clone())
            .collect()
    };
    assert_eq!(
        paths(LargeFileSort::Size, 0, 10),
        vec![
            r"C:\Windows\big.dll",
            r"C:\Users\ana\a.bin",
            r"C:\Users\ana\b.bin"
        ]
    );
    assert_eq!(
        paths(LargeFileSort::Newest, 0, 10),
        vec![
            r"C:\Windows\big.dll",
            r"C:\Users\ana\a.bin",
            r"C:\Users\ana\b.bin"
        ]
    );
    assert_eq!(
        paths(LargeFileSort::Oldest, 0, 10),
        vec![
            r"C:\Users\ana\a.bin",
            r"C:\Windows\big.dll",
            r"C:\Users\ana\b.bin"
        ],
        "sin fecha al final"
    );
    assert_eq!(
        paths(LargeFileSort::Size, 1, 1),
        vec![r"C:\Users\ana\a.bin"]
    );
    let page = store
        .list_large_files(scan, LargeFileSort::Size, 9, 5)
        .unwrap();
    assert_eq!((page.total, page.items.len()), (3, 0));
    let first = store
        .list_large_files(scan, LargeFileSort::Size, 0, 1)
        .unwrap()
        .items[0]
        .clone();
    assert_eq!(first.size_bytes, 30);
    assert_eq!(first.modified_at, Some(EpochMillis(300)));
    let folder_row = store.get_folder(scan, first.folder_id).unwrap().unwrap();
    assert_eq!(folder_row.path, r"C:\Windows");
    assert_eq!(
        store.get_large_file(scan, first.id).unwrap(),
        Some(first.clone())
    );
    assert_eq!(store.get_large_file(ScanId(999), first.id).unwrap(), None);
}

pub(crate) fn contract_rows_follow_scan_retention<S: Store>(make: impl Fn() -> S) {
    let store = make();
    let doomed = folder_scan(&store, 1_000);
    save(&store, doomed, &sample());
    for i in 1..=SCAN_RETENTION as i64 {
        let id = store
            .create_scan(&NewScan {
                kind: ScanKind::Full,
                trigger: ScanTrigger::Manual,
                started_at: EpochMillis(1_000 + i),
            })
            .unwrap();
        store
            .finish_scan(id, ScanStatus::Done, EpochMillis(1_001 + i))
            .unwrap();
    }
    assert_eq!(store.get_scan(doomed).unwrap(), None);
    assert_eq!(store.folder_scan_summary(doomed).unwrap(), None);
    assert_eq!(
        store
            .list_large_files(doomed, LargeFileSort::Size, 0, 10)
            .unwrap()
            .total,
        0
    );
    assert_eq!(store.last_done_folder_scan().unwrap(), None);
}

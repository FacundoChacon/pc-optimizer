use std::path::Path;
use std::sync::Arc;

use super::*;
use crate::domain::duplicates::model::RootReason::ContextUnavailable;
use crate::domain::duplicates::model::RootStatus;
use crate::domain::ports::fakes::{FakeFileSystem, FsOp};
use crate::domain::ports::fakes_content::FakeContentReader;
use crate::domain::ports::{FileIdentity, PortError};
use crate::domain::protection::test_support::ctx_c_ana;

const MIB: usize = 1 << 20;
const HOME: &str = r"C:\Users\ana";
const MTIME: EpochMillis = EpochMillis(1_000);

#[derive(Default)]
struct Events {
    walking: Vec<u64>,
    partial: Vec<(u64, u64)>,
    full: Vec<(u64, u64)>,
    cancel_on_walking: Option<Arc<AtomicBool>>,
}

impl DuplicateScanObserver for Events {
    fn walking(&mut self, files_seen: u64) {
        self.walking.push(files_seen);
        if let Some(flag) = &self.cancel_on_walking {
            flag.store(true, Ordering::Relaxed);
        }
    }
    fn partial(&mut self, done: u64, total: u64) {
        self.partial.push((done, total));
    }
    fn full(&mut self, done_bytes: u64, total_bytes: u64) {
        self.full.push((done_bytes, total_bytes));
    }
}

/// Bytes que solo dependen de `tag` (mismo tag, mismo contenido).
fn bytes(tag: u8, len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| u8::try_from((i * 7 + usize::from(tag) * 13 + i / 331) % 251).unwrap_or(0))
        .collect()
}

fn base_fs() -> FakeFileSystem {
    FakeFileSystem::new()
        .canonical_as_is()
        .with_dir(r"C:\Users")
        .with_dir(HOME)
}

struct Fixture {
    fs: FakeFileSystem,
    contents: Vec<(String, Vec<u8>)>,
}

impl Fixture {
    fn new() -> Self {
        let fs = [
            "Desktop",
            "Documents",
            "Downloads",
            "Music",
            "Pictures",
            "Videos",
        ]
        .iter()
        .fold(base_fs(), |fs, name| {
            fs.with_dir(&format!(r"{HOME}\{name}"))
        });
        Self {
            fs,
            contents: Vec::new(),
        }
    }

    fn dir(mut self, path: &str) -> Self {
        self.fs = self.fs.with_dir(path);
        self
    }

    fn file(mut self, path: &str, content: Vec<u8>) -> Self {
        self.fs = self.fs.with_file(path, content.len() as u64, Some(MTIME));
        self.contents.push((path.to_string(), content));
        self
    }

    fn with(mut self, f: impl FnOnce(FakeFileSystem) -> FakeFileSystem) -> Self {
        self.fs = f(self.fs);
        self
    }

    fn build(self) -> (Arc<FakeFileSystem>, FakeContentReader) {
        let fs = Arc::new(self.fs);
        let reader = self.contents.into_iter().fold(
            FakeContentReader::new(Arc::clone(&fs)),
            |reader, (path, content)| reader.with_content(&path, content),
        );
        (fs, reader)
    }
}

fn request(roots: Option<&[&str]>, min: Option<u64>) -> DuplicateScanRequest {
    DuplicateScanRequest::new(
        roots.map(|list| list.iter().map(|k| (*k).to_string()).collect()),
        min,
    )
    .unwrap()
}

fn scan(
    fs: &FakeFileSystem,
    reader: &FakeContentReader,
    request: &DuplicateScanRequest,
) -> (DuplicateScanOutcome, Events) {
    scan_with(
        fs,
        reader,
        request,
        &AtomicBool::new(false),
        Events::default(),
    )
}

fn scan_with(
    fs: &FakeFileSystem,
    reader: &FakeContentReader,
    request: &DuplicateScanRequest,
    cancel: &AtomicBool,
    mut events: Events,
) -> (DuplicateScanOutcome, Events) {
    let outcome = scan_duplicates(request, Ok(ctx_c_ana()), fs, reader, cancel, &mut events);
    (outcome, events)
}

fn completed(outcome: DuplicateScanOutcome) -> DuplicateScanResults {
    match outcome {
        DuplicateScanOutcome::Completed(results) => results,
        DuplicateScanOutcome::Cancelled => panic!("el escaneo se cancelo"),
    }
}

fn paths(group: &DuplicateGroup) -> Vec<&str> {
    group.files.iter().map(|f| f.path.as_str()).collect()
}

const A: &str = r"C:\Users\ana\Downloads\foto.jpg";
const B: &str = r"C:\Users\ana\Pictures\foto (1).jpg";

// ---------------------------------------------------------------- elegibilidad (D-04)

#[test]
fn scan_should_ignore_files_below_the_minimum_without_resolving_their_protection() {
    let (fs, reader) = Fixture::new()
        .file(A, bytes(1, 900 * 1024))
        .file(B, bytes(1, 900 * 1024))
        .file(r"C:\Users\ana\Downloads\big1.bin", bytes(2, 2 * MIB))
        .file(r"C:\Users\ana\Downloads\big2.bin", bytes(2, 2 * MIB))
        .build();
    let results = completed(scan(&fs, &reader, &request(None, None)).0);
    assert_eq!(results.counters.below_min_size, 2);
    assert_eq!(results.counters.files_seen, 4);
    assert_eq!(results.min_size.bytes(), MIB as u64);
    assert_eq!(results.groups.len(), 1);
    assert_eq!(results.groups[0].size_bytes, 2 * MIB as u64);
    assert_eq!(fs.call_count(FsOp::Metadata, A), 0);
    assert_eq!(fs.call_count(FsOp::Canonicalize, B), 0);
    assert!(!reader.opened().iter().any(|p| p == Path::new(A)));
}

#[test]
fn scan_should_apply_the_chosen_minimum_size() {
    let build = || {
        Fixture::new()
            .file(A, bytes(1, 900 * 1024))
            .file(B, bytes(1, 900 * 1024))
            .build()
    };
    let (fs, reader) = build();
    let default = completed(scan(&fs, &reader, &request(None, None)).0);
    assert!(default.groups.is_empty());
    let (fs, reader) = build();
    let small = completed(scan(&fs, &reader, &request(None, Some(102_400))).0);
    assert_eq!(small.groups.len(), 1);
    assert_eq!(small.min_size.bytes(), 102_400);
}

#[test]
fn scan_should_count_protected_files_by_decision_and_never_open_them() {
    let (fs, reader) = Fixture::new()
        .file(A, bytes(1, 2 * MIB))
        .file(B, bytes(1, 2 * MIB))
        .file(r"C:\Users\ana\Documents\locked.bin", bytes(1, 2 * MIB))
        .with(|fs| {
            fs.fail_on(
                r"C:\Users\ana\Documents\locked.bin",
                PortError::PermissionDenied("x".into()),
            )
        })
        .build();
    let results = completed(scan(&fs, &reader, &request(None, None)).0);
    assert_eq!(results.counters.protected_count, 1);
    assert_eq!(results.groups.len(), 1);
    assert_eq!(results.groups[0].files.len(), 2);
    assert_eq!(reader.open_count(r"C:\Users\ana\Documents\locked.bin"), 0);
}

#[test]
fn scan_should_count_cloud_files_and_never_open_them() {
    let cloud = r"C:\Users\ana\Documents\nube.bin";
    let (fs, reader) = Fixture::new()
        .file(A, bytes(1, 2 * MIB))
        .file(B, bytes(1, 2 * MIB))
        .with(|fs| fs.with_cloud_placeholder(cloud, 2 * MIB as u64))
        .build();
    // El doble tiene contenido para la ruta de nube: si se abriera, quedaria registrado.
    reader.replace_content(Path::new(cloud), bytes(1, 2 * MIB));
    let results = completed(scan(&fs, &reader, &request(None, None)).0);
    assert_eq!(results.counters.cloud_count, 1);
    assert_eq!(results.groups.len(), 1);
    assert!(!reader.opened().iter().any(|p| p == Path::new(cloud)));
    assert_eq!(reader.bytes_read_of(cloud), 0);
}

#[test]
fn scan_should_exclude_hard_links_and_count_them_apart() {
    let (fs, reader) = Fixture::new()
        .file(r"C:\Users\ana\Downloads\a.iso", bytes(1, 2 * MIB))
        .with(|fs| {
            fs.with_hard_link(
                r"C:\Users\ana\Documents\a.iso",
                r"C:\Users\ana\Downloads\a.iso",
            )
        })
        .build();
    reader.replace_content(
        Path::new(r"C:\Users\ana\Documents\a.iso"),
        bytes(1, 2 * MIB),
    );
    let results = completed(scan(&fs, &reader, &request(None, None)).0);
    assert_eq!(results.counters.hard_linked_count, 2);
    assert!(results.groups.is_empty());
    assert!(reader.opened().is_empty());
}

#[test]
fn scan_should_use_the_size_read_from_the_handle_not_from_the_listing() {
    let stale = r"C:\Users\ana\Downloads\creciendo.bin";
    let (fs, reader) = Fixture::new()
        .file(stale, bytes(1, 500 * 1024))
        .file(B, bytes(1, 500 * 1024))
        .with(|fs| fs.with_stale_listing(stale, 3 * MIB as u64, Some(MTIME)))
        .build();
    let results = completed(scan(&fs, &reader, &request(None, None)).0);
    // El listado decia 3 MiB (paso el filtro 1) pero el handle dice 500 KiB.
    assert_eq!(results.counters.below_min_size, 2);
    assert!(results.groups.is_empty());
    assert!(reader.opened().is_empty());
}

#[test]
fn scan_should_keep_a_single_candidate_per_identity() {
    let shared = FileIdentity {
        volume_serial: 1,
        file_id: 4_242,
    };
    let (fs, reader) = Fixture::new()
        .file(A, bytes(1, 2 * MIB))
        .file(B, bytes(1, 2 * MIB))
        .with(|fs| fs.with_identity(A, shared).with_identity(B, shared))
        .build();
    let results = completed(scan(&fs, &reader, &request(None, None)).0);
    assert!(results.groups.is_empty());
    assert_eq!(reader.opened().len(), 0);
}

#[test]
fn scan_should_find_nothing_when_the_context_is_invalid() {
    let (fs, reader) = Fixture::new()
        .file(A, bytes(1, 2 * MIB))
        .file(B, bytes(1, 2 * MIB))
        .build();
    let mut events = Events::default();
    let outcome = scan_duplicates(
        &request(None, None),
        Err(ContextError::MissingRequired("windows_dir")),
        fs.as_ref(),
        &reader,
        &AtomicBool::new(false),
        &mut events,
    );
    let results = completed(outcome);
    assert!(results.groups.is_empty());
    assert_eq!(results.counters.files_seen, 0);
    assert!(
        results
            .roots
            .iter()
            .all(|root| root.status == RootStatus::Unavailable(ContextUnavailable))
    );
    assert!(reader.opened().is_empty());
}

#[test]
fn scan_should_only_walk_the_requested_roots_and_report_the_rest_as_not_selected() {
    let (fs, reader) = Fixture::new()
        .file(A, bytes(1, 2 * MIB))
        .file(B, bytes(1, 2 * MIB))
        .file(r"C:\Users\ana\Downloads\otra.bin", bytes(1, 2 * MIB))
        .build();
    let results = completed(scan(&fs, &reader, &request(Some(&["downloads"]), None)).0);
    assert_eq!(results.groups.len(), 1);
    assert_eq!(paths(&results.groups[0]).len(), 2);
    let downloads = results
        .roots
        .iter()
        .find(|root| root.folder == ContentFolder::Downloads)
        .unwrap();
    assert_eq!(downloads.status, RootStatus::Done);
    assert_eq!(downloads.files_seen, 2);
    assert_eq!(
        results
            .roots
            .iter()
            .filter(|root| root.status == RootStatus::NotSelected)
            .count(),
        5
    );
    assert_eq!(fs.call_count(FsOp::ReadDir, r"C:\Users\ana\Pictures"), 0);
}

#[test]
fn scan_should_count_excluded_tool_folders_and_skip_their_files() {
    let (fs, reader) = Fixture::new()
        .dir(r"C:\Users\ana\Documents\proyecto")
        .dir(r"C:\Users\ana\Documents\proyecto\node_modules")
        .file(
            r"C:\Users\ana\Documents\proyecto\node_modules\lib.bin",
            bytes(5, 2 * MIB),
        )
        .dir(r"C:\Users\ana\Documents\otro\.git")
        .dir(r"C:\Users\ana\Documents\otro")
        .file(
            r"C:\Users\ana\Documents\otro\.git\pack.bin",
            bytes(5, 2 * MIB),
        )
        .build();
    let results = completed(scan(&fs, &reader, &request(None, None)).0);
    assert_eq!(results.counters.excluded_dirs, 2);
    assert!(results.groups.is_empty());
    assert_eq!(results.counters.files_seen, 0);
}

#[test]
fn scan_should_attribute_each_file_to_its_deepest_root() {
    let ctx_downloads_inside_documents = {
        let mut raw = crate::domain::protection::test_support::raw_c_ana();
        if let Some(user) = raw.user.as_mut() {
            for entry in &mut user.content_folders {
                if entry.0 == ContentFolder::Downloads {
                    entry.1 = r"C:\Users\ana\Documents\Descargas".into();
                }
            }
        }
        ProtectionContext::from_raw(raw).unwrap()
    };
    let (fs, reader) = Fixture::new()
        .dir(r"C:\Users\ana\Documents\Descargas")
        .file(r"C:\Users\ana\Documents\Descargas\a.bin", bytes(3, 2 * MIB))
        .file(r"C:\Users\ana\Documents\b.bin", bytes(3, 2 * MIB))
        .build();
    let mut events = Events::default();
    let outcome = scan_duplicates(
        &request(None, None),
        Ok(ctx_downloads_inside_documents),
        fs.as_ref(),
        &reader,
        &AtomicBool::new(false),
        &mut events,
    );
    let results = completed(outcome);
    let group = &results.groups[0];
    // Cada archivo aparece una sola vez y con su carpeta mas profunda (orden por ruta).
    assert_eq!(
        group.files.iter().map(|f| f.root).collect::<Vec<_>>(),
        [ContentFolder::Documents, ContentFolder::Downloads]
    );
    let status = |folder| results.roots.iter().find(|r| r.folder == folder).unwrap();
    assert_eq!(status(ContentFolder::Downloads).status, RootStatus::Nested);
    assert_eq!(status(ContentFolder::Downloads).files_seen, 1);
    assert_eq!(status(ContentFolder::Documents).files_seen, 1);
}

// ---------------------------------------------------------------- etapas (D-05)

#[test]
fn scan_should_group_two_identical_copies_with_their_reclaimable_bytes() {
    let (fs, reader) = Fixture::new()
        .file(A, bytes(1, 4 * MIB))
        .file(B, bytes(1, 4 * MIB))
        .build();
    let results = completed(scan(&fs, &reader, &request(None, None)).0);
    assert_eq!(results.groups.len(), 1);
    let group = &results.groups[0];
    assert_eq!(group.size_bytes, 4 * MIB as u64);
    assert_eq!(group.blake3, to_hex(&blake3::hash(&bytes(1, 4 * MIB))));
    assert_eq!(group.blake3.len(), 64);
    assert_eq!(group.files.len(), 2);
    assert_eq!(group.reclaimable_bytes(), 4 * MIB as u64);
    assert_eq!(
        group.files.iter().map(|f| f.root).collect::<Vec<_>>(),
        [ContentFolder::Downloads, ContentFolder::Pictures]
    );
    assert_eq!(group.files[0].modified, Some(MTIME));
}

#[test]
fn scan_should_group_three_identical_copies_into_one_group() {
    let (fs, reader) = Fixture::new()
        .file(r"C:\Users\ana\Desktop\c.bin", bytes(9, 10 * MIB))
        .file(r"C:\Users\ana\Documents\b.bin", bytes(9, 10 * MIB))
        .file(r"C:\Users\ana\Downloads\a.bin", bytes(9, 10 * MIB))
        .build();
    let results = completed(scan(&fs, &reader, &request(None, None)).0);
    assert_eq!(results.groups.len(), 1);
    assert_eq!(results.groups[0].files.len(), 3);
    assert_eq!(results.groups[0].reclaimable_bytes(), 20 * MIB as u64);
    assert_eq!(results.duplicate_file_count(), 3);
    assert_eq!(results.reclaimable_bytes(), 20 * MIB as u64);
    // Orden determinista: por ruta.
    assert_eq!(
        paths(&results.groups[0]),
        [
            r"C:\Users\ana\Desktop\c.bin",
            r"C:\Users\ana\Documents\b.bin",
            r"C:\Users\ana\Downloads\a.bin"
        ]
    );
}

#[test]
fn scan_should_reject_a_partial_hash_false_positive_after_hashing_the_whole_file() {
    let edge = 64 * 1024;
    let mut one = bytes(4, 5 * MIB);
    let mut two = one.clone();
    // Mismos 64 KiB iniciales y finales; un byte distinto en el medio.
    one[2 * MIB] = 1;
    two[2 * MIB] = 2;
    assert_eq!(one[..edge], two[..edge]);
    let (fs, reader) = Fixture::new().file(A, one).file(B, two).build();
    let results = completed(scan(&fs, &reader, &request(None, None)).0);
    assert!(results.groups.is_empty());
    // Se calculo el hash completo de ambos (5 MiB + 128 KiB de parcial cada uno).
    assert_eq!(reader.bytes_read_of(A), (5 * MIB + 2 * edge) as u64);
    assert_eq!(reader.bytes_read_of(B), (5 * MIB + 2 * edge) as u64);
}

#[test]
fn scan_should_not_read_a_file_whose_size_is_unique() {
    let unico = r"C:\Users\ana\Documents\unico.bin";
    let (fs, reader) = Fixture::new()
        .file(unico, bytes(7, 3 * MIB))
        .file(A, bytes(1, 2 * MIB))
        .file(B, bytes(1, 2 * MIB))
        .build();
    let results = completed(scan(&fs, &reader, &request(None, None)).0);
    assert_eq!(results.groups.len(), 1);
    assert_eq!(reader.open_count(unico), 0);
}

#[test]
fn scan_should_not_compute_the_full_hash_when_the_partial_hash_differs() {
    let (fs, reader) = Fixture::new()
        .file(A, bytes(1, 5 * MIB))
        .file(B, bytes(2, 5 * MIB))
        .build();
    let results = completed(scan(&fs, &reader, &request(None, None)).0);
    assert!(results.groups.is_empty());
    assert_eq!(reader.bytes_read_of(A), 128 * 1024);
    assert_eq!(reader.bytes_read_of(B), 128 * 1024);
    assert_eq!(results.counters.hashed_bytes, 256 * 1024);
}

#[test]
fn scan_should_read_a_small_file_once_and_use_it_as_partial_and_full() {
    let len = 110 * 1024;
    let (fs, reader) = Fixture::new()
        .file(A, bytes(6, len))
        .file(B, bytes(6, len))
        .build();
    let results = completed(scan(&fs, &reader, &request(None, Some(102_400))).0);
    assert_eq!(results.groups.len(), 1);
    assert_eq!(reader.open_count(A), 1);
    assert_eq!(reader.open_count(B), 1);
    assert_eq!(reader.bytes_read_of(A), len as u64);
    assert_eq!(
        results.groups[0].blake3,
        to_hex(&blake3::hash(&bytes(6, len)))
    );
}

#[test]
fn scan_should_order_groups_by_size_descending_and_copies_by_path() {
    let (fs, reader) = Fixture::new()
        .file(r"C:\Users\ana\Downloads\z.bin", bytes(1, 2 * MIB))
        .file(r"C:\Users\ana\Downloads\y.bin", bytes(1, 2 * MIB))
        .file(r"C:\Users\ana\Music\q.bin", bytes(2, 6 * MIB))
        .file(r"C:\Users\ana\Music\p.bin", bytes(2, 6 * MIB))
        .build();
    let first = completed(scan(&fs, &reader, &request(None, None)).0);
    let sizes: Vec<u64> = first.groups.iter().map(|g| g.size_bytes).collect();
    assert_eq!(sizes, [6 * MIB as u64, 2 * MIB as u64]);
    assert_eq!(
        paths(&first.groups[0]),
        [r"C:\Users\ana\Music\p.bin", r"C:\Users\ana\Music\q.bin"]
    );
    let (fs2, reader2) = Fixture::new()
        .file(r"C:\Users\ana\Music\p.bin", bytes(2, 6 * MIB))
        .file(r"C:\Users\ana\Music\q.bin", bytes(2, 6 * MIB))
        .file(r"C:\Users\ana\Downloads\y.bin", bytes(1, 2 * MIB))
        .file(r"C:\Users\ana\Downloads\z.bin", bytes(1, 2 * MIB))
        .build();
    let second = completed(scan(&fs2, &reader2, &request(None, None)).0);
    let shape = |results: &DuplicateScanResults| -> Vec<(u64, String, Vec<String>)> {
        results
            .groups
            .iter()
            .map(|g| {
                (
                    g.size_bytes,
                    g.blake3.clone(),
                    g.files.iter().map(|f| f.path.clone()).collect(),
                )
            })
            .collect()
    };
    assert_eq!(shape(&first), shape(&second));
}

// ---------------------------------------------------------------- lectura verificada

#[test]
fn scan_should_count_a_file_that_grows_while_being_read_as_changed() {
    let (fs, reader) = Fixture::new()
        .file(A, bytes(1, 3 * MIB))
        .file(B, bytes(1, 3 * MIB))
        .build();
    let hook_fs = Arc::clone(&fs);
    let reader = reader.after_read(A, 3, move || {
        hook_fs.set_size_and_time(Path::new(A), 4 * MIB as u64, Some(MTIME));
    });
    let results = completed(scan(&fs, &reader, &request(None, None)).0);
    assert_eq!(results.counters.changed_count, 1);
    assert!(results.groups.is_empty());
}

#[test]
fn scan_should_count_a_file_replaced_before_reading_as_changed() {
    let (fs, reader) = Fixture::new()
        .file(A, bytes(1, 3 * MIB))
        .file(B, bytes(1, 3 * MIB))
        .build();
    let hook_fs = Arc::clone(&fs);
    let reader = reader.on_open(A, move || {
        hook_fs.recreate_file(
            Path::new(A),
            FileIdentity {
                volume_serial: 1,
                file_id: 31_337,
            },
        );
    });
    let results = completed(scan(&fs, &reader, &request(None, None)).0);
    assert_eq!(results.counters.changed_count, 1);
    assert!(results.groups.is_empty());
}

#[test]
fn scan_should_count_in_use_and_unreadable_files_as_unreadable() {
    let (fs, reader) = Fixture::new()
        .file(A, bytes(1, 3 * MIB))
        .file(B, bytes(1, 3 * MIB))
        .file(r"C:\Users\ana\Music\c.bin", bytes(1, 3 * MIB))
        .build();
    let reader = reader
        .fail_open(A, PortError::InUse("x".into()))
        .fail_read(r"C:\Users\ana\Music\c.bin", PortError::Io("sector".into()));
    let results = completed(scan(&fs, &reader, &request(None, None)).0);
    assert_eq!(results.counters.unreadable_count, 2);
    assert!(results.groups.is_empty());
}

#[test]
fn scan_should_keep_a_pair_when_a_third_copy_fails() {
    let (fs, reader) = Fixture::new()
        .file(A, bytes(1, 3 * MIB))
        .file(B, bytes(1, 3 * MIB))
        .file(r"C:\Users\ana\Music\c.bin", bytes(1, 3 * MIB))
        .build();
    let reader = reader.fail_open(r"C:\Users\ana\Music\c.bin", PortError::InUse("x".into()));
    let results = completed(scan(&fs, &reader, &request(None, None)).0);
    assert_eq!(results.counters.unreadable_count, 1);
    assert_eq!(results.groups.len(), 1);
    assert_eq!(results.groups[0].files.len(), 2);
}

// ---------------------------------------------------------------- cancelacion y progreso

#[test]
fn scan_should_return_cancelled_without_reading_when_cancelled_beforehand() {
    let (fs, reader) = Fixture::new()
        .file(A, bytes(1, 2 * MIB))
        .file(B, bytes(1, 2 * MIB))
        .build();
    let (outcome, _) = scan_with(
        &fs,
        &reader,
        &request(None, None),
        &AtomicBool::new(true),
        Events::default(),
    );
    assert_eq!(outcome, DuplicateScanOutcome::Cancelled);
    assert!(reader.opened().is_empty());
}

#[test]
fn scan_should_cancel_during_the_walk() {
    let mut fixture = Fixture::new();
    for i in 0..300 {
        fixture = fixture.file(
            &format!(r"C:\Users\ana\Downloads\f{i:03}.bin"),
            vec![1u8; 10],
        );
    }
    let (fs, reader) = fixture
        .file(r"C:\Users\ana\Pictures\zz1.bin", bytes(1, 2 * MIB))
        .file(r"C:\Users\ana\Pictures\zz2.bin", bytes(1, 2 * MIB))
        .build();
    let cancel = Arc::new(AtomicBool::new(false));
    let events = Events {
        cancel_on_walking: Some(Arc::clone(&cancel)),
        ..Events::default()
    };
    let (outcome, events) = scan_with(&fs, &reader, &request(None, None), &cancel, events);
    assert_eq!(outcome, DuplicateScanOutcome::Cancelled);
    assert_eq!(events.walking, [256]);
    assert!(reader.opened().is_empty());
}

#[test]
fn scan_should_cancel_during_the_partial_stage() {
    let (fs, reader) = Fixture::new()
        .file(A, bytes(1, 3 * MIB))
        .file(B, bytes(1, 3 * MIB))
        .file(r"C:\Users\ana\Music\c.bin", bytes(1, 3 * MIB))
        .build();
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancel);
    let reader = reader.after_read(A, 1, move || flag.store(true, Ordering::Relaxed));
    let (outcome, _) = scan_with(
        &fs,
        &reader,
        &request(None, None),
        &cancel,
        Events::default(),
    );
    assert_eq!(outcome, DuplicateScanOutcome::Cancelled);
    // Los demas archivos no se abrieron despues de levantar la cancelacion.
    assert_eq!(reader.open_count(B), 0);
    assert_eq!(reader.open_count(r"C:\Users\ana\Music\c.bin"), 0);
}

#[test]
fn scan_should_cancel_inside_the_full_hash_of_a_2_gib_file() {
    let size = 2u64 << 30;
    let fs = Arc::new(
        base_fs()
            .with_dir(&format!(r"{HOME}\Videos"))
            .with_file(r"C:\Users\ana\Videos\a.mkv", size, Some(MTIME))
            .with_file(r"C:\Users\ana\Videos\b.mkv", size, Some(MTIME)),
    );
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancel);
    // Lecturas de a.mkv: 1 y 2 son el parcial (cabeza y cola); la 3 es el primer bloque del
    // hash completo.
    let reader = FakeContentReader::new(Arc::clone(&fs))
        .with_filled(r"C:\Users\ana\Videos\a.mkv", size, 5)
        .with_filled(r"C:\Users\ana\Videos\b.mkv", size, 5)
        .after_read(r"C:\Users\ana\Videos\a.mkv", 3, move || {
            flag.store(true, Ordering::Relaxed);
        });
    let (outcome, _) = scan_with(
        &fs,
        &reader,
        &request(Some(&["videos"]), None),
        &cancel,
        Events::default(),
    );
    assert_eq!(outcome, DuplicateScanOutcome::Cancelled);
    assert_eq!(
        reader.bytes_read_of(r"C:\Users\ana\Videos\a.mkv"),
        (128 * 1024 + MIB) as u64
    );
    assert_eq!(reader.open_count(r"C:\Users\ana\Videos\b.mkv"), 1);
}

#[test]
fn scan_should_report_the_walk_every_256_files_and_once_at_the_end() {
    let mut fixture = Fixture::new();
    for i in 0..300 {
        fixture = fixture.file(
            &format!(r"C:\Users\ana\Downloads\f{i:03}.bin"),
            vec![1u8; 10],
        );
    }
    let (fs, reader) = fixture.build();
    let (_, events) = scan(&fs, &reader, &request(None, None));
    assert_eq!(events.walking, [256, 300]);
    // Sin candidatos: las etapas 2 y 3 informan su cambio de etapa en cero.
    assert_eq!(events.partial, [(0, 0)]);
    assert_eq!(events.full, [(0, 0)]);
}

#[test]
fn scan_should_report_the_partial_stage_every_64_files() {
    let mut fixture = Fixture::new();
    for i in 0..65usize {
        let size = 102_400 + i;
        let content = bytes(u8::try_from(i).unwrap_or(0), size);
        fixture = fixture
            .file(
                &format!(r"C:\Users\ana\Downloads\p{i:02}.bin"),
                content.clone(),
            )
            .file(&format!(r"C:\Users\ana\Pictures\p{i:02}.bin"), content);
    }
    let (fs, reader) = fixture.build();
    let (outcome, events) = scan(&fs, &reader, &request(None, Some(102_400)));
    assert_eq!(completed(outcome).groups.len(), 65);
    assert_eq!(
        events.partial,
        [(0, 130), (64, 130), (128, 130), (130, 130)]
    );
    // Todos son chicos: el hash parcial ya es el completo.
    assert_eq!(events.full, [(0, 0)]);
}

#[test]
fn scan_should_report_the_full_stage_every_16_mib_and_at_the_end() {
    let size = 20 * MIB as u64;
    let fs = Arc::new(
        base_fs()
            .with_dir(&format!(r"{HOME}\Music"))
            .with_file(r"C:\Users\ana\Music\a.bin", size, Some(MTIME))
            .with_file(r"C:\Users\ana\Music\b.bin", size, Some(MTIME)),
    );
    let reader = FakeContentReader::new(Arc::clone(&fs))
        .with_filled(r"C:\Users\ana\Music\a.bin", size, 1)
        .with_filled(r"C:\Users\ana\Music\b.bin", size, 1);
    let (outcome, events) = scan(&fs, &reader, &request(Some(&["music"]), None));
    assert_eq!(completed(outcome).groups.len(), 1);
    let total = 2 * size;
    let mib = MIB as u64;
    // Cada 16 MiB acumulados (16 y 32) y el final (40).
    assert_eq!(
        events.full,
        [
            (0, total),
            (16 * mib, total),
            (32 * mib, total),
            (40 * mib, total)
        ]
    );
}

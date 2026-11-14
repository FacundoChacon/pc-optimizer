use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use serde_json::{Value, json};

use super::*;
use crate::domain::model::{CleanupOutcome, EpochMillis};
use crate::domain::ports::fakes::{FakeFileSystem, FakeTrash, FixedClock, InMemoryStore};
use crate::domain::ports::fakes_content::FakeContentReader;
use crate::domain::ports::{CleanupLogStore, DuplicateStore, ScanStore};
use crate::domain::privileges::{MismatchReason, UserMatch};
use crate::domain::protection::service::DeleteTarget;
use crate::domain::protection::test_support::raw_c_ana;

const NOW: EpochMillis = EpochMillis(1_800_000_000_000);
const HOUR: i64 = 3_600_000;
const SIZE: usize = 1_200_000;

fn matched() -> PrivilegeContext {
    PrivilegeContext::new(UserMatch::Match, true)
}

fn mismatched() -> PrivilegeContext {
    PrivilegeContext::new(UserMatch::Mismatch(MismatchReason::DifferentUser), true)
}

fn undetermined() -> PrivilegeContext {
    PrivilegeContext::new(UserMatch::Mismatch(MismatchReason::Undetermined), true)
}

fn bytes(tag: u8) -> Vec<u8> {
    (0..SIZE)
        .map(|i| u8::try_from((i * 7 + usize::from(tag) * 13 + i / 331) % 251).unwrap_or(0))
        .collect()
}

fn path(folder: &str, name: &str) -> String {
    format!(r"C:\Users\ana\{folder}\{name}")
}

/// Comparte un `FakeTrash` para poder inspeccionarlo despues de que el comando lo use.
struct SharedTrash(Rc<FakeTrash>);

impl Trash for SharedTrash {
    fn preflight(&self, sample: &Path) -> Result<(), TrashError> {
        self.0.preflight(sample)
    }

    fn move_to_trash(&self, target: &DeleteTarget) -> Result<(), TrashError> {
        self.0.move_to_trash(target)
    }
}

/// Archivos del escenario: `(carpeta, nombre, etiqueta de contenido, horas de antiguedad)`.
type Spec = (&'static str, &'static str, u8, i64);

struct Env {
    store: InMemoryStore,
    fs: Arc<FakeFileSystem>,
    reader: FakeContentReader,
    trash: Rc<FakeTrash>,
    clock: FixedClock,
    active: ActiveJob,
    previews: CleanupPreviews,
}

impl Env {
    fn new(files: &[Spec]) -> Self {
        let mut fs = FakeFileSystem::new()
            .canonical_as_is()
            .with_dir(r"C:\Users")
            .with_dir(r"C:\Users\ana");
        for folder in [
            "Desktop",
            "Documents",
            "Downloads",
            "Music",
            "Pictures",
            "Videos",
        ] {
            fs = fs.with_dir(&format!(r"C:\Users\ana\{folder}"));
        }
        for (folder, name, _, hours) in files {
            fs = fs.with_file(
                &path(folder, name),
                SIZE as u64,
                Some(EpochMillis(NOW.0 - hours * HOUR)),
            );
        }
        let fs = Arc::new(fs);
        let mut reader = FakeContentReader::new(Arc::clone(&fs));
        for (folder, name, tag, _) in files {
            reader = reader.with_content(&path(folder, name), bytes(*tag));
        }
        Self {
            store: InMemoryStore::new(),
            trash: Rc::new(FakeTrash::new(Arc::clone(&fs))),
            fs,
            reader,
            clock: FixedClock(NOW),
            active: ActiveJob::default(),
            previews: CleanupPreviews::default(),
        }
    }

    fn ports(
        &self,
    ) -> DuplicatePorts<'_, InMemoryStore, FakeFileSystem, FakeContentReader, FixedClock> {
        DuplicatePorts {
            store: &self.store,
            fs: &self.fs,
            reader: &self.reader,
            clock: &self.clock,
            previews: &self.previews,
            active: &self.active,
        }
    }

    fn scan_with(
        &self,
        privileges: &PrivilegeContext,
        roots: Option<Vec<String>>,
        min: Option<u64>,
        emit: &mut dyn FnMut(DuplicateScanEventDto),
    ) -> Result<DuplicateScanSummaryDto, AppError> {
        run_duplicate_scan(&self.ports(), privileges, raw_c_ana(), roots, min, emit)
    }

    fn scan(&self) -> DuplicateScanSummaryDto {
        self.scan_with(&matched(), None, None, &mut |_| {}).unwrap()
    }

    fn groups(&self, scan: i64) -> DuplicateGroupPageDto {
        list_groups(&self.store, scan, 0, 100, None, None, &FolderPaths::new()).unwrap()
    }

    fn preview_with(
        &self,
        privileges: &PrivilegeContext,
        scan_id: i64,
        decisions: &[DecisionArg],
    ) -> Result<DuplicateCleanupPreviewDto, AppError> {
        let trash = Rc::clone(&self.trash);
        create_duplicate_preview(
            &self.ports(),
            privileges,
            || Ok(SharedTrash(trash)),
            scan_id,
            decisions,
        )
    }

    fn preview(
        &self,
        scan_id: i64,
        decisions: &[DecisionArg],
    ) -> Result<DuplicateCleanupPreviewDto, AppError> {
        self.preview_with(&matched(), scan_id, decisions)
    }

    fn confirm_with(
        &self,
        privileges: &PrivilegeContext,
        preview_id: i64,
        emit: &mut dyn FnMut(CleanupEventDto),
    ) -> Result<DuplicateCleanupSummaryDto, AppError> {
        let trash = Rc::clone(&self.trash);
        confirm_duplicate_cleanup_run(
            &self.ports(),
            privileges,
            raw_c_ana(),
            || Ok(SharedTrash(trash)),
            preview_id,
            emit,
        )
    }

    fn confirm(&self, preview_id: i64) -> Result<DuplicateCleanupSummaryDto, AppError> {
        self.confirm_with(&matched(), preview_id, &mut |_| {})
    }

    fn log(&self) -> Vec<crate::domain::model::CleanupLogEntry> {
        let mut entries = self.store.recent_cleanup_log(1_000).unwrap();
        entries.reverse();
        entries
    }
}

fn pair() -> Vec<Spec> {
    vec![
        ("Downloads", "foto.jpg", 1, 72),
        ("Pictures", "foto (1).jpg", 1, 72),
    ]
}

fn decide(group_id: i64, keep: &[i64]) -> DecisionArg {
    DecisionArg {
        group_id,
        keep_file_ids: keep.to_vec(),
    }
}

fn is_kind<T: std::fmt::Debug>(result: Result<T, AppError>, kind: &str) -> bool {
    match result {
        Ok(_) => false,
        Err(error) => serde_json::to_value(&error).unwrap()["kind"] == kind,
    }
}

fn kind_of<T: std::fmt::Debug>(result: Result<T, AppError>) -> String {
    serde_json::to_value(result.unwrap_err()).unwrap()["kind"]
        .as_str()
        .unwrap()
        .to_owned()
}

// ---------------------------------------------------------------- escaneo

#[test]
fn start_scan_should_find_the_group_and_answer_the_exact_summary_shape() {
    let env = Env::new(&pair());
    let dto = env.scan();
    assert_eq!(dto.status, "done");
    assert_eq!((dto.group_count, dto.duplicate_file_count), (1, 2));
    assert_eq!(dto.reclaimable_bytes, SIZE as u64);
    assert_eq!(dto.open_group_count, 1);
    assert_eq!(dto.min_size_bytes, 1_048_576);
    assert_eq!(dto.roots.len(), 6);
    assert_eq!(dto.files_seen, 2);
    let value = serde_json::to_value(&dto).unwrap();
    let mut keys: Vec<&str> = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "belowMinSize",
            "changedCount",
            "cloudCount",
            "duplicateFileCount",
            "excludedDirs",
            "filesSeen",
            "finishedAt",
            "groupCount",
            "hardLinkedCount",
            "hashedBytes",
            "inaccessibleDirs",
            "minSizeBytes",
            "openGroupCount",
            "protectedCount",
            "reclaimableBytes",
            "roots",
            "scanId",
            "startedAt",
            "status",
            "unreadableCount",
        ]
    );
    assert_eq!(
        value["roots"][2],
        json!({"root": "downloads", "status": "done", "reason": null, "filesSeen": 1})
    );
    // Solo lectura: nada se movio ni se registro.
    assert!(env.trash.moved().is_empty());
    assert!(env.log().is_empty());
}

#[test]
fn start_scan_should_stream_started_and_one_event_per_stage() {
    let env = Env::new(&pair());
    let mut events = Vec::new();
    let dto = env
        .scan_with(&matched(), None, None, &mut |event| events.push(event))
        .unwrap();
    assert_eq!(
        events[0],
        DuplicateScanEventDto::Started {
            scan_id: dto.scan_id
        }
    );
    let tags: Vec<String> = events
        .iter()
        .map(|e| {
            serde_json::to_value(e).unwrap()["type"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert_eq!(
        tags,
        ["started", "walking", "partial", "partial", "full", "full"]
    );
    assert_eq!(
        serde_json::to_value(&events[1]).unwrap(),
        json!({"type": "walking", "filesSeen": 2})
    );
}

#[test]
fn start_scan_should_honor_the_requested_roots_and_minimum_size() {
    let env = Env::new(&pair());
    let only_pictures = env
        .scan_with(&matched(), Some(vec!["pictures".into()]), None, &mut |_| {})
        .unwrap();
    assert_eq!(only_pictures.group_count, 0);
    assert_eq!(only_pictures.files_seen, 1);
    let big_minimum = env
        .scan_with(&matched(), None, Some(10_485_760), &mut |_| {})
        .unwrap();
    assert_eq!(big_minimum.group_count, 0);
    assert_eq!(big_minimum.min_size_bytes, 10_485_760);
    assert_eq!(big_minimum.below_min_size, 2);
}

#[test]
fn start_scan_should_reject_invalid_arguments_without_creating_a_scan() {
    let env = Env::new(&pair());
    for (roots, min) in [
        (
            Some(vec!["downloads".to_string(), "appdata".to_string()]),
            None,
        ),
        (
            Some(vec!["downloads".to_string(), "downloads".to_string()]),
            None,
        ),
        (Some(vec![]), None),
        (None, Some(5)),
    ] {
        assert!(is_kind(
            env.scan_with(&matched(), roots, min, &mut |_| {}),
            "invalid_argument"
        ));
    }
    assert!(env.store.list_scans().unwrap().is_empty());
    assert!(env.reader.opened().is_empty());
}

#[test]
fn start_scan_should_be_blocked_in_a_mismatch_without_creating_a_scan_or_reading_anything() {
    let env = Env::new(&pair());
    for (privileges, reason) in [
        (mismatched(), "different_user"),
        (undetermined(), "undetermined"),
    ] {
        let error = env
            .scan_with(&privileges, None, None, &mut |_| {})
            .unwrap_err();
        let value = serde_json::to_value(&error).unwrap();
        assert_eq!(value["kind"], "duplicates_blocked");
        assert!(value["message"].as_str().unwrap().ends_with(reason));
    }
    assert!(env.store.list_scans().unwrap().is_empty());
    assert!(env.reader.opened().is_empty());
    assert_eq!(env.fs.total_calls(), 0);
}

#[test]
fn start_scan_should_fail_with_the_error_of_the_active_job_without_affecting_it() {
    let env = Env::new(&pair());
    let scan = env.active.begin_scan().unwrap();
    assert!(is_kind(
        env.scan_with(&matched(), None, None, &mut |_| {}),
        "scan_in_progress"
    ));
    drop(scan);
    let cleanup = env.active.begin_cleanup(PreviewId(1)).unwrap();
    assert!(is_kind(
        env.scan_with(&matched(), None, None, &mut |_| {}),
        "cleanup_in_progress"
    ));
    drop(cleanup);
    let empty = env.active.begin_empty().unwrap();
    assert!(is_kind(
        env.scan_with(&matched(), None, None, &mut |_| {}),
        "recycle_bin_empty_in_progress"
    ));
    drop(empty);
    assert!(env.store.list_scans().unwrap().is_empty());
    // Con el lugar libre vuelve a funcionar.
    assert_eq!(env.scan().group_count, 1);
}

#[test]
fn start_scan_should_release_the_slot_when_it_finishes() {
    let env = Env::new(&pair());
    env.scan();
    assert!(env.active.ensure_idle().is_ok());
}

#[test]
fn start_scan_should_end_cancelled_without_groups_when_cancelled_through_cancel_scan() {
    let env = Env::new(&pair());
    let active = env.active.clone();
    let dto = env
        .scan_with(&matched(), None, None, &mut |event| {
            if let DuplicateScanEventDto::Started { scan_id } = event {
                active.cancel_scan(ScanId(scan_id)).unwrap();
            }
        })
        .unwrap();
    assert_eq!(dto.status, "cancelled");
    assert_eq!((dto.group_count, dto.roots.len()), (0, 0));
    assert_eq!(
        env.store
            .get_scan(ScanId(dto.scan_id))
            .unwrap()
            .unwrap()
            .status,
        ScanStatus::Cancelled
    );
    assert_eq!(last_duplicate_scan(&env.store).unwrap(), None);
    assert!(env.reader.opened().is_empty());
}

#[test]
fn start_scan_should_close_the_scan_as_error_when_saving_fails() {
    // Un grupo invalido del escaner es imposible; se fuerza el fallo con un escaneo ya cerrado.
    let env = Env::new(&pair());
    let guard = env.active.begin_scan().unwrap();
    drop(guard);
    // Se cierra el escaneo recien creado antes de guardar: `complete_duplicate_scan` falla.
    let store = &env.store;
    let result = env.scan_with(&matched(), None, None, &mut |event| {
        if let DuplicateScanEventDto::Started { scan_id } = event {
            store
                .finish_scan(ScanId(scan_id), ScanStatus::Error, NOW)
                .unwrap();
        }
    });
    assert!(result.is_err());
    assert!(env.active.ensure_idle().is_ok());
}

// ---------------------------------------------------------------- consulta

#[test]
fn get_last_duplicate_scan_should_be_null_until_a_scan_finishes() {
    let env = Env::new(&pair());
    assert_eq!(last_duplicate_scan(&env.store).unwrap(), None);
    let dto = env.scan();
    assert_eq!(last_duplicate_scan(&env.store).unwrap().unwrap(), dto);
}

#[test]
fn list_groups_should_answer_the_exact_group_shape_with_relative_paths_and_no_kept_copy() {
    let env = Env::new(&pair());
    let scan = env.scan().scan_id;
    let mut folders = FolderPaths::new();
    folders.insert(
        ContentFolder::Downloads,
        normalize_lexical(r"C:\Users\ana\Downloads").unwrap(),
    );
    folders.insert(
        ContentFolder::Pictures,
        normalize_lexical(r"C:\Users\ana\Pictures").unwrap(),
    );
    let page = list_groups(&env.store, scan, 0, 20, None, None, &folders).unwrap();
    assert_eq!((page.scan_id, page.total), (scan, 1));
    let group = &page.groups[0];
    assert_eq!(group.size_bytes, SIZE as u64);
    assert_eq!(group.blake3.len(), 64);
    assert_eq!(group.reclaimable_bytes, SIZE as u64);
    assert_eq!(group.resolved_at, None);
    let relative: Vec<&str> = group
        .files
        .iter()
        .map(|f| f.relative_path.as_str())
        .collect();
    assert_eq!(relative, ["foto.jpg", "foto (1).jpg"]);
    assert_eq!(group.files[0].root, "downloads");
    assert_eq!(group.files[0].path, r"C:\Users\ana\Downloads\foto.jpg");
    assert!(group.files.iter().all(|f| !f.kept));
    let value = serde_json::to_value(&page).unwrap();
    assert_eq!(
        value["groups"][0]["files"][0]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>(),
        [
            "fileId",
            "kept",
            "modifiedAt",
            "path",
            "relativePath",
            "root"
        ]
        .into_iter()
        .collect()
    );
}

#[test]
fn list_groups_should_fall_back_to_the_full_path_without_a_known_root() {
    let env = Env::new(&pair());
    let scan = env.scan().scan_id;
    let page = env.groups(scan);
    assert_eq!(
        page.groups[0].files[0].relative_path,
        r"C:\Users\ana\Downloads\foto.jpg"
    );
}

#[test]
fn list_groups_should_filter_by_root_and_minimum_size_and_validate_arguments() {
    let env = Env::new(&pair());
    let scan = env.scan().scan_id;
    let none = FolderPaths::new();
    let by_root = |root: &str| list_groups(&env.store, scan, 0, 10, Some(root), None, &none);
    assert_eq!(by_root("pictures").unwrap().total, 1);
    assert_eq!(by_root("music").unwrap().total, 0);
    assert!(is_kind(by_root("appdata"), "invalid_argument"));
    let by_min = |min: u64| list_groups(&env.store, scan, 0, 10, None, Some(min), &none);
    assert_eq!(by_min(1_048_576).unwrap().total, 1);
    assert_eq!(by_min(10_485_760).unwrap().total, 0);
    assert!(is_kind(by_min(5), "invalid_argument"));
    for (offset, limit) in [(0, 0), (0, 201), (0, -1), (-1, 10)] {
        assert!(
            is_kind(
                list_groups(&env.store, scan, offset, limit, None, None, &none),
                "invalid_argument"
            ),
            "offset {offset} limit {limit}"
        );
    }
    assert!(list_groups(&env.store, scan, 0, 200, None, None, &none).is_ok());
    assert!(is_kind(
        list_groups(&env.store, scan + 1, 0, 10, None, None, &none),
        "scan_not_found"
    ));
    assert!(is_kind(
        list_groups(&env.store, -1, 0, 10, None, None, &none),
        "invalid_argument"
    ));
    let beyond = list_groups(&env.store, scan, 500, 10, None, None, &none).unwrap();
    assert_eq!((beyond.total, beyond.groups.len()), (1, 0));
}

// ---------------------------------------------------------------- preview

#[test]
fn preview_should_report_groups_copies_and_bytes_and_leave_every_file_in_place() {
    let env = Env::new(&pair());
    let scan = env.scan();
    let page = env.groups(scan.scan_id);
    let group = &page.groups[0];
    let dto = env
        .preview(
            scan.scan_id,
            &[decide(group.group_id, &[group.files[0].file_id])],
        )
        .unwrap();
    assert_eq!(dto.group_count, 1);
    assert_eq!(dto.preview.total_items, 1);
    assert_eq!(dto.preview.total_bytes, SIZE as u64);
    assert!(dto.preview.recycle_bin_available);
    assert_eq!(dto.preview.scan_id, scan.scan_id);
    assert_eq!(dto.preview.categories[0].category, "duplicates");
    let value = serde_json::to_value(&dto).unwrap();
    for key in [
        "previewId",
        "scanId",
        "createdAt",
        "expiresAt",
        "totalItems",
        "totalBytes",
        "recycleBinAvailable",
        "categories",
        "groupCount",
    ] {
        assert!(value.get(key).is_some(), "{key}");
    }
    let text = value.to_string();
    assert!(!text.contains("foto"), "sin rutas en el preview: {text}");
    assert!(env.fs.exists(Path::new(&path("Pictures", "foto (1).jpg"))));
    assert!(env.trash.moved().is_empty());
}

#[test]
fn preview_should_reject_an_attempt_to_delete_every_copy() {
    let env = Env::new(&pair());
    let scan = env.scan();
    let group_id = env.groups(scan.scan_id).groups[0].group_id;
    assert!(is_kind(
        env.preview(scan.scan_id, &[decide(group_id, &[])]),
        "duplicate_keep_required"
    ));
}

#[test]
fn preview_should_map_the_decision_and_scan_errors() {
    let env = Env::new(&pair());
    let scan = env.scan();
    let group = env.groups(scan.scan_id).groups.remove(0);
    let (a, b) = (group.files[0].file_id, group.files[1].file_id);
    assert!(is_kind(
        env.preview(scan.scan_id, &[decide(group.group_id, &[a, b])]),
        "invalid_argument"
    ));
    assert!(is_kind(
        env.preview(scan.scan_id, &[decide(group.group_id, &[99_999])]),
        "invalid_argument"
    ));
    assert!(is_kind(env.preview(scan.scan_id, &[]), "invalid_argument"));
    assert!(is_kind(
        env.preview(
            scan.scan_id,
            &[decide(group.group_id, &[a]), decide(group.group_id, &[b])]
        ),
        "invalid_argument"
    ));
    assert!(is_kind(
        env.preview(scan.scan_id, &[decide(4_242, &[a])]),
        "duplicate_group_not_found"
    ));
    assert!(is_kind(
        env.preview(scan.scan_id + 1, &[decide(group.group_id, &[a])]),
        "scan_not_found"
    ));
}

#[test]
fn preview_should_reject_ids_outside_the_javascript_safe_range() {
    let env = Env::new(&pair());
    let scan = env.scan();
    for decisions in [
        vec![decide(-1, &[1])],
        vec![decide((1_i64 << 53) + 1, &[1])],
        vec![decide(1, &[-5])],
        vec![decide(1, &[1_i64 << 60])],
    ] {
        assert!(is_kind(
            env.preview(scan.scan_id, &decisions),
            "invalid_argument"
        ));
    }
    assert!(is_kind(
        env.preview(-1, &[decide(1, &[1])]),
        "invalid_argument"
    ));
    assert!(is_kind(
        env.preview(1_i64 << 53, &[decide(1, &[1])]),
        "invalid_argument"
    ));
}

#[test]
fn preview_should_be_blocked_in_a_mismatch_and_create_no_preview() {
    let env = Env::new(&pair());
    let scan = env.scan();
    let group = env.groups(scan.scan_id).groups.remove(0);
    for (privileges, reason) in [
        (mismatched(), "different_user"),
        (undetermined(), "undetermined"),
    ] {
        let error = env
            .preview_with(
                &privileges,
                scan.scan_id,
                &[decide(group.group_id, &[group.files[0].file_id])],
            )
            .unwrap_err();
        let value = serde_json::to_value(&error).unwrap();
        assert_eq!(value["kind"], "duplicates_blocked");
        assert!(value["message"].as_str().unwrap().ends_with(reason));
    }
    // Ningun preview quedo guardado.
    assert!(is_kind(env.confirm(1), "preview_not_found"));
}

#[test]
fn preview_should_fail_while_a_job_is_running_without_taking_its_place() {
    let env = Env::new(&pair());
    let scan = env.scan();
    let group = env.groups(scan.scan_id).groups.remove(0);
    let decisions = [decide(group.group_id, &[group.files[0].file_id])];
    let job = env.active.begin_scan().unwrap();
    assert_eq!(
        kind_of(env.preview(scan.scan_id, &decisions)),
        "scan_in_progress"
    );
    drop(job);
    let job = env.active.begin_cleanup(PreviewId(9)).unwrap();
    assert_eq!(
        kind_of(env.preview(scan.scan_id, &decisions)),
        "cleanup_in_progress"
    );
    drop(job);
    let job = env.active.begin_empty().unwrap();
    assert_eq!(
        kind_of(env.preview(scan.scan_id, &decisions)),
        "recycle_bin_empty_in_progress"
    );
    drop(job);
    assert!(env.active.ensure_idle().is_ok());
    assert!(env.preview(scan.scan_id, &decisions).is_ok());
}

#[test]
fn preview_should_flag_the_recycle_bin_as_unavailable_when_the_preflight_fails() {
    let env = Env::new(&pair());
    let scan = env.scan();
    let group = env.groups(scan.scan_id).groups.remove(0);
    let decisions = [decide(group.group_id, &[group.files[0].file_id])];
    let failing = FakeTrash::new(Arc::clone(&env.fs)).preflight_fails(TrashError::BinUnavailable);
    let failing = Rc::new(failing);
    let dto = create_duplicate_preview(
        &env.ports(),
        &matched(),
        || Ok(SharedTrash(failing)),
        scan.scan_id,
        &decisions,
    )
    .unwrap();
    assert!(!dto.preview.recycle_bin_available);
}

#[test]
fn preview_should_exclude_a_copy_written_within_24_hours_and_report_it() {
    let env = Env::new(&[
        ("Downloads", "foto.jpg", 1, 72),
        ("Pictures", "foto (1).jpg", 1, 2),
    ]);
    let scan = env.scan();
    let group = env.groups(scan.scan_id).groups.remove(0);
    let dto = env
        .preview(
            scan.scan_id,
            &[decide(group.group_id, &[group.files[0].file_id])],
        )
        .unwrap();
    assert_eq!(dto.preview.total_items, 0);
    assert_eq!(dto.preview.categories[0].recent_count, 1);
    assert!(is_kind(
        env.confirm(dto.preview.preview_id as i64),
        "nothing_to_clean"
    ));
}

// ---------------------------------------------------------------- confirmacion

fn three_groups() -> Vec<Spec> {
    vec![
        ("Downloads", "a.bin", 1, 72),
        ("Pictures", "a (1).bin", 1, 72),
        ("Downloads", "b.bin", 2, 72),
        ("Documents", "b copia.bin", 2, 72),
        ("Music", "c.bin", 3, 72),
        ("Videos", "c copia.bin", 3, 72),
    ]
}

fn keep_first_of_each(env: &Env, scan: i64) -> (Vec<DecisionArg>, Vec<DuplicateGroupDto>) {
    let groups = env.groups(scan).groups;
    (
        groups
            .iter()
            .map(|g| decide(g.group_id, &[g.files[0].file_id]))
            .collect(),
        groups,
    )
}

#[test]
fn confirm_should_move_the_chosen_copies_keep_the_others_log_and_resolve_the_groups() {
    let env = Env::new(&three_groups());
    let scan = env.scan().scan_id;
    let (decisions, groups) = keep_first_of_each(&env, scan);
    let preview = env.preview(scan, &decisions).unwrap();
    assert_eq!((preview.group_count, preview.preview.total_items), (3, 3));

    let events = RefCell::new(Vec::new());
    let summary = env
        .confirm_with(&matched(), preview.preview.preview_id as i64, &mut |e| {
            events.borrow_mut().push(e);
        })
        .unwrap();

    assert_eq!(summary.summary.status, "completed");
    assert_eq!(summary.summary.moved_count, 3);
    assert_eq!(summary.summary.moved_bytes, 3 * SIZE as u64);
    assert!(summary.groups_resolved);
    let mut resolved = summary.resolved_group_ids.clone();
    resolved.sort_unstable();
    let mut expected: Vec<i64> = groups.iter().map(|g| g.group_id).collect();
    expected.sort_unstable();
    assert_eq!(resolved, expected);
    // Las copias conservadas siguen y las otras estan en la Papelera.
    for group in &groups {
        assert!(
            env.fs.exists(Path::new(&group.files[0].path)),
            "{}",
            group.files[0].path
        );
        assert!(
            !env.fs.exists(Path::new(&group.files[1].path)),
            "{}",
            group.files[1].path
        );
    }
    assert_eq!(env.trash.moved().len(), 3);
    // Log append-only con la categoria y el escaneo de duplicados.
    let log = env.log();
    assert_eq!(log.len(), 3);
    assert!(log.iter().all(|e| e.category == "duplicates"
        && e.outcome == CleanupOutcome::MovedToTrash
        && e.scan_id == Some(ScanId(scan))));
    // Progreso por item: started y un progress por copia.
    let events = events.into_inner();
    assert_eq!(
        events[0],
        CleanupEventDto::Started {
            preview_id: preview.preview.preview_id,
            total: 3
        }
    );
    let processed: Vec<u64> = events
        .iter()
        .filter_map(|e| match e {
            CleanupEventDto::Progress { processed, .. } => Some(*processed),
            CleanupEventDto::Started { .. } => None,
        })
        .collect();
    assert_eq!(processed, [1, 2, 3]);
    // Resueltos: con la marca de conservada solo en la elegida y sin nueva decision posible.
    let after = env.groups(scan).groups;
    assert!(after.iter().all(|g| g.resolved_at.is_some()));
    for (group, before) in after.iter().zip(&groups) {
        let kept: Vec<i64> = group
            .files
            .iter()
            .filter(|f| f.kept)
            .map(|f| f.file_id)
            .collect();
        assert_eq!(kept, [before.files[0].file_id]);
    }
    assert!(is_kind(
        env.preview(scan, &decisions),
        "duplicate_group_resolved"
    ));
    let summary_after = last_duplicate_scan(&env.store).unwrap().unwrap();
    assert_eq!(
        (summary_after.group_count, summary_after.open_group_count),
        (3, 0)
    );
    assert_eq!(summary_after.reclaimable_bytes, 0);
    // El resumen de ejecucion no lleva rutas ni identidades.
    let text = serde_json::to_value(&summary).unwrap().to_string();
    assert!(!text.contains("Users") && !text.contains("bin\""), "{text}");
}

#[test]
fn confirm_summary_should_serialize_the_standard_summary_plus_the_resolution() {
    let env = Env::new(&pair());
    let scan = env.scan().scan_id;
    let (decisions, _) = keep_first_of_each(&env, scan);
    let preview = env.preview(scan, &decisions).unwrap();
    let summary = env.confirm(preview.preview.preview_id as i64).unwrap();
    let value = serde_json::to_value(&summary).unwrap();
    for key in [
        "previewId",
        "scanId",
        "status",
        "abortReason",
        "movedCount",
        "movedBytes",
        "reasons",
        "categories",
        "groupsResolved",
        "resolvedGroupIds",
    ] {
        assert!(value.get(key).is_some(), "{key}");
    }
    assert_eq!(value["categories"][0]["category"], "duplicates");
    assert_eq!(value["groupsResolved"], Value::Bool(true));
}

#[test]
fn confirm_should_consume_the_preview_so_a_second_confirmation_is_not_found() {
    let env = Env::new(&pair());
    let scan = env.scan().scan_id;
    let (decisions, _) = keep_first_of_each(&env, scan);
    let id = env.preview(scan, &decisions).unwrap().preview.preview_id as i64;
    assert!(env.confirm(id).is_ok());
    assert!(is_kind(env.confirm(id), "preview_not_found"));
    assert!(env.active.ensure_idle().is_ok());
}

#[test]
fn the_junk_confirmation_should_not_accept_a_duplicates_preview_and_keeps_it() {
    use crate::commands::cleanup::{CleanupPorts, confirm_cleanup_run};
    let env = Env::new(&pair());
    let scan = env.scan().scan_id;
    let (decisions, _) = keep_first_of_each(&env, scan);
    let id = env.preview(scan, &decisions).unwrap().preview.preview_id as i64;
    let junk_ports = CleanupPorts {
        store: &env.store,
        fs: &*env.fs,
        clock: &env.clock,
        previews: &env.previews,
        active: &env.active,
    };
    let trash = Rc::clone(&env.trash);
    let result = confirm_cleanup_run(
        &junk_ports,
        &matched(),
        raw_c_ana(),
        || Ok(SharedTrash(trash)),
        id,
        &mut |_| {},
    );
    assert!(is_kind(result, "preview_not_found"));
    assert!(env.trash.moved().is_empty());
    // El preview de duplicados sigue disponible.
    assert!(env.confirm(id).is_ok());
}

#[test]
fn confirm_should_fail_with_preview_stale_when_another_scan_finished_after_the_preview() {
    let env = Env::new(&pair());
    let scan = env.scan().scan_id;
    let (decisions, _) = keep_first_of_each(&env, scan);
    let id = env.preview(scan, &decisions).unwrap().preview.preview_id as i64;
    let second = env.scan();
    assert_ne!(second.scan_id, scan);
    assert!(is_kind(env.confirm(id), "preview_stale"));
    assert!(env.trash.moved().is_empty());
    assert!(env.log().is_empty());
}

#[test]
fn confirm_should_fail_with_preview_expired_after_ten_minutes() {
    let env = Env::new(&pair());
    let scan = env.scan().scan_id;
    let (decisions, _) = keep_first_of_each(&env, scan);
    let id = env.preview(scan, &decisions).unwrap().preview.preview_id as i64;
    let later = FixedClock(EpochMillis(NOW.0 + 10 * 60 * 1000 + 1));
    let ports = DuplicatePorts {
        store: &env.store,
        fs: &*env.fs,
        reader: &env.reader,
        clock: &later,
        previews: &env.previews,
        active: &env.active,
    };
    let trash = Rc::clone(&env.trash);
    let result = confirm_duplicate_cleanup_run(
        &ports,
        &matched(),
        raw_c_ana(),
        || Ok(SharedTrash(trash)),
        id,
        &mut |_| {},
    );
    assert!(is_kind(result, "preview_expired"));
    assert!(env.trash.moved().is_empty());
}

#[test]
fn confirm_should_fail_while_another_job_runs_without_consuming_the_preview() {
    let env = Env::new(&pair());
    let scan = env.scan().scan_id;
    let (decisions, _) = keep_first_of_each(&env, scan);
    let id = env.preview(scan, &decisions).unwrap().preview.preview_id as i64;
    let job = env.active.begin_scan().unwrap();
    assert_eq!(kind_of(env.confirm(id)), "scan_in_progress");
    drop(job);
    let job = env.active.begin_empty().unwrap();
    assert_eq!(kind_of(env.confirm(id)), "recycle_bin_empty_in_progress");
    drop(job);
    assert!(env.confirm(id).is_ok());
}

#[test]
fn confirm_should_reevaluate_privileges_and_move_nothing_on_a_mismatch() {
    let env = Env::new(&pair());
    let scan = env.scan().scan_id;
    let (decisions, groups) = keep_first_of_each(&env, scan);
    let id = env.preview(scan, &decisions).unwrap().preview.preview_id as i64;
    let summary = env.confirm_with(&mismatched(), id, &mut |_| {}).unwrap();
    assert_eq!(summary.summary.moved_count, 0);
    assert_eq!(summary.summary.skipped_protected, 1);
    let log = env.log();
    assert_eq!(
        log[0].error_detail.as_deref(),
        Some("blocked_different_user")
    );
    assert_eq!(log[0].category, "duplicates");
    assert!(env.fs.exists(Path::new(&groups[0].files[1].path)));
    assert!(env.trash.moved().is_empty());
}

#[test]
fn confirm_should_skip_a_copy_whose_content_changed_and_still_resolve_the_group() {
    let env = Env::new(&pair());
    let scan = env.scan().scan_id;
    let (decisions, groups) = keep_first_of_each(&env, scan);
    let id = env.preview(scan, &decisions).unwrap().preview.preview_id as i64;
    // Contenido distinto con el mismo tamano y fecha.
    env.reader
        .replace_content(Path::new(&groups[0].files[1].path), bytes(9));
    let summary = env.confirm(id).unwrap();
    assert_eq!(summary.summary.moved_count, 0);
    assert_eq!(summary.summary.skipped_locked, 1);
    assert_eq!(summary.summary.reasons[0].detail, "content_changed");
    // Un grupo con un item procesado (omitido) queda resuelto: hay que buscar de nuevo.
    assert!(summary.groups_resolved);
    assert_eq!(summary.resolved_group_ids, [groups[0].group_id]);
    assert!(env.fs.exists(Path::new(&groups[0].files[1].path)));
}

#[test]
fn confirm_should_protect_a_group_whose_kept_copy_vanished_and_go_on_with_the_others() {
    let env = Env::new(&three_groups());
    let scan = env.scan().scan_id;
    let (decisions, groups) = keep_first_of_each(&env, scan);
    let id = env.preview(scan, &decisions).unwrap().preview.preview_id as i64;
    // El usuario borro la copia que habia elegido conservar del primer grupo.
    env.fs.remove(Path::new(&groups[0].files[0].path));
    let summary = env.confirm(id).unwrap();
    assert_eq!(summary.summary.moved_count, 2);
    assert_eq!(summary.summary.skipped_protected, 1);
    assert!(
        summary
            .summary
            .reasons
            .iter()
            .any(|r| r.detail == "kept_copy_unavailable")
    );
    assert!(env.fs.exists(Path::new(&groups[0].files[1].path)));
}

#[test]
fn confirm_should_resolve_only_the_groups_reached_when_cancelled() {
    let env = Env::new(&three_groups());
    let scan = env.scan().scan_id;
    let (decisions, groups) = keep_first_of_each(&env, scan);
    let id = env.preview(scan, &decisions).unwrap().preview.preview_id as i64;
    let active = env.active.clone();
    let summary = env
        .confirm_with(&matched(), id, &mut |event| {
            // Se cancela despues de procesar la copia del primer grupo.
            if let CleanupEventDto::Progress { processed: 1, .. } = event {
                active.cancel_cleanup(PreviewId(id as u64)).unwrap();
            }
        })
        .unwrap();
    assert_eq!(summary.summary.status, "cancelled");
    assert_eq!(
        (summary.summary.processed, summary.summary.not_processed),
        (1, 2)
    );
    assert!(summary.groups_resolved);
    assert_eq!(summary.resolved_group_ids.len(), 1);
    let open: Vec<i64> = env
        .groups(scan)
        .groups
        .iter()
        .filter(|g| g.resolved_at.is_none())
        .map(|g| g.group_id)
        .collect();
    assert_eq!(open.len(), 2);
    assert!(!open.contains(&summary.resolved_group_ids[0]));
    // Los grupos sin procesar siguen sin marcas y disponibles para decidir.
    let untouched = env.groups(scan);
    assert!(
        untouched
            .groups
            .iter()
            .filter(|g| g.resolved_at.is_none())
            .all(|g| g.files.iter().all(|f| !f.kept))
    );
    assert_eq!(groups.len(), 3);
}

#[test]
fn confirm_should_report_unresolved_groups_when_the_resolution_cannot_be_saved() {
    let env = Env::new(&pair());
    let scan = env.scan().scan_id;
    let (decisions, groups) = keep_first_of_each(&env, scan);
    let id = env.preview(scan, &decisions).unwrap().preview.preview_id as i64;
    // El grupo se resuelve por otro camino entre el preview y la confirmacion: guardar la
    // resolucion falla, pero la ejecucion ya ocurrio y se informa igual.
    env.store
        .resolve_duplicate_groups(
            ScanId(scan),
            &[crate::domain::duplicates::model::GroupResolution {
                group_id: groups[0].group_id,
                kept_file_ids: vec![groups[0].files[0].file_id],
            }],
            NOW,
        )
        .unwrap();
    let summary = env.confirm(id).unwrap();
    assert_eq!(summary.summary.moved_count, 1);
    assert!(!summary.groups_resolved);
    assert!(summary.resolved_group_ids.is_empty());
}

#[test]
fn confirm_should_reject_ids_outside_the_safe_range_and_release_the_slot() {
    let env = Env::new(&pair());
    for bad in [-1, 1_i64 << 53] {
        assert!(is_kind(env.confirm(bad), "invalid_argument"));
    }
    assert!(is_kind(env.confirm(404), "preview_not_found"));
    assert!(env.active.ensure_idle().is_ok());
}

#[test]
fn cancel_commands_should_work_over_the_duplicate_jobs() {
    let env = Env::new(&pair());
    // Sin trabajo en curso: los errores de siempre.
    assert!(is_kind(env.active.cancel_scan(ScanId(1)), "scan_not_found"));
    assert!(is_kind(
        env.active.cancel_cleanup(PreviewId(1)),
        "preview_not_found"
    ));
}

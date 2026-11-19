use std::cell::RefCell;
use std::path::PathBuf;

use serde_json::{Value, json};

use super::*;
use crate::domain::disk_usage::model::RevealReason;
use crate::domain::model::{EpochMillis, ScanId};
use crate::domain::ports::fakes::{FakeFileSystem, FixedClock, InMemoryStore};
use crate::domain::ports::fakes_shell::{FakeExplorer, FakeFolderPicker, FakeVolumeSpace};
use crate::domain::ports::{PortError, ScanStore};
use crate::domain::privileges::{MismatchReason, UserMatch};
use crate::domain::protection::test_support::raw_c_ana;

const NOW: EpochMillis = EpochMillis(1_800_000_000_000);
const MIB: u64 = 1024 * 1024;
const GIB: u64 = 1024 * MIB;

fn matched() -> PrivilegeContext {
    PrivilegeContext::new(UserMatch::Match, true)
}

fn mismatched() -> PrivilegeContext {
    PrivilegeContext::new(UserMatch::Mismatch(MismatchReason::DifferentUser), true)
}

fn undetermined() -> PrivilegeContext {
    PrivilegeContext::new(UserMatch::Mismatch(MismatchReason::Undetermined), true)
}

fn machine() -> FakeFileSystem {
    FakeFileSystem::new()
        .canonical_as_is()
        .with_dir(r"C:\")
        .with_dir(r"C:\Windows")
        .with_file(r"C:\Windows\big.dll", 20 * MIB, Some(EpochMillis(9_000)))
        .with_dir(r"C:\Users")
        .with_dir(r"C:\Users\ana")
        .with_dir(r"C:\Users\ana\Downloads")
        .with_file(
            r"C:\Users\ana\Downloads\setup.exe",
            8 * MIB,
            Some(EpochMillis(5_000)),
        )
        .with_dir(r"C:\Users\bob")
        .with_file(r"C:\Users\bob\d.txt", 2 * MIB, None)
        .with_reparse_point(r"C:\Users\ana\Swapped")
}

struct Env {
    store: InMemoryStore,
    fs: FakeFileSystem,
    volume: FakeVolumeSpace,
    active: ActiveJob,
    slot: PickedRootSlot,
}

impl Env {
    fn new() -> Self {
        Self {
            store: InMemoryStore::new(),
            fs: machine(),
            volume: FakeVolumeSpace::with_values(100 * GIB, 60 * GIB),
            active: ActiveJob::default(),
            slot: PickedRootSlot::default(),
        }
    }

    fn ports(&self) -> FolderPorts<'_, InMemoryStore, FakeFileSystem, FakeVolumeSpace, FixedClock> {
        FolderPorts {
            store: &self.store,
            fs: &self.fs,
            volume: &self.volume,
            clock: &FixedClock(NOW),
            active: &self.active,
        }
    }

    fn scan_with(
        &self,
        privileges: &PrivilegeContext,
        root: &RootArg,
        emit: &mut dyn FnMut(FolderScanEventDto),
    ) -> Result<FolderScanSummaryDto, AppError> {
        run_folder_scan(
            &self.ports(),
            privileges,
            raw_c_ana(),
            root,
            &self.slot,
            emit,
        )
    }

    fn scan(&self) -> FolderScanSummaryDto {
        self.scan_with(&matched(), &system_drive(), &mut |_| {})
            .unwrap()
    }
}

fn system_drive() -> RootArg {
    RootArg {
        kind: "system_drive".into(),
        pick_id: None,
    }
}

fn picked_arg(id: i64) -> RootArg {
    RootArg {
        kind: "picked".into(),
        pick_id: Some(id),
    }
}

fn kind_of(error: &AppError) -> String {
    serde_json::to_value(error).unwrap()["kind"]
        .as_str()
        .unwrap()
        .to_string()
}

fn message_of(error: &AppError) -> String {
    serde_json::to_value(error).unwrap()["message"]
        .as_str()
        .unwrap()
        .to_string()
}

fn normalized(raw: &str) -> NormalizedPath {
    normalize_lexical(raw).expect("valid path")
}

// ---- errores del comando ----

#[test]
fn app_error_should_carry_the_domain_rejection_as_a_folder_root_rejected_code() {
    use crate::domain::protection::decision::ProtectedReason;
    let error = AppError::from(FolderScanError::RootRejected(ProtectedReason::ReparsePoint));
    assert_eq!(kind_of(&error), "folder_root_rejected");
    assert!(message_of(&error).ends_with("reparse_point"));
}

// ---- start_folder_scan ----

#[test]
fn start_should_scan_the_system_drive_and_answer_the_exact_summary() {
    let env = Env::new();
    let events = RefCell::new(Vec::new());
    let dto = env
        .scan_with(&matched(), &system_drive(), &mut |event| {
            events
                .borrow_mut()
                .push(serde_json::to_value(event).unwrap());
        })
        .unwrap();
    let used = 40 * GIB;
    assert_eq!(
        serde_json::to_value(&dto).unwrap(),
        json!({
            "scanId": 1,
            "status": "done",
            "startedAt": NOW.0,
            "finishedAt": NOW.0,
            "root": { "kind": "system_drive", "path": "C:\\", "folderId": 1 },
            "volume": { "totalBytes": 100 * GIB, "freeBytes": 60 * GIB },
            "unaccountedBytes": used - 30 * MIB,
            "overCounted": false,
            "totalBytes": 30 * MIB,
            "fileCount": 3,
            "dirCount": 5,
            "inaccessibleDirs": 0,
            "reparseSkipped": 1,
            "cloudFiles": 0,
            "opaqueDirs": 1,
            "foldersStored": 6,
            "foldersTruncated": false,
            "issues": [],
        })
    );
    assert_eq!(
        events.into_inner(),
        vec![
            json!({"type": "started", "scanId": 1, "rootKind": "system_drive"}),
            json!({"type": "walking", "filesSeen": 3, "dirsSeen": 6, "bytesSeen": 30 * MIB}),
            json!({"type": "saving"}),
        ]
    );
    assert_eq!(env.store.list_scans().unwrap().len(), 1);
}

#[test]
fn start_should_scan_a_picked_folder_by_its_pick_id() {
    let env = Env::new();
    let id = env.slot.put(normalized(r"C:\Users\ana\Downloads"));
    let dto = env
        .scan_with(&matched(), &picked_arg(id.get() as i64), &mut |_| {})
        .unwrap();
    let value = serde_json::to_value(&dto).unwrap();
    assert_eq!(value["root"]["kind"], "picked");
    assert_eq!(value["root"]["path"], r"C:\Users\ana\Downloads");
    assert_eq!(value["totalBytes"], 8 * MIB);
    assert_eq!(value["unaccountedBytes"], Value::Null);
    assert_eq!(value["fileCount"], 1);
}

#[test]
fn start_should_fail_with_pick_not_found_for_unknown_or_replaced_ids_without_creating_a_scan() {
    let env = Env::new();
    let first = env.slot.put(normalized(r"C:\Users\ana\Downloads"));
    let second = env.slot.put(normalized(r"C:\Users\ana"));
    for id in [first.get() as i64, 999, second.get() as i64 + 1] {
        let error = env
            .scan_with(&matched(), &picked_arg(id), &mut |_| {})
            .unwrap_err();
        assert_eq!(kind_of(&error), "folder_pick_not_found", "id {id}");
    }
    assert!(env.store.list_scans().unwrap().is_empty());
    // El id vigente si se puede usar.
    assert!(
        env.scan_with(&matched(), &picked_arg(second.get() as i64), &mut |_| {})
            .is_ok()
    );
}

#[test]
fn start_should_reject_a_root_that_stopped_being_valid_without_creating_a_scan() {
    let env = Env::new();
    // La carpeta elegida ya no es una carpeta: ahora es una junction.
    let id = env.slot.put(normalized(r"C:\Users\ana\Swapped"));
    let error = env
        .scan_with(&matched(), &picked_arg(id.get() as i64), &mut |_| {})
        .unwrap_err();
    assert_eq!(kind_of(&error), "folder_root_rejected");
    assert!(message_of(&error).ends_with("reparse_point"));
    assert!(env.store.list_scans().unwrap().is_empty());
}

#[test]
fn start_should_reject_invalid_root_arguments_before_creating_a_scan() {
    let env = Env::new();
    let bad = [
        RootArg {
            kind: "everything".into(),
            pick_id: None,
        },
        RootArg {
            kind: "picked".into(),
            pick_id: None,
        },
        RootArg {
            kind: "picked".into(),
            pick_id: Some(-1),
        },
        RootArg {
            kind: "system_drive".into(),
            pick_id: Some(1),
        },
    ];
    for root in bad {
        let error = env.scan_with(&matched(), &root, &mut |_| {}).unwrap_err();
        assert_eq!(kind_of(&error), "invalid_argument", "{root:?}");
    }
    assert!(env.store.list_scans().unwrap().is_empty());
}

#[test]
fn start_should_fail_with_the_error_of_the_active_job_without_creating_a_scan() {
    use crate::domain::cleanup::model::PreviewId;
    let env = Env::new();
    let cases: Vec<(JobGuard, &str)> = vec![(env.active.begin_scan().unwrap(), "scan_in_progress")];
    for (guard, expected) in cases {
        let error = env
            .scan_with(&matched(), &system_drive(), &mut |_| {})
            .unwrap_err();
        assert_eq!(kind_of(&error), expected);
        drop(guard);
    }
    let cleanup = env.active.begin_cleanup(PreviewId(7)).unwrap();
    let error = env
        .scan_with(&matched(), &system_drive(), &mut |_| {})
        .unwrap_err();
    assert_eq!(kind_of(&error), "cleanup_in_progress");
    drop(cleanup);
    let empty = env.active.begin_empty().unwrap();
    let error = env
        .scan_with(&matched(), &system_drive(), &mut |_| {})
        .unwrap_err();
    assert_eq!(kind_of(&error), "recycle_bin_empty_in_progress");
    drop(empty);
    assert!(env.store.list_scans().unwrap().is_empty());
    assert!(
        env.scan_with(&matched(), &system_drive(), &mut |_| {})
            .is_ok()
    );
}

#[test]
fn start_should_end_cancelled_without_folders_and_keep_the_previous_analysis() {
    let env = Env::new();
    let first = env.scan();
    assert_eq!(first.status, "done");
    let scan_id = RefCell::new(None);
    let dto = env
        .scan_with(&matched(), &system_drive(), &mut |event| match event {
            FolderScanEventDto::Started { scan_id: id, .. } => *scan_id.borrow_mut() = Some(id),
            FolderScanEventDto::Walking { .. } => {
                if let Some(id) = *scan_id.borrow() {
                    env.active.cancel_scan(ScanId(id)).unwrap();
                }
            }
            FolderScanEventDto::Saving => {}
        })
        .unwrap();
    assert_eq!(dto.status, "cancelled");
    let value = serde_json::to_value(&dto).unwrap();
    assert_eq!(value["root"]["folderId"], Value::Null);
    assert_eq!(value["foldersStored"], 0);
    assert_eq!(value["issues"], json!([]));
    let last = last_folder_scan(&env.store).unwrap().unwrap();
    assert_eq!(
        last.scan_id, first.scan_id,
        "el ultimo analisis sigue siendo el anterior"
    );
    assert_eq!(
        env.store
            .get_scan(ScanId(2))
            .unwrap()
            .unwrap()
            .status
            .as_str(),
        "cancelled"
    );
}

#[test]
fn start_should_treat_every_profile_as_opaque_in_over_the_shoulder() {
    let env = Env::new();
    let dto = env
        .scan_with(&mismatched(), &system_drive(), &mut |_| {})
        .unwrap();
    assert_eq!(dto.status, "done");
    let users = list_children(&env.store, 1, None, "size", 0, 10)
        .unwrap()
        .items
        .into_iter()
        .find(|folder| folder.name == "Users")
        .unwrap();
    let page = list_children(&env.store, 1, Some(users.id), "name", 0, 10).unwrap();
    let ana = page.items.iter().find(|f| f.name == "ana").unwrap();
    assert_eq!(ana.opaque, Some("other_user_profile"));
    assert!(!ana.has_children);
    assert!(page.items.iter().all(|f| f.opaque.is_some()));
}

// ---- consultas ----

#[test]
fn get_last_should_answer_none_then_the_summary() {
    let env = Env::new();
    assert_eq!(last_folder_scan(&env.store).unwrap(), None);
    let dto = env.scan();
    assert_eq!(last_folder_scan(&env.store).unwrap(), Some(dto));
}

#[test]
fn list_children_should_answer_the_exact_page_of_the_root() {
    let env = Env::new();
    env.scan();
    let page = list_children(&env.store, 1, None, "size", 0, 10).unwrap();
    let value = serde_json::to_value(&page).unwrap();
    assert_eq!(value["scanId"], 1);
    assert_eq!(value["total"], 2);
    assert_eq!(value["remainderBytes"], 0);
    assert_eq!(
        value["folder"],
        json!({
            "id": 1, "name": "C:\\", "path": "C:\\", "sizeBytes": 30 * MIB, "directBytes": 0,
            "fileCount": 3, "dirCount": 5, "access": "complete", "opaque": Value::Null,
            "system": false, "junkHint": false, "hasChildren": true,
        })
    );
    assert_eq!(value["breadcrumb"], json!([{"id": 1, "name": "C:\\"}]));
    let names: Vec<&str> = value["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["Windows", "Users"]);
    assert_eq!(value["items"][0]["system"], true);
    let by_name = list_children(&env.store, 1, None, "name", 0, 10).unwrap();
    assert_eq!(
        by_name
            .items
            .iter()
            .map(|f| f.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Users", "Windows"]
    );
    let second = list_children(&env.store, 1, None, "size", 1, 1).unwrap();
    assert_eq!((second.total, second.items.len()), (2, 1));
    assert_eq!(second.items[0].name, "Users");
}

#[test]
fn list_children_should_answer_an_opaque_folder_without_children() {
    let env = Env::new();
    env.scan();
    let users = list_children(&env.store, 1, None, "size", 0, 10)
        .unwrap()
        .items
        .into_iter()
        .find(|f| f.name == "Users")
        .unwrap();
    let users_page = list_children(&env.store, 1, Some(users.id), "size", 0, 10).unwrap();
    let bob = users_page.items.iter().find(|f| f.name == "bob").unwrap();
    assert_eq!(bob.opaque, Some("other_user_profile"));
    assert!(!bob.has_children);
    let page = list_children(&env.store, 1, Some(bob.id), "size", 0, 10).unwrap();
    assert_eq!((page.total, page.items.len()), (0, 0));
    assert_eq!(
        page.breadcrumb
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        vec!["C:\\", "Users", "bob"]
    );
}

#[test]
fn list_children_should_validate_arguments_and_the_scan_and_folder_ids() {
    let env = Env::new();
    env.scan();
    let kind = |result: Result<FolderPageDto, AppError>| kind_of(&result.unwrap_err());
    assert_eq!(
        kind(list_children(&env.store, 1, None, "size", 0, 0)),
        "invalid_argument"
    );
    assert_eq!(
        kind(list_children(&env.store, 1, None, "size", 0, 201)),
        "invalid_argument"
    );
    assert_eq!(
        kind(list_children(&env.store, 1, None, "size", -1, 10)),
        "invalid_argument"
    );
    assert_eq!(
        kind(list_children(&env.store, 1, None, "date", 0, 10)),
        "invalid_argument"
    );
    assert_eq!(
        kind(list_children(&env.store, -1, None, "size", 0, 10)),
        "invalid_argument"
    );
    assert_eq!(
        kind(list_children(&env.store, 1, Some(-3), "size", 0, 10)),
        "invalid_argument"
    );
    assert_eq!(
        kind(list_children(&env.store, 1, Some(1 << 53), "size", 0, 10)),
        "invalid_argument"
    );
    assert_eq!(
        kind(list_children(&env.store, 7, None, "size", 0, 10)),
        "scan_not_found"
    );
    assert_eq!(
        kind(list_children(&env.store, 1, Some(999_999), "size", 0, 10)),
        "folder_not_found"
    );
    assert!(list_children(&env.store, 1, None, "size", 0, 200).is_ok());
}

#[test]
fn list_children_should_not_answer_an_older_analysis() {
    let env = Env::new();
    env.scan();
    env.scan();
    let error = list_children(&env.store, 1, None, "size", 0, 10).unwrap_err();
    assert_eq!(kind_of(&error), "scan_not_found");
    assert!(list_children(&env.store, 2, None, "size", 0, 10).is_ok());
}

#[test]
fn list_files_should_answer_the_exact_pages_in_each_order() {
    let env = Env::new();
    env.scan();
    let page = list_files(&env.store, 1, "size", 0, 10).unwrap();
    let value = serde_json::to_value(&page).unwrap();
    assert_eq!(value["scanId"], 1);
    assert_eq!(value["total"], 2);
    assert_eq!(
        value["items"][0],
        json!({
            "id": 1, "name": "big.dll", "path": "C:\\Windows\\big.dll", "folderId": 6,
            "sizeBytes": 20 * MIB, "modifiedAt": 9_000,
        })
    );
    let names = |sort: &str| -> Vec<String> {
        list_files(&env.store, 1, sort, 0, 10)
            .unwrap()
            .items
            .into_iter()
            .map(|f| f.name)
            .collect()
    };
    assert_eq!(names("size"), vec!["big.dll", "setup.exe"]);
    assert_eq!(names("newest"), vec!["big.dll", "setup.exe"]);
    assert_eq!(names("oldest"), vec!["setup.exe", "big.dll"]);
    assert!(
        list_files(&env.store, 1, "size", 0, 10)
            .unwrap()
            .items
            .iter()
            .all(|f| !f.path.contains("bob"))
    );
}

#[test]
fn list_files_should_validate_arguments_and_the_scan() {
    let env = Env::new();
    env.scan();
    let kind = |result: Result<LargeFilePageDto, AppError>| kind_of(&result.unwrap_err());
    assert_eq!(
        kind(list_files(&env.store, 1, "size", 0, 0)),
        "invalid_argument"
    );
    assert_eq!(
        kind(list_files(&env.store, 1, "size", 0, 500)),
        "invalid_argument"
    );
    assert_eq!(
        kind(list_files(&env.store, 1, "size", -5, 10)),
        "invalid_argument"
    );
    assert_eq!(
        kind(list_files(&env.store, 1, "name", 0, 10)),
        "invalid_argument"
    );
    assert_eq!(
        kind(list_files(&env.store, 3, "size", 0, 10)),
        "scan_not_found"
    );
}

// ---- selector de carpeta ----

fn pick(
    env: &Env,
    picker: &FakeFolderPicker,
    gate: &PickerGate,
    privileges: &PrivilegeContext,
) -> Result<PickResultDto, AppError> {
    pick_folder_run(picker, gate, &env.slot, &env.fs, privileges, raw_c_ana())
}

#[test]
fn pick_should_answer_a_valid_folder_with_a_new_id_and_replace_the_previous_one() {
    let env = Env::new();
    let gate = PickerGate::default();
    let picker = FakeFolderPicker::scripted(vec![
        Ok(Some(PathBuf::from(r"C:\Users\ana\Downloads"))),
        Ok(Some(PathBuf::from(r"C:\Windows"))),
    ]);
    let first = pick(&env, &picker, &gate, &matched()).unwrap();
    let PickResultDto::Picked { pick_id, path } = first else {
        panic!("se esperaba picked");
    };
    assert_eq!(path, r"C:\Users\ana\Downloads");
    assert!(env.slot.get(PickId::new(pick_id).unwrap()).is_some());
    let second = pick(&env, &picker, &gate, &matched()).unwrap();
    let PickResultDto::Picked {
        pick_id: next,
        path,
    } = second
    else {
        panic!("se esperaba picked");
    };
    assert_eq!(path, r"C:\Windows");
    assert_ne!(pick_id, next);
    assert!(
        env.slot.get(PickId::new(pick_id).unwrap()).is_none(),
        "el id viejo ya no sirve"
    );
    assert!(env.slot.get(PickId::new(next).unwrap()).is_some());
}

#[test]
fn pick_should_serialize_with_a_status_tag() {
    let picked = PickResultDto::Picked {
        pick_id: 4,
        path: r"C:\x".into(),
    };
    assert_eq!(
        serde_json::to_value(&picked).unwrap(),
        json!({"status": "picked", "pickId": 4, "path": "C:\\x"})
    );
    assert_eq!(
        serde_json::to_value(&PickResultDto::Cancelled).unwrap(),
        json!({"status": "cancelled"})
    );
}

#[test]
fn pick_should_keep_the_previous_selection_when_the_dialog_is_cancelled() {
    let env = Env::new();
    let gate = PickerGate::default();
    let picker = FakeFolderPicker::scripted(vec![
        Ok(Some(PathBuf::from(r"C:\Users\ana\Downloads"))),
        Ok(None),
    ]);
    let PickResultDto::Picked { pick_id, .. } = pick(&env, &picker, &gate, &matched()).unwrap()
    else {
        panic!("se esperaba picked");
    };
    let cancelled = pick(&env, &picker, &gate, &matched()).unwrap();
    assert_eq!(cancelled, PickResultDto::Cancelled);
    assert!(env.slot.get(PickId::new(pick_id).unwrap()).is_some());
}

#[test]
fn pick_should_reject_other_volumes_other_profiles_and_links_keeping_the_selection() {
    let env = Env::new();
    let gate = PickerGate::default();
    let cases = [
        (r"D:\Datos", "unsupported_volume"),
        (r"C:\Users\bob", "other_user_profile"),
        (r"C:\Users\ana\Swapped", "reparse_point"),
        (r"C:\Users\ana\Downloads\setup.exe", "not_a_directory"),
        (r"C:\Users\ana\nope", "not_found"),
    ];
    for (raw, code) in cases {
        let picker = FakeFolderPicker::scripted(vec![Ok(Some(PathBuf::from(raw)))]);
        let error = pick(&env, &picker, &gate, &matched()).unwrap_err();
        assert_eq!(kind_of(&error), "folder_root_rejected", "{raw}");
        assert!(
            message_of(&error).ends_with(code),
            "{raw}: {}",
            message_of(&error)
        );
    }
    assert!(
        env.slot.get(PickId::new(1).unwrap()).is_none(),
        "nada se guardo"
    );
}

#[test]
fn pick_should_reject_the_own_profile_in_over_the_shoulder() {
    let env = Env::new();
    let gate = PickerGate::default();
    for privileges in [mismatched(), undetermined()] {
        let picker =
            FakeFolderPicker::scripted(vec![Ok(Some(PathBuf::from(r"C:\Users\ana\Downloads")))]);
        let error = pick(&env, &picker, &gate, &privileges).unwrap_err();
        assert_eq!(kind_of(&error), "folder_root_rejected");
        assert!(message_of(&error).ends_with("other_user_profile"));
    }
}

#[test]
fn pick_should_allow_a_single_dialog_at_a_time() {
    let env = Env::new();
    let gate = PickerGate::default();
    let picker = FakeFolderPicker::scripted(vec![Ok(None)]);
    let lease = gate.try_acquire().expect("free gate");
    let error = pick(&env, &picker, &gate, &matched()).unwrap_err();
    assert_eq!(kind_of(&error), "folder_picker_busy");
    assert_eq!(picker.openings(), 0, "el segundo dialogo ni se abre");
    drop(lease);
    assert!(matches!(
        pick(&env, &picker, &gate, &matched()),
        Ok(PickResultDto::Cancelled)
    ));
    assert_eq!(picker.openings(), 1);
}

#[test]
fn pick_should_report_a_dialog_failure_and_release_the_gate() {
    let env = Env::new();
    let gate = PickerGate::default();
    let picker = FakeFolderPicker::scripted(vec![
        Err(PortError::Unsupported("com_init".into())),
        Ok(None),
    ]);
    let error = pick(&env, &picker, &gate, &matched()).unwrap_err();
    assert_eq!(kind_of(&error), "folder_picker_unavailable");
    assert!(matches!(
        pick(&env, &picker, &gate, &matched()),
        Ok(PickResultDto::Cancelled)
    ));
}

#[test]
fn picked_slot_should_issue_increasing_ids_and_forget_the_replaced_one() {
    let slot = PickedRootSlot::default();
    let a = slot.put(normalized(r"C:\a"));
    let b = slot.put(normalized(r"C:\b"));
    assert!(b.get() > a.get());
    assert_eq!(slot.get(a), None);
    assert_eq!(slot.get(b).unwrap().to_string(), r"C:\b");
    assert_eq!(slot.get(PickId::new(0).unwrap()), None);
}

// ---- mostrar en el Explorador ----

fn reveal(
    env: &Env,
    explorer: &FakeExplorer,
    privileges: &PrivilegeContext,
    scan: i64,
    kind: &str,
    id: i64,
) -> Result<(), AppError> {
    reveal_run(
        &env.store,
        &env.fs,
        explorer,
        privileges,
        scan,
        &RevealTargetArg {
            kind: kind.into(),
            id,
        },
    )
}

fn folder_id(env: &Env, name: &str) -> i64 {
    fn find(env: &Env, folder: Option<i64>, name: &str) -> Option<i64> {
        let page = list_children(&env.store, 1, folder, "size", 0, 50).ok()?;
        for item in page.items {
            if item.name == name {
                return Some(item.id);
            }
            if item.opaque.is_none()
                && let Some(found) = find(env, Some(item.id), name)
            {
                return Some(found);
            }
        }
        None
    }
    find(env, None, name).expect("folder in the fixture")
}

#[test]
fn reveal_should_show_a_large_file_by_its_id_and_never_open_it() {
    let env = Env::new();
    env.scan();
    let explorer = FakeExplorer::new();
    reveal(&env, &explorer, &matched(), 1, "file", 1).unwrap();
    assert_eq!(
        explorer.revealed(),
        vec![PathBuf::from(r"C:\Windows\big.dll")]
    );
    let downloads = folder_id(&env, "Downloads");
    reveal(&env, &explorer, &matched(), 1, "folder", downloads).unwrap();
    assert_eq!(
        explorer.revealed().last(),
        Some(&PathBuf::from(r"C:\Users\ana\Downloads"))
    );
    reveal(&env, &explorer, &matched(), 1, "folder", 1).unwrap();
    assert_eq!(explorer.revealed().last(), Some(&PathBuf::from(r"C:\")));
}

#[test]
fn reveal_should_refuse_opaque_folders() {
    let env = Env::new();
    env.scan();
    let explorer = FakeExplorer::new();
    let bob = folder_id(&env, "bob");
    let error = reveal(&env, &explorer, &matched(), 1, "folder", bob).unwrap_err();
    assert_eq!(kind_of(&error), "reveal_unavailable");
    assert!(message_of(&error).ends_with("opaque"));
    assert!(explorer.revealed().is_empty());
}

#[test]
fn reveal_should_be_blocked_in_over_the_shoulder_with_the_mismatch_reason() {
    let env = Env::new();
    env.scan();
    for (privileges, code) in [
        (mismatched(), "different_user"),
        (undetermined(), "undetermined"),
    ] {
        let explorer = FakeExplorer::new();
        let error = reveal(&env, &explorer, &privileges, 1, "file", 1).unwrap_err();
        assert_eq!(kind_of(&error), "reveal_unavailable");
        assert!(message_of(&error).ends_with(code));
        assert!(explorer.revealed().is_empty());
    }
}

#[test]
fn reveal_should_report_an_item_deleted_after_the_analysis() {
    let env = Env::new();
    env.scan();
    env.fs.remove(std::path::Path::new(r"C:\Windows\big.dll"));
    let explorer = FakeExplorer::new();
    let error = reveal(&env, &explorer, &matched(), 1, "file", 1).unwrap_err();
    assert_eq!(kind_of(&error), "reveal_unavailable");
    assert!(message_of(&error).ends_with("target_missing"));
    assert!(explorer.revealed().is_empty());
}

#[test]
fn reveal_should_report_a_shell_error() {
    let env = Env::new();
    env.scan();
    let explorer = FakeExplorer::failing(PortError::Unsupported("io_0x80004005".into()));
    let error = reveal(&env, &explorer, &matched(), 1, "file", 1).unwrap_err();
    assert_eq!(kind_of(&error), "reveal_unavailable");
    assert!(message_of(&error).ends_with("shell_error"));
    assert_eq!(RevealReason::ShellError.as_str(), "shell_error");
}

#[test]
fn reveal_should_validate_arguments_the_scan_and_the_ids() {
    let env = Env::new();
    env.scan();
    let explorer = FakeExplorer::new();
    let kind = |scan: i64, target: &str, id: i64| {
        kind_of(&reveal(&env, &explorer, &matched(), scan, target, id).unwrap_err())
    };
    assert_eq!(kind(1, "executable", 1), "invalid_argument");
    assert_eq!(kind(1, "file", -1), "invalid_argument");
    assert_eq!(kind(-1, "file", 1), "invalid_argument");
    assert_eq!(kind(9, "file", 1), "scan_not_found");
    assert_eq!(kind(1, "file", 12_345), "folder_not_found");
    assert_eq!(kind(1, "folder", 12_345), "folder_not_found");
    assert!(explorer.revealed().is_empty());
}

use super::*;
use crate::domain::duplicates::model::RootStatus::{Done, Nested, NotSelected, Unavailable};
use crate::domain::ports::PortError;
use crate::domain::ports::fakes::FakeFileSystem;
use crate::domain::protection::path::normalize_lexical;
use crate::domain::protection::test_support::{ctx_c_ana, raw_c_ana};

const HOME: &str = r"C:\Users\ana";

fn standard_fs() -> FakeFileSystem {
    [
        "Desktop",
        "Documents",
        "Downloads",
        "Music",
        "Pictures",
        "Videos",
    ]
    .iter()
    .fold(FakeFileSystem::new(), |fs, name| {
        fs.with_dir(&format!(r"{HOME}\{name}"))
    })
}

fn status_of(plans: &[RootPlan], folder: ContentFolder) -> RootStatus {
    plans
        .iter()
        .find(|plan| plan.folder == folder)
        .map(|plan| plan.status)
        .unwrap_or(NotSelected)
}

fn with_user_folder(folder: ContentFolder, raw_path: &str) -> ProtectionContext {
    let mut raw = raw_c_ana();
    if let Some(user) = raw.user.as_mut() {
        for entry in &mut user.content_folders {
            if entry.0 == folder {
                entry.1 = raw_path.to_string();
            }
        }
    }
    ProtectionContext::from_raw(raw).unwrap()
}

fn all() -> Vec<ContentFolder> {
    ContentFolder::ALL.to_vec()
}

#[test]
fn resolve_should_mark_the_six_folders_done_by_default() {
    let plans = resolve_roots(&standard_fs(), &Ok(ctx_c_ana()), &all());
    assert_eq!(plans.len(), 6);
    assert!(plans.iter().all(|plan| plan.status == Done));
    assert_eq!(
        plans[2].path.as_ref().map(ToString::to_string),
        Some(r"C:\Users\ana\Downloads".to_string())
    );
}

#[test]
fn resolve_should_mark_the_unselected_ones_without_touching_the_disk() {
    let fs = standard_fs();
    let before = fs.total_calls();
    let plans = resolve_roots(&fs, &Ok(ctx_c_ana()), &[ContentFolder::Downloads]);
    assert_eq!(status_of(&plans, ContentFolder::Downloads), Done);
    for folder in [
        ContentFolder::Desktop,
        ContentFolder::Documents,
        ContentFolder::Music,
        ContentFolder::Pictures,
        ContentFolder::Videos,
    ] {
        assert_eq!(status_of(&plans, folder), NotSelected, "{folder:?}");
    }
    // Solo se consulto la carpeta seleccionada.
    assert_eq!(fs.total_calls() - before, 1);
}

#[test]
fn resolve_should_report_each_unavailable_reason() {
    let missing = FakeFileSystem::new();
    let plans = resolve_roots(&missing, &Ok(ctx_c_ana()), &[ContentFolder::Music]);
    assert_eq!(
        status_of(&plans, ContentFolder::Music),
        Unavailable(RootReason::NotFound)
    );

    let file = FakeFileSystem::new().with_file(r"C:\Users\ana\Music", 5, None);
    let plans = resolve_roots(&file, &Ok(ctx_c_ana()), &[ContentFolder::Music]);
    assert_eq!(
        status_of(&plans, ContentFolder::Music),
        Unavailable(RootReason::NotADirectory)
    );

    let junction = FakeFileSystem::new().with_reparse_point(r"C:\Users\ana\Music");
    let plans = resolve_roots(&junction, &Ok(ctx_c_ana()), &[ContentFolder::Music]);
    assert_eq!(
        status_of(&plans, ContentFolder::Music),
        Unavailable(RootReason::ReparsePoint)
    );

    let cloud = FakeFileSystem::new().with_cloud_reparse_point(r"C:\Users\ana\Music");
    let plans = resolve_roots(&cloud, &Ok(ctx_c_ana()), &[ContentFolder::Music]);
    assert_eq!(
        status_of(&plans, ContentFolder::Music),
        Unavailable(RootReason::Cloud)
    );
}

#[test]
fn resolve_should_report_inaccessible_when_the_metadata_fails_for_another_reason() {
    let fs = FakeFileSystem::new().fail_on(
        r"C:\Users\ana\Music",
        PortError::PermissionDenied("x".into()),
    );
    let plans = resolve_roots(&fs, &Ok(ctx_c_ana()), &[ContentFolder::Music]);
    assert_eq!(
        status_of(&plans, ContentFolder::Music),
        Unavailable(RootReason::Inaccessible)
    );
}

#[test]
fn resolve_should_report_another_volume_without_querying_it() {
    let ctx = with_user_folder(ContentFolder::Videos, r"D:\Videos");
    let fs = standard_fs();
    let before = fs.total_calls();
    let plans = resolve_roots(
        &fs,
        &Ok(ctx),
        &[ContentFolder::Videos, ContentFolder::Music],
    );
    assert_eq!(
        status_of(&plans, ContentFolder::Videos),
        Unavailable(RootReason::UnsupportedVolume)
    );
    // Las demas siguen: Music se resolvio normalmente.
    assert_eq!(status_of(&plans, ContentFolder::Music), Done);
    assert_eq!(fs.total_calls() - before, 1);
}

#[test]
fn resolve_should_mark_every_selected_folder_when_the_context_is_invalid() {
    let broken: Result<ProtectionContext, ContextError> =
        Err(ContextError::MissingRequired("windows_dir"));
    let plans = resolve_roots(&standard_fs(), &broken, &[ContentFolder::Downloads]);
    assert_eq!(
        status_of(&plans, ContentFolder::Downloads),
        Unavailable(RootReason::ContextUnavailable)
    );
    assert_eq!(status_of(&plans, ContentFolder::Music), NotSelected);
    assert!(plans.iter().all(|plan| plan.path.is_none()));
}

#[test]
fn resolve_should_report_not_found_when_the_context_has_no_such_folder() {
    let mut raw = raw_c_ana();
    if let Some(user) = raw.user.as_mut() {
        user.content_folders
            .retain(|(key, _)| *key != ContentFolder::Downloads);
    }
    let ctx = ProtectionContext::from_raw(raw).unwrap();
    let plans = resolve_roots(&standard_fs(), &Ok(ctx), &[ContentFolder::Downloads]);
    assert_eq!(
        status_of(&plans, ContentFolder::Downloads),
        Unavailable(RootReason::NotFound)
    );
}

#[test]
fn resolve_should_mark_downloads_nested_inside_documents() {
    let ctx = with_user_folder(
        ContentFolder::Downloads,
        r"C:\Users\ana\Documents\Descargas",
    );
    let fs = standard_fs().with_dir(r"C:\Users\ana\Documents\Descargas");
    let plans = resolve_roots(&fs, &Ok(ctx), &all());
    assert_eq!(status_of(&plans, ContentFolder::Downloads), Nested);
    assert_eq!(status_of(&plans, ContentFolder::Documents), Done);
    assert_eq!(status_of(&plans, ContentFolder::Desktop), Done);
}

#[test]
fn resolve_should_not_nest_inside_a_folder_that_is_not_selected_or_not_available() {
    let ctx = with_user_folder(
        ContentFolder::Downloads,
        r"C:\Users\ana\Documents\Descargas",
    );
    let fs = standard_fs().with_dir(r"C:\Users\ana\Documents\Descargas");
    let only_downloads = resolve_roots(&fs, &Ok(ctx), &[ContentFolder::Downloads]);
    assert_eq!(status_of(&only_downloads, ContentFolder::Downloads), Done);

    let ctx = with_user_folder(
        ContentFolder::Downloads,
        r"C:\Users\ana\Documents\Descargas",
    );
    let cloud_documents = FakeFileSystem::new()
        .with_cloud_reparse_point(r"C:\Users\ana\Documents")
        .with_dir(r"C:\Users\ana\Documents\Descargas");
    let plans = resolve_roots(
        &cloud_documents,
        &Ok(ctx),
        &[ContentFolder::Documents, ContentFolder::Downloads],
    );
    assert_eq!(
        status_of(&plans, ContentFolder::Documents),
        Unavailable(RootReason::Cloud)
    );
    assert_eq!(status_of(&plans, ContentFolder::Downloads), Done);
}

#[test]
fn resolve_should_nest_the_later_folder_when_two_keys_share_a_path() {
    let ctx = with_user_folder(ContentFolder::Pictures, r"C:\Users\ana\Music");
    let plans = resolve_roots(&standard_fs(), &Ok(ctx), &all());
    assert_eq!(status_of(&plans, ContentFolder::Music), Done);
    assert_eq!(status_of(&plans, ContentFolder::Pictures), Nested);
}

#[test]
fn owning_root_should_attribute_a_file_to_the_deepest_containing_root() {
    let ctx = with_user_folder(
        ContentFolder::Downloads,
        r"C:\Users\ana\Documents\Descargas",
    );
    let fs = standard_fs().with_dir(r"C:\Users\ana\Documents\Descargas");
    let plans = resolve_roots(&fs, &Ok(ctx), &all());
    let owner = |raw: &str| owning_root(&plans, &normalize_lexical(raw).unwrap());
    assert_eq!(
        owner(r"C:\Users\ana\Documents\Descargas\a.iso"),
        Some(ContentFolder::Downloads)
    );
    assert_eq!(
        owner(r"C:\Users\ana\Documents\otro\a.iso"),
        Some(ContentFolder::Documents)
    );
    assert_eq!(owner(r"C:\Users\ana\AppData\a.iso"), None);
}

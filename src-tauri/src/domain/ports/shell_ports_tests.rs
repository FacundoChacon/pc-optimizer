//! Contrato de los dobles de los puertos `VolumeSpace`, `FolderPicker` y `Explorer` (C-07).

use std::path::{Path, PathBuf};

use crate::domain::ports::fakes_shell::{FakeExplorer, FakeFolderPicker, FakeVolumeSpace};
use crate::domain::ports::{Explorer, FolderPicker, PortError, VolumeSpace, VolumeSpaceInfo};

#[test]
fn fake_volume_space_should_report_the_configured_values_for_any_root() {
    let volume = FakeVolumeSpace::with_values(500, 120);
    assert_eq!(
        volume.space(Path::new(r"C:\")),
        Ok(VolumeSpaceInfo {
            total_bytes: 500,
            free_bytes: 120
        })
    );
    let other = FakeVolumeSpace::with_values(1, 1);
    assert_eq!(
        other.space(Path::new(r"C:\x")).map(|info| info.total_bytes),
        Ok(1)
    );
}

#[test]
fn fake_volume_space_should_fail_when_configured_to() {
    let volume = FakeVolumeSpace::failing(PortError::Io("boom".into()));
    assert_eq!(
        volume.space(Path::new(r"C:\")),
        Err(PortError::Io("boom".into()))
    );
}

#[test]
fn fake_folder_picker_should_play_its_script_and_count_openings() {
    let picker = FakeFolderPicker::scripted(vec![
        Ok(Some(PathBuf::from(r"C:\Games"))),
        Ok(None),
        Err(PortError::Unsupported("com".into())),
    ]);
    assert_eq!(picker.pick(), Ok(Some(PathBuf::from(r"C:\Games"))));
    assert_eq!(picker.pick(), Ok(None));
    assert_eq!(picker.pick(), Err(PortError::Unsupported("com".into())));
    assert_eq!(picker.openings(), 3);
}

#[test]
fn fake_folder_picker_should_report_cancelled_when_the_script_runs_out() {
    let picker = FakeFolderPicker::scripted(vec![]);
    assert_eq!(picker.pick(), Ok(None));
    assert_eq!(picker.openings(), 1);
}

#[test]
fn fake_explorer_should_record_revealed_paths_in_order() {
    let explorer = FakeExplorer::new();
    assert_eq!(explorer.reveal(Path::new(r"C:\a\x.bin")), Ok(()));
    assert_eq!(explorer.reveal(Path::new(r"C:\b")), Ok(()));
    assert_eq!(
        explorer.revealed(),
        vec![PathBuf::from(r"C:\a\x.bin"), PathBuf::from(r"C:\b")]
    );
}

#[test]
fn fake_explorer_should_fail_without_recording_when_configured_to() {
    let explorer = FakeExplorer::failing(PortError::Io("shell".into()));
    assert_eq!(
        explorer.reveal(Path::new(r"C:\a")),
        Err(PortError::Io("shell".into()))
    );
    assert!(explorer.revealed().is_empty());
}

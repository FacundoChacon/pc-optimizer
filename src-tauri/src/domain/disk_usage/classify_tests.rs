use super::*;
use crate::domain::protection::path::{NormalizedPath, normalize_lexical};
use crate::domain::protection::test_support::{ctx_c_ana, ctx_c_no_user};

fn np(raw: &str) -> NormalizedPath {
    match normalize_lexical(raw) {
        Ok(path) => path,
        Err(rejection) => panic!("{raw:?}: {rejection:?}"),
    }
}

#[test]
fn opaque_should_flag_other_profiles_the_recycle_bin_and_system_volume_information() {
    let ctx = ctx_c_ana();
    let classifier = Classifier::new(&ctx);
    let cases = [
        (r"C:\Users\bob", Some(OpaqueReason::OtherUserProfile)),
        (r"C:\Users\Public", Some(OpaqueReason::OtherUserProfile)),
        (r"C:\$Recycle.Bin", Some(OpaqueReason::RecycleBin)),
        (r"C:\$RECYCLE.BIN", Some(OpaqueReason::RecycleBin)),
        (
            r"C:\System Volume Information",
            Some(OpaqueReason::SystemVolumeInformation),
        ),
    ];
    for (raw, expected) in cases {
        assert_eq!(classifier.opaque(&np(raw)), expected, "caso {raw:?}");
    }
}

#[test]
fn opaque_should_not_flag_own_profile_ordinary_folders_or_nested_look_alikes() {
    let ctx = ctx_c_ana();
    let classifier = Classifier::new(&ctx);
    for raw in [
        r"C:\Users",
        r"C:\Users\ana",
        r"C:\Users\ana\Downloads",
        r"C:\Windows",
        r"C:\x\$Recycle.Bin",
        r"C:\x\System Volume Information",
        r"C:\Recovery",
    ] {
        assert_eq!(classifier.opaque(&np(raw)), None, "caso {raw:?}");
    }
}

#[test]
fn opaque_should_flag_every_profile_when_the_context_has_no_user() {
    let ctx = ctx_c_no_user();
    let classifier = Classifier::new(&ctx);
    assert_eq!(
        classifier.opaque(&np(r"C:\Users\ana")),
        Some(OpaqueReason::OtherUserProfile)
    );
    assert_eq!(
        classifier.opaque(&np(r"C:\Users\bob")),
        Some(OpaqueReason::OtherUserProfile)
    );
    assert_eq!(classifier.opaque(&np(r"C:\Users")), None);
}

#[test]
fn system_should_flag_windows_programs_and_volume_system_entries_only() {
    let ctx = ctx_c_ana();
    let classifier = Classifier::new(&ctx);
    for raw in [
        r"C:\Windows",
        r"C:\Windows\System32",
        r"C:\Windows\Temp",
        r"C:\Program Files",
        r"C:\Program Files (x86)\X",
        r"C:\ProgramData",
        r"C:\Recovery",
    ] {
        assert!(classifier.is_system(&np(raw)), "caso {raw:?}");
    }
    for raw in [
        r"C:\Users\ana\Downloads",
        r"C:\Games",
        r"C:\Users",
        r"C:\Windows.old",
    ] {
        assert!(!classifier.is_system(&np(raw)), "caso {raw:?}");
    }
}

#[test]
fn junk_hint_should_flag_whitelist_locations_and_what_is_inside() {
    let ctx = ctx_c_ana();
    let classifier = Classifier::new(&ctx);
    for raw in [
        r"C:\Users\ana\AppData\Local\Microsoft\Edge\User Data\Default\Cache",
        r"C:\Users\ana\AppData\Local\Microsoft\Edge\User Data\Default\Cache\Cache_Data",
        r"C:\Users\ana\AppData\Local\Temp",
        r"C:\Users\ana\AppData\Local\Temp\sub\deep",
        r"C:\Windows\Temp",
    ] {
        assert!(classifier.junk_hint(&np(raw)), "caso {raw:?}");
    }
}

#[test]
fn junk_hint_should_not_flag_ordinary_folders_or_the_browser_profile_itself() {
    let ctx = ctx_c_ana();
    let classifier = Classifier::new(&ctx);
    for raw in [
        r"C:\Users\ana\Downloads",
        r"C:\Users\ana\AppData\Local",
        r"C:\Users\ana\AppData\Local\Microsoft\Edge\User Data",
        r"C:\Users\ana\AppData\Local\Microsoft\Edge\User Data\Default",
        r"C:\Users\ana\AppData\Local\Microsoft\Edge\User Data\Default\Local Storage",
        r"C:\Games",
    ] {
        assert!(!classifier.junk_hint(&np(raw)), "caso {raw:?}");
    }
}

#[test]
fn junk_hint_should_only_flag_machine_locations_without_a_user() {
    let ctx = ctx_c_no_user();
    let classifier = Classifier::new(&ctx);
    assert!(classifier.junk_hint(&np(r"C:\Windows\Temp")));
    assert!(!classifier.junk_hint(&np(r"C:\Users\ana\AppData\Local\Temp")));
    assert!(!classifier.junk_hint(&np(
        r"C:\Users\ana\AppData\Local\Microsoft\Edge\User Data\Default\Cache"
    )));
}

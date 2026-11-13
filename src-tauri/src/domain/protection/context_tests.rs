use super::*;
use crate::domain::protection::path::normalize_lexical;
use crate::domain::protection::test_support::{raw_c_ana, raw_for};

/// Cambio que una tabla aplica a un contexto crudo.
type Mutation<T> = Box<dyn Fn(&mut T)>;

fn np(raw: &str) -> crate::domain::protection::path::NormalizedPath {
    match normalize_lexical(raw) {
        Ok(path) => path,
        Err(rejection) => panic!("{raw:?}: {rejection:?}"),
    }
}

#[test]
fn from_raw_should_accept_a_complete_system_drive_c_context() {
    let ctx = ProtectionContext::from_raw(raw_c_ana()).unwrap();
    assert_eq!(ctx.system_drive(), 'C');
    assert_eq!(ctx.supported_volumes(), &['C']);
    assert_eq!(ctx.windows_dir(), &np(r"C:\Windows"));
    assert_eq!(ctx.program_files().len(), 2);
    assert_eq!(ctx.program_data(), &np(r"C:\ProgramData"));
    assert_eq!(ctx.users_root(), &np(r"C:\Users"));
    assert_eq!(
        ctx.app_dirs(),
        &[np(r"C:\Users\ana\AppData\Local\pc-optimizer")]
    );
    let user = ctx.user().unwrap();
    assert_eq!(user.profile(), &np(r"C:\Users\ana"));
    assert_eq!(user.local_app_data(), &np(r"C:\Users\ana\AppData\Local"));
    assert_eq!(
        user.roaming_app_data(),
        &np(r"C:\Users\ana\AppData\Roaming")
    );
    assert_eq!(user.temp(), Some(&np(r"C:\Users\ana\AppData\Local\Temp")));
    assert_eq!(user.content_folders().len(), 6);
    assert_eq!(user.source(), UserSource::ProcessToken);
}

#[test]
fn from_raw_should_follow_the_system_drive_of_windows_dir() {
    let ctx = ProtectionContext::from_raw(raw_for('D', "ana")).unwrap();
    assert_eq!(ctx.system_drive(), 'D');
    assert_eq!(ctx.supported_volumes(), &['D']);
    assert_eq!(ctx.windows_dir(), &np(r"D:\Windows"));
    assert_eq!(
        ctx.user().map(|u| u.profile().clone()),
        Some(np(r"D:\Users\ana"))
    );
}

#[test]
fn from_raw_should_fail_when_any_mandatory_location_is_missing() {
    let cases: Vec<(&str, Mutation<RawProtectionContext>)> = vec![
        ("windows_dir", Box::new(|r| r.windows_dir = None)),
        ("program_files", Box::new(|r| r.program_files.clear())),
        ("program_data", Box::new(|r| r.program_data = None)),
        ("users_root", Box::new(|r| r.users_root = None)),
    ];
    for (name, mutate) in cases {
        let mut raw = raw_c_ana();
        mutate(&mut raw);
        assert_eq!(
            ProtectionContext::from_raw(raw).err(),
            Some(ContextError::MissingRequired(name)),
            "caso {name}"
        );
    }
}

#[test]
fn from_raw_should_fail_when_a_mandatory_location_cannot_be_normalized() {
    let cases: Vec<(&str, Mutation<RawProtectionContext>)> = vec![
        (
            "windows_dir",
            Box::new(|r| r.windows_dir = Some("Windows".into())),
        ),
        (
            "program_files",
            Box::new(|r| r.program_files.push(r"\\srv\sh".into())),
        ),
        (
            "program_data",
            Box::new(|r| r.program_data = Some(r"C:\a\..\..".into())),
        ),
        (
            "users_root",
            Box::new(|r| r.users_root = Some(String::new())),
        ),
        ("app_dirs", Box::new(|r| r.app_dirs.push("rel".into()))),
    ];
    for (name, mutate) in cases {
        let mut raw = raw_c_ana();
        mutate(&mut raw);
        let error = ProtectionContext::from_raw(raw).err();
        assert!(
            matches!(error, Some(ContextError::Unnormalizable { location, .. }) if location == name),
            "caso {name}: {error:?}"
        );
    }
}

#[test]
fn from_raw_should_fail_when_an_anchor_is_a_volume_root() {
    let cases: Vec<(&str, Mutation<RawProtectionContext>)> = vec![
        (
            "windows_dir",
            Box::new(|r| r.windows_dir = Some(r"C:\".into())),
        ),
        (
            "program_files",
            Box::new(|r| r.program_files = vec![r"C:\".into()]),
        ),
        (
            "program_data",
            Box::new(|r| r.program_data = Some(r"C:\".into())),
        ),
        (
            "users_root",
            Box::new(|r| r.users_root = Some(r"C:\".into())),
        ),
        ("app_dirs", Box::new(|r| r.app_dirs = vec![r"C:\".into()])),
        (
            "profile",
            Box::new(|r| {
                if let Some(user) = r.user.as_mut() {
                    user.profile = r"C:\".into();
                }
            }),
        ),
        (
            "temp",
            Box::new(|r| {
                if let Some(user) = r.user.as_mut() {
                    user.temp = Some(r"C:\".into());
                }
            }),
        ),
        (
            "content_folders",
            Box::new(|r| {
                if let Some(user) = r.user.as_mut() {
                    user.content_folders.push(r"C:\".into());
                }
            }),
        ),
    ];
    for (name, mutate) in cases {
        let mut raw = raw_c_ana();
        mutate(&mut raw);
        assert_eq!(
            ProtectionContext::from_raw(raw).err(),
            Some(ContextError::VolumeRootAnchor(name)),
            "caso {name}"
        );
    }
}

#[test]
fn from_raw_should_fail_when_the_profile_is_the_users_root() {
    let mut raw = raw_c_ana();
    if let Some(user) = raw.user.as_mut() {
        user.profile = r"c:\USERS".into();
    }
    assert_eq!(
        ProtectionContext::from_raw(raw).err(),
        Some(ContextError::ProfileIsUsersRoot)
    );
}

#[test]
fn from_raw_should_drop_unnormalizable_optional_content_folders_without_error() {
    let mut raw = raw_c_ana();
    if let Some(user) = raw.user.as_mut() {
        user.temp = Some("nada".into());
        user.content_folders.push(r"\\srv\share\Docs".into());
        user.content_folders.push(String::new());
    }
    let ctx = ProtectionContext::from_raw(raw).unwrap();
    let user = ctx.user().unwrap();
    assert_eq!(user.temp(), None);
    assert_eq!(user.content_folders().len(), 6);
}

#[test]
fn from_raw_should_have_no_user_when_a_user_anchor_fails() {
    let cases: Vec<(&str, Mutation<RawUserFolders>)> = vec![
        (
            "local_app_data",
            Box::new(|u| u.local_app_data = "x".into()),
        ),
        (
            "roaming_app_data",
            Box::new(|u| u.roaming_app_data = String::new()),
        ),
        ("profile", Box::new(|u| u.profile = r"\\srv\x".into())),
    ];
    for (name, mutate) in cases {
        let mut raw = raw_c_ana();
        if let Some(user) = raw.user.as_mut() {
            mutate(user);
        }
        let ctx = ProtectionContext::from_raw(raw).unwrap();
        assert!(ctx.user().is_none(), "caso {name}");
        // El resto del contexto sigue valido: lo ajeno a la sesion se protege con `users_root`.
        assert_eq!(ctx.users_root(), &np(r"C:\Users"));
    }
}

#[test]
fn from_raw_should_have_no_user_when_the_raw_context_has_none() {
    let mut raw = raw_c_ana();
    raw.user = None;
    assert!(ProtectionContext::from_raw(raw).unwrap().user().is_none());
}

#[test]
fn from_raw_should_keep_the_user_source() {
    let mut raw = raw_c_ana();
    if let Some(user) = raw.user.as_mut() {
        user.source = UserSource::InteractiveSession;
    }
    let ctx = ProtectionContext::from_raw(raw).unwrap();
    assert_eq!(
        ctx.user().map(UserContext::source),
        Some(UserSource::InteractiveSession)
    );
}

#[test]
fn context_error_should_describe_the_failing_location() {
    let rendered = ContextError::MissingRequired("program_data").to_string();
    assert!(rendered.contains("program_data"), "{rendered}");
    let rendered = ContextError::VolumeRootAnchor("temp").to_string();
    assert!(rendered.contains("temp"), "{rendered}");
}

#[test]
fn raw_system_drive_should_follow_windows_dir_even_when_the_context_is_incomplete() {
    let mut raw = RawProtectionContext {
        windows_dir: Some(r"d:\Windows".to_owned()),
        ..RawProtectionContext::default()
    };
    assert!(ProtectionContext::from_raw(raw.clone()).is_err());
    assert_eq!(raw.system_drive(), Some('D'));
    raw.windows_dir = Some(r"E:\WinNT".to_owned());
    assert_eq!(raw.system_drive(), Some('E'));
}

#[test]
fn raw_system_drive_should_be_none_when_windows_dir_is_missing_or_not_a_drive_path() {
    let mut raw = RawProtectionContext::default();
    assert_eq!(raw.system_drive(), None);
    raw.windows_dir = Some("Windows".to_owned());
    assert_eq!(raw.system_drive(), None);
    raw.windows_dir = Some(String::new());
    assert_eq!(raw.system_drive(), None);
}

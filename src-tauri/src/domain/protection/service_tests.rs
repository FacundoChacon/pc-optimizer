use std::path::Path;

use super::*;
use crate::domain::ports::fakes::{FakeFileSystem, FsOp};
use crate::domain::ports::{EntryKind, FileIdentity, PortError};
use crate::domain::protection::context::ContextError;
use crate::domain::protection::decision::{
    AllowRule, AnchorKind, Browser, ExceptionId, PathRejection, ProtectedReason, ResolveFailure,
};
use crate::domain::protection::path::{NormalizedPath, normalize_lexical};
use crate::domain::protection::test_support::{ctx_c_ana, ctx_c_user};

use ProtectedReason as R;

const TEMP: &str = r"C:\Users\ana\AppData\Local\Temp";

fn id(n: u128) -> FileIdentity {
    FileIdentity {
        volume_serial: 1,
        file_id: n,
    }
}

fn np(raw: &str) -> NormalizedPath {
    match normalize_lexical(raw) {
        Ok(path) => path,
        Err(rejection) => panic!("{raw:?}: {rejection:?}"),
    }
}

/// Maquina de prueba: perfil de `ana`, Temp con `a.tmp`, Descargas y `C:\Windows\Temp`.
fn base_fs() -> FakeFileSystem {
    FakeFileSystem::new()
        .with_dir(r"C:\Users")
        .with_dir(r"C:\Users\ana")
        .with_dir(r"C:\Users\ana\AppData")
        .with_dir(r"C:\Users\ana\AppData\Local")
        .with_dir(TEMP)
        .with_file(&format!(r"{TEMP}\a.tmp"), 7, None)
        .with_identity(&format!(r"{TEMP}\a.tmp"), id(100))
        .with_file(&format!(r"{TEMP}\b.tmp"), 9, None)
        .with_dir(r"C:\Users\ana\Downloads")
        .with_file(r"C:\Users\ana\Downloads\setup-viejo.exe", 5, None)
        .with_dir(r"C:\Windows")
        .with_dir(r"C:\Windows\Temp")
        .with_file(r"C:\Windows\Temp\x.tmp", 3, None)
        .canonical_as_is()
}

fn service(fs: &FakeFileSystem) -> PathProtection<'_, FakeFileSystem> {
    PathProtection::new(fs, Ok(ctx_c_ana()))
}

fn decide(fs: &FakeFileSystem, raw: &str) -> ProtectionDecision {
    service(fs).check(Path::new(raw))
}

fn protected(reason: ProtectedReason) -> ProtectionDecision {
    ProtectionDecision::Protected(reason)
}

fn allowed(decision: ProtectionDecision) -> CheckedPath {
    match decision {
        ProtectionDecision::Allowed(checked) => checked,
        other => panic!("se esperaba Allowed, dio {other:?}"),
    }
}

// --- 6.1 camino feliz y nivel lexico ---

#[test]
fn check_should_allow_a_plain_file_with_real_path_size_identity_and_default_rule() {
    let fs = base_fs();
    let checked = allowed(decide(&fs, &format!(r"{TEMP}\a.tmp")));
    assert_eq!(
        checked,
        CheckedPath {
            path: np(&format!(r"{TEMP}\a.tmp")),
            kind: EntryKind::File,
            size_bytes: 7,
            identity: id(100),
            rule: AllowRule::Default,
        }
    );
}

#[test]
fn check_should_report_exception_rules_and_cover_directories_too() {
    let fs = base_fs().with_dir(&format!(r"{TEMP}\sub"));
    let windows_temp = allowed(decide(&fs, r"C:\Windows\Temp\x.tmp"));
    assert_eq!(
        windows_temp.rule,
        AllowRule::Exception(ExceptionId::WindowsTemp)
    );
    assert_eq!(windows_temp.size_bytes, 3);
    let download = allowed(decide(&fs, r"C:\Users\ana\Downloads\setup-viejo.exe"));
    assert_eq!(download.rule, AllowRule::Default);
    let directory = allowed(decide(&fs, &format!(r"{TEMP}\sub")));
    assert_eq!(directory.kind, EntryKind::Directory);
}

#[test]
fn check_should_treat_the_verbatim_prefix_like_the_plain_path() {
    let fs = base_fs();
    assert_eq!(
        decide(&fs, r"\\?\C:\Windows\Temp\x.tmp"),
        decide(&fs, r"C:\Windows\Temp\x.tmp")
    );
    assert!(decide(&fs, r"C:/Windows//Temp/./x.tmp").is_allowed());
}

#[test]
fn check_should_protect_lexically_protected_paths_without_touching_the_port() {
    let cases = [
        (r"C:\Windows\System32\kernel32.dll", R::WindowsDir),
        (r"C:\", R::VolumeRoot),
        (r"\\?\C:\Windows\x", R::WindowsDir),
        (r"C:\Users\bob\x", R::OtherUserProfile),
        (r"D:\Temp\a.tmp", R::UnsupportedVolume),
        (r"C:\Windows\Temp", R::Anchor(AnchorKind::ExceptionRoot)),
        (r"c:\wInDoWs\sYsTeM32\x.dll", R::WindowsDir),
        (
            r"C:\Users\ana\AppData\Local\Temp\..\..\..\..\..\Windows\System32\x.dll",
            R::WindowsDir,
        ),
    ];
    for (raw, reason) in cases {
        let fs = base_fs();
        assert_eq!(decide(&fs, raw), protected(reason), "caso {raw:?}");
        assert_eq!(
            fs.total_calls(),
            0,
            "caso {raw:?} no debe consultar el puerto"
        );
    }
}

#[test]
fn check_should_protect_everything_when_the_context_is_unavailable() {
    let fs = base_fs();
    let service = PathProtection::new(&fs, Err(ContextError::MissingRequired("program_data")));
    for raw in [
        format!(r"{TEMP}\a.tmp"),
        r"C:\Windows\Temp\x.tmp".to_string(),
    ] {
        assert_eq!(
            service.check(Path::new(&raw)),
            protected(R::ContextUnavailable),
            "caso {raw:?}"
        );
    }
    assert_eq!(fs.total_calls(), 0);
}

#[test]
fn check_should_reject_unparseable_paths_with_the_matching_reason() {
    let cases = [
        (
            r"C:\Users\ana\AppData\Local\Temp\a.txt:oculto",
            PathRejection::AlternateDataStream,
        ),
        (r"\\servidor\compartido\a.tmp", PathRejection::NetworkPath),
        (
            r"\\?\UNC\servidor\compartido\a.tmp",
            PathRejection::NetworkPath,
        ),
        (r"\\.\C:\Users\ana\a.tmp", PathRejection::DevicePath),
        (
            r"C:\Windows.\System32\x.dll",
            PathRejection::TrailingDotOrSpace,
        ),
        (
            r"C:\Users\ana\AppData\Local\Temp\a.tmp ",
            PathRejection::TrailingDotOrSpace,
        ),
        (
            r"C:\Users\ana\AppData\Local\Temp\nul.txt",
            PathRejection::ReservedDeviceName,
        ),
        (r"Temp\a.tmp", PathRejection::NotAbsolute),
        (r"C:Temp\a.tmp", PathRejection::NotAbsolute),
        (r"C:\..\Windows\x.dll", PathRejection::EscapesRoot),
        (
            r"\\?\C:\Users\ana\..\bob\x.txt",
            PathRejection::VerbatimNotNormalized,
        ),
        ("", PathRejection::Empty),
    ];
    for (raw, rejection) in cases {
        let fs = base_fs();
        assert_eq!(
            decide(&fs, raw),
            protected(R::Unparseable(rejection)),
            "caso {raw:?}"
        );
        assert_eq!(fs.total_calls(), 0, "caso {raw:?}");
    }
}

#[test]
fn check_should_reject_a_non_unicode_path() {
    let fs = base_fs();
    let decision = service(&fs).check(&crate::test_util::non_unicode_path());
    assert_eq!(
        decision,
        protected(R::Unparseable(PathRejection::NonUnicode))
    );
    assert_eq!(fs.total_calls(), 0);
}

// --- 6.2 reparse y nube ---

#[test]
fn check_should_protect_a_path_below_a_junction_ancestor() {
    // `j` apunta a Windows en un caso y a Descargas en otro: nada se atraviesa.
    let fs = base_fs()
        .with_reparse_point(&format!(r"{TEMP}\j"))
        .with_file(&format!(r"{TEMP}\j\System32\x.dll"), 1, None);
    for raw in [
        format!(r"{TEMP}\j\System32\x.dll"),
        format!(r"{TEMP}\j\a.exe"),
    ] {
        assert_eq!(
            decide(&fs, &raw),
            protected(R::ReparsePointInPath),
            "caso {raw:?}"
        );
    }
}

#[test]
fn check_should_skip_cloud_when_an_ancestor_is_a_cloud_reparse_point() {
    let fs = base_fs().with_cloud_reparse_point(&format!(r"{TEMP}\od"));
    let raw = format!(r"{TEMP}\od\video.mp4");
    assert_eq!(
        decide(&fs, &raw),
        ProtectionDecision::SkippedCloud { path: np(&raw) }
    );
}

#[test]
fn check_should_protect_when_an_ancestor_is_not_a_directory() {
    let fs = base_fs();
    let raw = format!(r"{TEMP}\a.tmp\x");
    assert_eq!(
        decide(&fs, &raw),
        protected(R::Unresolvable(ResolveFailure::NotADirectory))
    );
}

#[test]
fn check_should_protect_a_reparse_point_target_but_skip_a_cloud_one() {
    let fs = base_fs()
        .with_reparse_point(&format!(r"{TEMP}\link.dll"))
        .with_reparse_point(&format!(r"{TEMP}\j"))
        .with_cloud_reparse_point(&format!(r"{TEMP}\od.file"))
        .with_cloud_placeholder(&format!(r"{TEMP}\hydrated.bin"), 4);
    for raw in [format!(r"{TEMP}\link.dll"), format!(r"{TEMP}\j")] {
        assert_eq!(
            decide(&fs, &raw),
            protected(R::ReparsePoint),
            "caso {raw:?}"
        );
    }
    for raw in [format!(r"{TEMP}\od.file"), format!(r"{TEMP}\hydrated.bin")] {
        assert_eq!(
            decide(&fs, &raw),
            ProtectionDecision::SkippedCloud { path: np(&raw) },
            "caso {raw:?}"
        );
    }
}

#[test]
fn check_should_let_protection_win_over_the_cloud_marker() {
    let fs = FakeFileSystem::new()
        .with_dir(r"C:\Users")
        .with_dir(r"C:\Users\bob")
        .with_dir(r"C:\Users\bob\OneDrive")
        .with_cloud_placeholder(r"C:\Users\bob\OneDrive\v.mp4", 1)
        .canonical_as_is();
    assert_eq!(
        decide(&fs, r"C:\Users\bob\OneDrive\v.mp4"),
        protected(R::OtherUserProfile)
    );
}

#[test]
fn check_should_protect_files_with_several_hard_links_but_not_directories() {
    let fs = base_fs()
        .with_hard_links(&format!(r"{TEMP}\a.tmp"), 2)
        .with_dir(&format!(r"{TEMP}\d"))
        .with_hard_links(&format!(r"{TEMP}\d"), 5);
    assert_eq!(
        decide(&fs, &format!(r"{TEMP}\a.tmp")),
        protected(R::MultipleHardLinks)
    );
    assert!(decide(&fs, &format!(r"{TEMP}\b.tmp")).is_allowed());
    assert!(decide(&fs, &format!(r"{TEMP}\d")).is_allowed());
}

#[test]
fn check_should_protect_when_identity_or_link_count_is_missing() {
    let fs = base_fs()
        .without_identity(&format!(r"{TEMP}\a.tmp"))
        .with_file(&format!(r"{TEMP}\nolinks.tmp"), 1, None);
    let fs = fs.without_hard_links(&format!(r"{TEMP}\nolinks.tmp"));
    let missing = protected(R::Unresolvable(ResolveFailure::MissingIdentity));
    assert_eq!(decide(&fs, &format!(r"{TEMP}\a.tmp")), missing);
    assert_eq!(decide(&fs, &format!(r"{TEMP}\nolinks.tmp")), missing);
}

#[test]
fn check_should_protect_a_missing_path_and_unreadable_metadata_with_the_failure_kind() {
    let fs = base_fs().fail_on_op(
        TEMP,
        FsOp::Metadata,
        PortError::PermissionDenied("acl".into()),
    );
    assert_eq!(
        decide(&base_fs(), &format!(r"{TEMP}\no-existe.tmp")),
        protected(R::Unresolvable(ResolveFailure::NotFound))
    );
    assert_eq!(
        decide(&fs, &format!(r"{TEMP}\a.tmp")),
        protected(R::Unresolvable(ResolveFailure::PermissionDenied))
    );
}

// --- 6.3 resolucion real ---

fn short_name_fs() -> FakeFileSystem {
    FakeFileSystem::new()
        .with_dir(r"C:\PROGRA~1")
        .with_dir(r"C:\PROGRA~1\App")
        .with_file(r"C:\PROGRA~1\App\a.log", 1, None)
        .with_canonical(r"C:\PROGRA~1\App\a.log", r"C:\Program Files\App\a.log")
        .with_dir(r"C:\Users")
        .with_dir(r"C:\Users\ANAMAR~1")
        .with_dir(r"C:\Users\ANAMAR~1\AppData")
        .with_dir(r"C:\Users\ANAMAR~1\AppData\Local")
        .with_dir(r"C:\Users\ANAMAR~1\AppData\Local\Temp")
        .with_file(r"C:\Users\ANAMAR~1\AppData\Local\Temp\a.tmp", 2, None)
        .with_canonical(
            r"C:\Users\ANAMAR~1\AppData\Local\Temp\a.tmp",
            r"C:\Users\ana maria\AppData\Local\Temp\a.tmp",
        )
}

#[test]
fn check_should_resolve_a_short_name_to_the_protected_real_path() {
    let fs = short_name_fs();
    assert_eq!(
        decide(&fs, r"C:\PROGRA~1\App\a.log"),
        protected(R::ProgramFiles)
    );
}

#[test]
fn check_should_allow_a_short_profile_name_with_the_long_real_path() {
    let fs = short_name_fs();
    let service = PathProtection::new(&fs, Ok(ctx_c_user("ana maria")));
    let checked = allowed(service.check(Path::new(r"C:\Users\ANAMAR~1\AppData\Local\Temp\a.tmp")));
    assert_eq!(
        checked.path.to_string(),
        r"C:\Users\ana maria\AppData\Local\Temp\a.tmp"
    );
    assert_eq!(checked.rule, AllowRule::Default);
}

#[test]
fn check_should_protect_a_short_profile_name_of_another_user() {
    let fs = short_name_fs();
    // El contexto es de `ana`: el alias resuelve al perfil de `ana maria`, que es ajeno.
    assert_eq!(
        decide(&fs, r"C:\Users\ANAMAR~1\AppData\Local\Temp\a.tmp"),
        protected(R::OtherUserProfile)
    );
}

#[test]
fn check_should_protect_when_the_real_path_differs_from_the_lexical_one() {
    let target = format!(r"{TEMP}\a.tmp");
    let mismatches = [
        // Redireccion no detectada por metadatos: otra cantidad de componentes.
        r"C:\Windows\a.tmp".to_string(),
        // Misma cantidad, otro nombre.
        format!(r"{TEMP}\b.tmp"),
        // Otra unidad.
        r"D:\Users\ana\AppData\Local\Temp\a.tmp".to_string(),
        // Sin unidad / UNC / volumen GUID: no interpretable.
        r"\\?\UNC\srv\sh\a.tmp".to_string(),
        r"\\?\Volume{1234}\a.tmp".to_string(),
        r"\Device\HarddiskVolume3\a.tmp".to_string(),
    ];
    for real in mismatches {
        let fs = base_fs().with_canonical(&target, &real);
        assert_eq!(
            decide(&fs, &target),
            protected(R::ResolvedPathMismatch),
            "canonica {real:?}"
        );
    }
}

#[test]
fn check_should_accept_a_real_path_that_only_differs_in_case() {
    let fs = FakeFileSystem::new()
        .with_dir(r"C:\Users")
        .with_dir(r"C:\Users\ana")
        .with_dir(r"C:\Users\ana\AppData")
        .with_dir(r"C:\Users\ana\AppData\Local")
        .with_dir(r"C:\Users\ana\AppData\Local\Temp")
        .with_file(r"C:\Users\ana\AppData\Local\Temp\a.tmp", 2, None)
        .with_dir(r"C:\users")
        .with_dir(r"C:\users\ana")
        .with_dir(r"C:\users\ana\appdata")
        .with_dir(r"C:\users\ana\appdata\local")
        .with_dir(r"C:\users\ana\appdata\local\temp")
        .with_file(r"C:\users\ana\appdata\local\temp\a.tmp", 2, None)
        .with_canonical(
            r"C:\users\ana\appdata\local\temp\a.tmp",
            r"C:\Users\ana\AppData\Local\Temp\a.tmp",
        );
    let checked = allowed(decide(&fs, r"c:\users\ana\appdata\local\temp\a.tmp"));
    assert_eq!(
        checked.path.to_string(),
        r"C:\Users\ana\AppData\Local\Temp\a.tmp"
    );
}

#[test]
fn check_should_protect_when_the_real_path_is_protected_even_if_the_lexical_one_is_not() {
    let fs = FakeFileSystem::new()
        .with_dir(r"C:\Games")
        .with_dir(r"C:\Games\Win")
        .with_file(r"C:\Games\Win\x.dll", 1, None)
        .with_canonical(r"C:\Games\Win\x.dll", r"C:\Windows\Win\x.dll");
    // Misma cantidad de componentes pero distinto nombre: mismatch, nunca allowed.
    assert!(!decide(&fs, r"C:\Games\Win\x.dll").is_allowed());
}

// --- 6.4 fail-closed exhaustivo ---

fn every_port_error() -> [(PortError, ResolveFailure); 5] {
    [
        (PortError::NotFound("x".into()), ResolveFailure::NotFound),
        (
            PortError::PermissionDenied("x".into()),
            ResolveFailure::PermissionDenied,
        ),
        (PortError::InUse("x".into()), ResolveFailure::InUse),
        (
            PortError::Unsupported("x".into()),
            ResolveFailure::Unsupported,
        ),
        (PortError::Io("x".into()), ResolveFailure::Io),
    ]
}

#[test]
fn check_should_never_allow_when_any_port_call_fails_with_any_error() {
    let file = format!(r"{TEMP}\a.tmp");
    let ancestors = [
        r"C:\Users",
        r"C:\Users\ana",
        r"C:\Users\ana\AppData",
        r"C:\Users\ana\AppData\Local",
        TEMP,
    ];
    let mut injected = 0;
    for (error, failure) in every_port_error() {
        let mut calls: Vec<(&str, FsOp)> = ancestors.iter().map(|a| (*a, FsOp::Metadata)).collect();
        calls.push((&file, FsOp::Metadata));
        calls.push((&file, FsOp::Canonicalize));
        for (path, op) in calls {
            let fs = base_fs().fail_on_op(path, op, error.clone());
            let decision = decide(&fs, &file);
            assert_eq!(
                decision,
                protected(R::Unresolvable(failure)),
                "{op:?} sobre {path:?} con {error:?}"
            );
            injected += 1;
        }
        // Falla en todas las operaciones de la ruta (modo `fail_on`).
        let fs = base_fs().fail_on(&file, error.clone());
        assert_eq!(decide(&fs, &file), protected(R::Unresolvable(failure)));
        injected += 1;
    }
    assert_eq!(injected, 5 * 8);
}

// --- 6.5 CheckSession ---

#[test]
fn session_should_query_each_shared_ancestor_only_once() {
    let fs = base_fs();
    let service = service(&fs);
    let session = service.session();
    assert!(
        session
            .check(Path::new(&format!(r"{TEMP}\a.tmp")))
            .is_allowed()
    );
    assert!(
        session
            .check(Path::new(&format!(r"{TEMP}\b.tmp")))
            .is_allowed()
    );
    for ancestor in [r"C:\Users", r"C:\Users\ana", TEMP] {
        assert_eq!(fs.call_count(FsOp::Metadata, ancestor), 1, "{ancestor}");
    }
    // Los objetivos si se consultan siempre.
    assert_eq!(fs.call_count(FsOp::Metadata, &format!(r"{TEMP}\a.tmp")), 1);
    assert_eq!(fs.call_count(FsOp::Metadata, &format!(r"{TEMP}\b.tmp")), 1);
}

#[test]
fn plain_check_should_not_memoize_ancestors() {
    let fs = base_fs();
    let service = service(&fs);
    service.check(Path::new(&format!(r"{TEMP}\a.tmp")));
    service.check(Path::new(&format!(r"{TEMP}\b.tmp")));
    assert_eq!(fs.call_count(FsOp::Metadata, TEMP), 2);
}

#[test]
fn session_should_keep_protecting_below_a_reparse_ancestor() {
    let fs = base_fs()
        .with_reparse_point(&format!(r"{TEMP}\j"))
        .with_file(&format!(r"{TEMP}\j\f1"), 1, None)
        .with_file(&format!(r"{TEMP}\j\f2"), 1, None);
    let service = service(&fs);
    let session = service.session();
    for name in ["f1", "f2"] {
        assert_eq!(
            session.check(Path::new(&format!(r"{TEMP}\j\{name}"))),
            protected(R::ReparsePointInPath),
            "caso {name}"
        );
    }
}

#[test]
fn session_memo_should_not_leak_into_reverify() {
    let fs = base_fs();
    let service = service(&fs);
    let session = service.session();
    let checked = allowed(session.check(Path::new(&format!(r"{TEMP}\a.tmp"))));
    // Despues del escaneo, el directorio padre pasa a ser un enlace.
    fs.replace_with_reparse_point(Path::new(TEMP));
    // La sesion (escaneo) puede seguir confiando en su memo...
    assert!(
        session
            .check(Path::new(&format!(r"{TEMP}\b.tmp")))
            .is_allowed()
    );
    // ...pero la re-verificacion consulta todo de nuevo.
    assert_eq!(service.reverify(&checked), protected(R::ReparsePointInPath));
}

// --- 7.1 reverify ---

fn scanned(fs: &FakeFileSystem, raw: &str) -> CheckedPath {
    allowed(decide(fs, raw))
}

#[test]
fn reverify_should_allow_an_unchanged_entry() {
    let fs = base_fs();
    let checked = scanned(&fs, &format!(r"{TEMP}\a.tmp"));
    assert_eq!(
        service(&fs).reverify(&checked),
        ProtectionDecision::Allowed(checked.clone())
    );
}

#[test]
fn reverify_should_detect_each_kind_of_race() {
    let target = format!(r"{TEMP}\a.tmp");
    type Race = Box<dyn Fn(&FakeFileSystem)>;
    let races: Vec<(&str, Race, ProtectedReason)> = vec![
        (
            "archivo recreado",
            Box::new(|fs| fs.recreate_file(Path::new(&format!(r"{TEMP}\a.tmp")), id(999))),
            R::IdentityChanged,
        ),
        (
            "objetivo reemplazado por enlace",
            Box::new(|fs| fs.replace_with_reparse_point(Path::new(&format!(r"{TEMP}\a.tmp")))),
            R::ReparsePoint,
        ),
        (
            "padre reemplazado por junction",
            Box::new(|fs| fs.replace_with_reparse_point(Path::new(TEMP))),
            R::ReparsePointInPath,
        ),
        (
            "archivo borrado",
            Box::new(|fs| fs.remove(Path::new(&format!(r"{TEMP}\a.tmp")))),
            R::Unresolvable(ResolveFailure::NotFound),
        ),
        (
            "archivo pasa a carpeta con otra identidad",
            Box::new(|fs| fs.replace_with_directory(Path::new(&format!(r"{TEMP}\a.tmp")), id(777))),
            R::IdentityChanged,
        ),
        (
            "archivo pasa a carpeta con la misma identidad",
            Box::new(|fs| fs.replace_with_directory(Path::new(&format!(r"{TEMP}\a.tmp")), id(100))),
            R::IdentityChanged,
        ),
    ];
    for (label, race, reason) in races {
        let fs = base_fs();
        let checked = scanned(&fs, &target);
        race(&fs);
        assert_eq!(
            service(&fs).reverify(&checked),
            protected(reason),
            "caso {label}"
        );
    }
}

// --- 7.2 prepare_delete ---

#[test]
fn prepare_delete_should_hand_out_the_real_path_and_hold_the_pin_until_dropped() {
    let fs = base_fs();
    let checked = scanned(&fs, &format!(r"{TEMP}\a.tmp"));
    let target = service(&fs).prepare_delete(&checked).unwrap();
    assert_eq!(target.path(), Path::new(&format!(r"{TEMP}\a.tmp")));
    assert_eq!(target.checked(), &checked);
    assert_eq!(
        fs.pinned(),
        [
            r"C:\Users",
            r"C:\Users\ana",
            r"C:\Users\ana\AppData",
            r"C:\Users\ana\AppData\Local",
            TEMP
        ]
        .map(std::path::PathBuf::from)
        .to_vec()
    );
    drop(target);
    assert!(fs.pinned().is_empty());
}

#[test]
fn prepare_delete_should_fail_closed_when_the_pin_fails() {
    for (error, _) in every_port_error() {
        let fs = base_fs();
        let checked = scanned(&fs, &format!(r"{TEMP}\a.tmp"));
        let failing = fs.fail_on_op(&format!(r"{TEMP}\a.tmp"), FsOp::Pin, error.clone());
        let result = service(&failing).prepare_delete(&checked);
        assert_eq!(
            result.err(),
            Some(protected(R::Unresolvable(ResolveFailure::PinFailed))),
            "con {error:?}"
        );
        assert!(failing.pinned().is_empty());
    }
}

#[test]
fn prepare_delete_should_pin_before_reverifying() {
    let fs = base_fs();
    let checked = scanned(&fs, &format!(r"{TEMP}\a.tmp"));
    let before = fs.calls().len();
    let _target = service(&fs).prepare_delete(&checked).unwrap();
    let calls = fs.calls();
    assert_eq!(calls[before].0, FsOp::Pin);
    assert!(calls[before + 1..].iter().all(|(op, _)| *op != FsOp::Pin));
    assert!(
        calls.len() > before + 1,
        "la re-verificacion debe consultar el puerto"
    );
}

#[test]
fn prepare_delete_should_release_the_pin_when_reverification_fails() {
    let fs = base_fs();
    let checked = scanned(&fs, &format!(r"{TEMP}\a.tmp"));
    fs.recreate_file(Path::new(&format!(r"{TEMP}\a.tmp")), id(555));
    let result = service(&fs).prepare_delete(&checked);
    assert_eq!(result.err(), Some(protected(R::IdentityChanged)));
    assert!(fs.pinned().is_empty());
}

#[test]
fn prepare_delete_should_report_cloud_and_never_hand_out_a_target() {
    let fs = base_fs().with_cloud_placeholder(&format!(r"{TEMP}\cloud.bin"), 1);
    let checked = CheckedPath {
        path: np(&format!(r"{TEMP}\cloud.bin")),
        kind: EntryKind::File,
        size_bytes: 1,
        identity: id(1),
        rule: AllowRule::Default,
    };
    let result = service(&fs).prepare_delete(&checked);
    assert!(matches!(
        result.err(),
        Some(ProtectionDecision::SkippedCloud { .. })
    ));
    assert!(fs.pinned().is_empty());
}

// --- 7.3 contrato para C-09: check -> prepare_delete -> trash -> drop ---

#[test]
fn delete_flow_should_chain_check_prepare_move_to_trash_and_release() {
    use crate::domain::ports::Trash;
    use crate::domain::ports::fakes::FakeTrash;
    use std::sync::Arc;

    let fs = Arc::new(base_fs());
    let trash = FakeTrash::new(Arc::clone(&fs));
    let service = PathProtection::new(&*fs, Ok(ctx_c_ana()));

    let checked = allowed(service.check(Path::new(&format!(r"{TEMP}\a.tmp"))));
    let target = service.prepare_delete(&checked).unwrap();
    assert!(!fs.pinned().is_empty(), "el pin vive durante el movimiento");
    assert_eq!(trash.move_to_trash(target.path()), Ok(()));
    drop(target);

    assert_eq!(
        trash.moved(),
        vec![std::path::PathBuf::from(format!(r"{TEMP}\a.tmp"))]
    );
    assert!(!fs.exists(Path::new(&format!(r"{TEMP}\a.tmp"))));
    assert!(fs.pinned().is_empty());
}

// --- 8.1 check_rule_root ---

fn rule_fs() -> FakeFileSystem {
    base_fs()
        .with_dir(r"C:\Games")
        .with_dir(r"C:\Program Files")
        .with_dir(r"C:\Program Files\X")
        .with_dir(r"C:\Users\bob")
        .with_dir(r"C:\Users\bob\Downloads")
        .with_dir(r"C:\Users\ana\AppData\Roaming")
        .with_reparse_point(r"C:\Games\junction")
}

fn rule_root(raw: &str) -> Result<NormalizedPath, ProtectedReason> {
    let fs = rule_fs();
    service(&fs).check_rule_root(Path::new(raw))
}

#[test]
fn check_rule_root_should_reject_critical_locations_with_their_reason() {
    let cases = [
        (r"C:\", R::VolumeRoot),
        (r"C:\Users", R::Anchor(AnchorKind::UsersRoot)),
        (r"C:\Windows", R::Anchor(AnchorKind::WindowsDir)),
        (r"C:\Users\ana", R::Anchor(AnchorKind::UserProfile)),
        (
            r"C:\Users\ana\AppData\Local",
            R::Anchor(AnchorKind::LocalAppData),
        ),
        (
            r"C:\Users\ana\AppData",
            R::Anchor(AnchorKind::AncestorOfProtectedRoot),
        ),
        (r"C:\Users\bob\Downloads", R::OtherUserProfile),
        (r"C:\Program Files\X", R::ProgramFiles),
        (r"D:\x", R::UnsupportedVolume),
        (r"C:\Windows\System32", R::WindowsDir),
    ];
    for (raw, reason) in cases {
        assert_eq!(rule_root(raw), Err(reason), "caso {raw:?}");
    }
}

#[test]
fn check_rule_root_should_accept_content_anchors_and_ordinary_folders() {
    let cases = [
        r"C:\Users\ana\Downloads",
        TEMP,
        r"C:\Games",
        r"C:\Windows\Temp",
    ];
    for raw in cases {
        let accepted = rule_root(raw);
        assert_eq!(accepted, Ok(np(raw)), "caso {raw:?}");
    }
}

#[test]
fn check_rule_root_should_reject_a_junction_and_unresolvable_roots() {
    assert_eq!(rule_root(r"C:\Games\junction"), Err(R::ReparsePoint));
    assert_eq!(
        rule_root(r"C:\Games\junction\x"),
        Err(R::ReparsePointInPath)
    );
    assert_eq!(
        rule_root(r"C:\Games\no-existe"),
        Err(R::Unresolvable(ResolveFailure::NotFound))
    );
    assert_eq!(
        rule_root(r"C:\Games\nul"),
        Err(R::Unparseable(PathRejection::ReservedDeviceName))
    );
}

#[test]
fn check_rule_root_should_accept_a_cloud_folder_because_each_item_is_decided_alone() {
    let fs = rule_fs().with_cloud_reparse_point(r"C:\Users\ana\OneDrive");
    let raw = r"C:\Users\ana\OneDrive";
    assert_eq!(service(&fs).check_rule_root(Path::new(raw)), Ok(np(raw)));
}

#[test]
fn check_rule_root_should_fail_closed_without_context() {
    let fs = rule_fs();
    let service = PathProtection::new(&fs, Err(ContextError::MissingRequired("windows_dir")));
    assert_eq!(
        service.check_rule_root(Path::new(r"C:\Games")),
        Err(R::ContextUnavailable)
    );
}

#[test]
fn check_rule_root_should_reject_a_browser_profile_folder() {
    let fs = rule_fs();
    let service = service(&fs);
    let chrome = r"C:\Users\ana\AppData\Local\Google\Chrome\User Data\Default\Network";
    assert!(matches!(
        service.check_rule_root(Path::new(chrome)),
        Err(R::BrowserProfile(Browser::Chrome))
    ));
}

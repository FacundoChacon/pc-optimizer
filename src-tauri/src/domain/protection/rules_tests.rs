use super::*;
use crate::domain::protection::context::ProtectionContext;
use crate::domain::protection::decision::{
    AllowRule, AnchorKind, Browser, ExceptionId, ProtectedReason, RuleVerdict,
};
use crate::domain::protection::path::normalize_lexical;
use crate::domain::protection::test_support::{
    ctx_c_ana, ctx_c_no_user, ctx_c_user, ctx_d_ana, raw_c_ana,
};

use ProtectedReason as R;
use RuleVerdict::{Allowed, Protected};

const LOCAL: &str = r"C:\Users\ana\AppData\Local";
const ROAMING: &str = r"C:\Users\ana\AppData\Roaming";

fn verdict(ctx: &ProtectionContext, raw: &str) -> RuleVerdict {
    match normalize_lexical(raw) {
        Ok(path) => evaluate_rules(ctx, &path),
        Err(rejection) => panic!("{raw:?} no normaliza: {rejection:?}"),
    }
}

fn check_table(ctx: &ProtectionContext, rows: &[(String, RuleVerdict)], label: &str) {
    for (raw, expected) in rows {
        assert_eq!(verdict(ctx, raw), *expected, "{label}: caso {raw:?}");
    }
}

fn rows(rows: &[(&str, RuleVerdict)]) -> Vec<(String, RuleVerdict)> {
    rows.iter().map(|(p, v)| ((*p).to_string(), *v)).collect()
}

// --- 5.2 niveles 1-2 ---

#[test]
fn unsupported_volume_should_win_over_every_other_rule() {
    let ctx = ctx_c_ana();
    for raw in [
        r"D:\x",
        r"D:\",
        r"D:\Windows\System32",
        r"D:\$Recycle.Bin\x",
        r"E:\Users\ana\Downloads",
    ] {
        assert_eq!(
            verdict(&ctx, raw),
            Protected(R::UnsupportedVolume),
            "caso {raw:?}"
        );
    }
}

#[test]
fn volume_root_should_be_protected() {
    assert_eq!(verdict(&ctx_c_ana(), r"C:\"), Protected(R::VolumeRoot));
    assert_eq!(verdict(&ctx_d_ana(), r"D:\"), Protected(R::VolumeRoot));
    assert_eq!(
        verdict(&ctx_d_ana(), r"C:\"),
        Protected(R::UnsupportedVolume)
    );
}

#[test]
fn volume_system_entries_should_be_protected_with_their_contents() {
    let ctx = ctx_c_ana();
    for raw in [
        r"C:\$Recycle.Bin\S-1-5-21-1\$R1.tmp",
        r"C:\$WinREAgent",
        r"C:\$anything",
        r"C:\System Volume Information\x",
        r"c:\SYSTEM VOLUME INFORMATION",
        r"C:\pagefile.sys",
        r"C:\PAGEFILE.SYS",
        r"C:\hiberfil.sys",
        r"C:\swapfile.sys",
        r"C:\DumpStack.log",
        r"C:\DumpStack.log.tmp",
        r"C:\Recovery\x",
        r"C:\Boot\x",
        r"C:\bootmgr",
        r"C:\BOOTNXT",
        r"C:\Config.Msi\x",
        r"C:\Documents and Settings\x",
    ] {
        assert_eq!(
            verdict(&ctx, raw),
            Protected(R::VolumeSystemEntry),
            "caso {raw:?}"
        );
    }
    // Una entrada del mismo nombre que no esta en la raiz no es del sistema.
    assert_eq!(
        verdict(&ctx, r"C:\Games\Boot\x"),
        Allowed(AllowRule::Default)
    );
    assert_eq!(
        verdict(&ctx_d_ana(), r"D:\$Recycle.Bin\x"),
        Protected(R::VolumeSystemEntry)
    );
}

#[test]
fn each_context_anchor_should_be_protected_as_an_anchor() {
    let ctx = ctx_c_ana();
    let cases = [
        (r"C:\Users", AnchorKind::UsersRoot),
        (r"C:\Users\ana", AnchorKind::UserProfile),
        (LOCAL, AnchorKind::LocalAppData),
        (ROAMING, AnchorKind::RoamingAppData),
        (r"C:\Windows", AnchorKind::WindowsDir),
        (r"c:\WINDOWS", AnchorKind::WindowsDir),
        (r"C:\Program Files", AnchorKind::ProgramFiles),
        (r"C:\Program Files (x86)", AnchorKind::ProgramFiles),
        (r"C:\ProgramData", AnchorKind::ProgramData),
        (
            r"C:\Users\ana\AppData\Local\pc-optimizer",
            AnchorKind::AppDir,
        ),
        (r"C:\Users\ana\AppData\Local\Temp", AnchorKind::Temp),
        (r"C:\Users\ana\Desktop", AnchorKind::KnownFolder),
        (r"C:\Users\ana\Documents", AnchorKind::KnownFolder),
        (r"C:\Users\ana\Downloads", AnchorKind::KnownFolder),
        (r"C:\Users\ana\Music", AnchorKind::KnownFolder),
        (r"C:\Users\ana\Pictures", AnchorKind::KnownFolder),
        (r"C:\Users\ana\Videos", AnchorKind::KnownFolder),
    ];
    for (raw, kind) in cases {
        assert_eq!(
            verdict(&ctx, raw),
            Protected(R::Anchor(kind)),
            "caso {raw:?}"
        );
    }
}

#[test]
fn every_exception_root_should_be_an_anchor_itself() {
    let ctx = ctx_c_ana();
    let roots = [
        r"C:\Windows\Temp",
        r"C:\Windows\SoftwareDistribution\Download",
        r"C:\Windows\Minidump",
        r"C:\ProgramData\Microsoft\Windows\WER\ReportArchive",
        r"C:\ProgramData\Microsoft\Windows\WER\ReportQueue",
        r"C:\Users\ana\AppData\Local\Google\Chrome\User Data\Default\Cache",
        r"C:\Users\ana\AppData\Local\Google\Chrome\User Data\Profile 2\Code Cache",
        r"C:\Users\ana\AppData\Local\Google\Chrome\User Data\Default\GPUCache",
        r"C:\Users\ana\AppData\Local\Microsoft\Edge\User Data\Default\Cache",
        r"C:\Users\ana\AppData\Local\BraveSoftware\Brave-Browser\User Data\Default\GPUCache",
        r"C:\Users\ana\AppData\Local\Mozilla\Firefox\Profiles\ab12.default\cache2",
    ];
    for raw in roots {
        assert_eq!(
            verdict(&ctx, raw),
            Protected(R::Anchor(AnchorKind::ExceptionRoot)),
            "caso {raw:?}"
        );
    }
}

#[test]
fn ancestors_of_anchors_and_protected_roots_should_be_protected_too() {
    let ctx = ctx_c_ana();
    for raw in [
        r"C:\Users\ana\AppData",
        r"C:\Users\ana\AppData\Local\Google",
        r"C:\Users\ana\AppData\Local\Google\Chrome",
        r"C:\Users\ana\AppData\Local\Microsoft",
        r"C:\Users\ana\AppData\Local\Mozilla",
        r"C:\Users\ana\AppData\Roaming\Mozilla",
        r"C:\Users\ana\AppData\Roaming\Mozilla\Firefox",
    ] {
        assert_eq!(
            verdict(&ctx, raw),
            Protected(R::Anchor(AnchorKind::AncestorOfProtectedRoot)),
            "caso {raw:?}"
        );
    }
}

#[test]
fn other_user_profiles_should_be_protected_without_exceptions() {
    let ctx = ctx_c_ana();
    for raw in [
        r"C:\Users\bob",
        r"C:\Users\bob\AppData\Local\Temp\a.tmp",
        r"C:\Users\Public",
        r"C:\Users\Public\Documents\a.txt",
        r"C:\Users\Default\NTUSER.DAT",
        r"C:\Users\All Users\x",
        r"C:\Users\anabel\Downloads\x",
        r"C:\Users\bob\AppData\Local\Google\Chrome\User Data\Default\Cache\x",
        r"C:\Users\bob\OneDrive\v.mp4",
    ] {
        assert_eq!(
            verdict(&ctx, raw),
            Protected(R::OtherUserProfile),
            "caso {raw:?}"
        );
    }
}

#[test]
fn everything_under_the_users_root_should_be_foreign_without_a_current_user() {
    let ctx = ctx_c_no_user();
    for raw in [
        r"C:\Users\ana",
        r"C:\Users\ana\Downloads\x.exe",
        r"C:\Users\ana\AppData\Local\Temp\a.tmp",
        r"C:\Users\bob\x",
    ] {
        assert_eq!(
            verdict(&ctx, raw),
            Protected(R::OtherUserProfile),
            "caso {raw:?}"
        );
    }
    assert_eq!(
        verdict(&ctx, r"C:\Users"),
        Protected(R::Anchor(AnchorKind::UsersRoot))
    );
}

// --- 5.3 niveles 3-5 ---

fn exception_rows() -> Vec<(String, RuleVerdict)> {
    let ex = |id| Allowed(AllowRule::Exception(id));
    let cache = |browser| ex(ExceptionId::BrowserCache(browser));
    rows(&[
        (r"C:\Windows\Temp\x.tmp", ex(ExceptionId::WindowsTemp)),
        (r"C:\windows\TEMP\a\b\c.tmp", ex(ExceptionId::WindowsTemp)),
        (
            r"C:\Windows\SoftwareDistribution\Download\abc\x.cab",
            ex(ExceptionId::WindowsUpdateDownload),
        ),
        (r"C:\Windows\Minidump\a.dmp", ex(ExceptionId::Minidump)),
        (
            r"C:\ProgramData\Microsoft\Windows\WER\ReportArchive\Report1\a.wer",
            ex(ExceptionId::WerReportArchive),
        ),
        (
            r"C:\ProgramData\Microsoft\Windows\WER\ReportQueue\r\a.wer",
            ex(ExceptionId::WerReportQueue),
        ),
        (
            r"C:\Users\ana\AppData\Local\Google\Chrome\User Data\Profile 2\Cache\Cache_Data\f_000001",
            cache(Browser::Chrome),
        ),
        (
            r"C:\Users\ana\AppData\Local\Google\Chrome\User Data\Default\Code Cache\js\x",
            cache(Browser::Chrome),
        ),
        (
            r"C:\Users\ana\AppData\Local\Google\Chrome\User Data\Default\GPUCache\x",
            cache(Browser::Chrome),
        ),
        (
            r"C:\Users\ana\AppData\Local\Microsoft\Edge\User Data\Default\Cache\x",
            cache(Browser::Edge),
        ),
        (
            r"C:\Users\ana\AppData\Local\BraveSoftware\Brave-Browser\User Data\Profile 1\Code Cache\x",
            cache(Browser::Brave),
        ),
        (
            r"C:\Users\ana\AppData\Local\Mozilla\Firefox\Profiles\ab12.default\cache2\entries\X",
            cache(Browser::Firefox),
        ),
    ])
}

#[test]
fn every_exception_should_allow_strict_descendants() {
    check_table(&ctx_c_ana(), &exception_rows(), "C:");
    // Misma tabla con el sistema en D:.
    let on_d: Vec<_> = exception_rows()
        .into_iter()
        .map(|(raw, verdict)| (format!("D{}", &raw[1..]), verdict))
        .collect();
    check_table(&ctx_d_ana(), &on_d, "D:");
}

#[test]
fn siblings_of_exceptions_should_be_protected_with_the_reason_of_their_root() {
    let table = rows(&[
        (
            r"C:\Windows\SoftwareDistribution\DataStore\x.edb",
            Protected(R::WindowsDir),
        ),
        (
            r"C:\Windows\SoftwareDistribution\Downloads\x",
            Protected(R::WindowsDir),
        ),
        (r"C:\Windows\Tempx\a", Protected(R::WindowsDir)),
        (
            r"C:\Windows\System32\kernel32.dll",
            Protected(R::WindowsDir),
        ),
        (r"C:\Windows\Logs\CBS\CBS.log", Protected(R::WindowsDir)),
        (r"C:\Windows\Minidump2\a", Protected(R::WindowsDir)),
        (
            r"C:\ProgramData\Microsoft\Windows\WER\Temp\x",
            Protected(R::ProgramData),
        ),
        (
            r"C:\ProgramData\Microsoft\Windows\WER\ReportArchiveX\x",
            Protected(R::ProgramData),
        ),
        (r"C:\ProgramData\Foo\a.bin", Protected(R::ProgramData)),
    ]);
    check_table(&ctx_c_ana(), &table, "C:");
}

#[test]
fn browser_wildcard_should_match_exactly_one_profile_component() {
    let user_data = format!(r"{LOCAL}\Google\Chrome\User Data");
    let chrome = Protected(R::BrowserProfile(Browser::Chrome));
    let cache_ok = Allowed(AllowRule::Exception(ExceptionId::BrowserCache(
        Browser::Chrome,
    )));
    let table: Vec<(String, RuleVerdict)> = vec![
        (user_data.clone(), chrome),
        (format!(r"{user_data}\Default"), chrome),
        (format!(r"{user_data}\Cache"), chrome),
        (format!(r"{user_data}\Cache\x"), chrome),
        (format!(r"{user_data}\Default\Network\Cookies"), chrome),
        (format!(r"{user_data}\Default\Login Data"), chrome),
        (format!(r"{user_data}\Default\History"), chrome),
        (format!(r"{user_data}\Default\Extensions\abc\x"), chrome),
        (format!(r"{user_data}\Default\Cachex\x"), chrome),
        (
            format!(r"{user_data}\Default\Cache"),
            Protected(R::Anchor(AnchorKind::ExceptionRoot)),
        ),
        (format!(r"{user_data}\Default\Cache\x"), cache_ok),
        (format!(r"{user_data}\Default\Cache\a\b\c"), cache_ok),
    ];
    check_table(&ctx_c_ana(), &table, "chrome");
}

#[test]
fn firefox_profiles_should_be_protected_except_the_local_cache2() {
    let firefox = Protected(R::BrowserProfile(Browser::Firefox));
    let table = rows(&[
        (
            r"C:\Users\ana\AppData\Roaming\Mozilla\Firefox\Profiles\ab12.default\logins.json",
            firefox,
        ),
        (
            r"C:\Users\ana\AppData\Roaming\Mozilla\Firefox\Profiles\ab12.default\cache2\x",
            firefox,
        ),
        (
            r"C:\Users\ana\AppData\Local\Mozilla\Firefox\Profiles\ab12.default\prefs.js",
            firefox,
        ),
        (
            r"C:\Users\ana\AppData\Local\Mozilla\Firefox\Profiles\cache2\x",
            firefox,
        ),
    ]);
    check_table(&ctx_c_ana(), &table, "firefox");
}

#[test]
fn protected_roots_should_cover_program_files_app_dirs_and_windows() {
    let table = rows(&[
        (
            r"C:\Program Files (x86)\App\cache\a.tmp",
            Protected(R::ProgramFiles),
        ),
        (r"C:\Program Files\X\y.dll", Protected(R::ProgramFiles)),
        (r"c:\program files\x", Protected(R::ProgramFiles)),
        (
            r"C:\Users\ana\AppData\Local\pc-optimizer\pc-optimizer.db",
            Protected(R::AppDir),
        ),
        (
            r"C:\Users\ana\AppData\Local\pc-optimizer\logs\a.log",
            Protected(R::AppDir),
        ),
        (r"c:\wInDoWs\sYsTeM32\x.dll", Protected(R::WindowsDir)),
    ]);
    check_table(&ctx_c_ana(), &table, "C:");
}

#[test]
fn paths_outside_every_protected_root_should_be_allowed_by_default() {
    let ctx = ctx_c_ana();
    for raw in [
        r"C:\Users\ana\Downloads\x.exe",
        r"C:\Users\ana\Downloads\setup-viejo.exe",
        r"C:\Users\ana\AppData\Local\Temp\a.tmp",
        r"C:\Users\ana\AppData\Local\Temp\sub\b.tmp",
        r"C:\Users\ana\Documents\x.docx",
        r"C:\Users\ana\AppData\Local\Microsoft\Windows\Explorer\thumbcache_32.db",
        r"C:\Users\ana\AppData\Local\CrashDumps\a.dmp",
        r"C:\Games\a.dat",
    ] {
        assert_eq!(
            verdict(&ctx, raw),
            Allowed(AllowRule::Default),
            "caso {raw:?}"
        );
    }
}

#[test]
fn the_same_rows_should_hold_with_the_system_on_d() {
    let table = rows(&[
        (r"D:\Windows\System32\x.dll", Protected(R::WindowsDir)),
        (
            r"D:\Windows\Temp\a.tmp",
            Allowed(AllowRule::Exception(ExceptionId::WindowsTemp)),
        ),
        (
            r"D:\Windows\Temp",
            Protected(R::Anchor(AnchorKind::ExceptionRoot)),
        ),
        (
            r"D:\Users\ana\Downloads",
            Protected(R::Anchor(AnchorKind::KnownFolder)),
        ),
        (r"D:\Users\ana\Downloads\x.exe", Allowed(AllowRule::Default)),
        (
            r"D:\Users\bob\AppData\Local\Temp\a.tmp",
            Protected(R::OtherUserProfile),
        ),
        (r"D:\Program Files (x86)\A\b", Protected(R::ProgramFiles)),
        (r"D:\$Recycle.Bin\x", Protected(R::VolumeSystemEntry)),
        (r"D:\", Protected(R::VolumeRoot)),
        (r"C:\Windows\Temp\a.tmp", Protected(R::UnsupportedVolume)),
        (
            r"C:\Users\ana\Downloads\x.exe",
            Protected(R::UnsupportedVolume),
        ),
    ]);
    check_table(&ctx_d_ana(), &table, "D:");
}

#[test]
fn precedence_should_report_the_first_protected_root_in_the_documented_order() {
    let mut raw = raw_c_ana();
    raw.app_dirs.push(r"C:\Program Files\pc-optimizer".into());
    let ctx = ProtectionContext::from_raw(raw).unwrap();
    assert_eq!(
        verdict(&ctx, r"C:\Program Files\pc-optimizer\app.exe"),
        Protected(R::ProgramFiles)
    );
    assert_eq!(
        verdict(&ctx, r"C:\Program Files\pc-optimizer"),
        Protected(R::Anchor(AnchorKind::AppDir))
    );
}

#[test]
fn an_exception_must_not_allow_a_path_that_another_protected_root_covers() {
    let mut raw = raw_c_ana();
    raw.app_dirs.push(r"C:\Windows\Temp\pco".into());
    let ctx = ProtectionContext::from_raw(raw).unwrap();
    // Protegida (no `Allowed`) y con el motivo de la primera raiz en el orden documentado.
    assert_eq!(
        verdict(&ctx, r"C:\Windows\Temp\pco\x.db"),
        Protected(R::WindowsDir)
    );
    assert_eq!(
        verdict(&ctx, r"C:\Windows\Temp\other\x.tmp"),
        Allowed(AllowRule::Exception(ExceptionId::WindowsTemp))
    );
}

#[test]
fn user_names_should_compare_with_ntfs_folding() {
    let juergen = ctx_c_user("Jürgen");
    assert_eq!(
        verdict(&juergen, r"C:\Users\JÜRGEN\AppData\Local\Temp\a.tmp"),
        Allowed(AllowRule::Default)
    );
    let jose = ctx_c_user("José María");
    assert_eq!(
        verdict(&jose, r"C:\Users\José María\AppData\Local\Temp\a.tmp"),
        Allowed(AllowRule::Default)
    );
    assert_eq!(
        verdict(&jose, r"C:\Users\José\AppData\Local\Temp\a.tmp"),
        Protected(R::OtherUserProfile)
    );
    let strasse = ctx_c_user("Straße");
    assert_eq!(
        verdict(&strasse, r"C:\Users\STRASSE\AppData\Local\Temp\a.tmp"),
        Protected(R::OtherUserProfile)
    );
}

#[test]
fn is_protected_should_be_the_negation_of_allowed() {
    let ctx = ctx_c_ana();
    let allowed = normalize_lexical(r"C:\Users\ana\Downloads\x.exe").unwrap();
    let denied = normalize_lexical(r"C:\Windows\System32\x.dll").unwrap();
    assert!(!is_protected(&ctx, &allowed));
    assert!(is_protected(&ctx, &denied));
}

// --- 5.4 invariantes enumeradas (generador determinista, sin crates nuevas) ---

/// Raices protegidas concretas de un contexto (como texto Win32).
fn protected_roots(ctx: &ProtectionContext) -> Vec<String> {
    let mut roots = vec![
        ctx.windows_dir().to_string(),
        ctx.program_data().to_string(),
    ];
    roots.extend(ctx.program_files().iter().map(ToString::to_string));
    roots.extend(ctx.app_dirs().iter().map(ToString::to_string));
    if let Some(user) = ctx.user() {
        let local = user.local_app_data().to_string();
        let roaming = user.roaming_app_data().to_string();
        roots.push(format!(r"{local}\Google\Chrome\User Data"));
        roots.push(format!(r"{local}\Microsoft\Edge\User Data"));
        roots.push(format!(r"{local}\BraveSoftware\Brave-Browser\User Data"));
        roots.push(format!(r"{roaming}\Mozilla\Firefox\Profiles"));
        roots.push(format!(r"{local}\Mozilla\Firefox\Profiles"));
    }
    roots
}

const SUFFIXES: &[&str] = &[
    r"x",
    r"a\b",
    r"System32\x.dll",
    r"Default\Network\Cookies",
    r"Temp\x.tmp",
    r"SoftwareDistribution\Download\abc\x.cab",
    r"Microsoft\Windows\WER\ReportQueue\r",
    r"Default\Cache\f_1",
    r"Profile 2\Code Cache\js\x",
    r"ab12.default\cache2\entries\X",
    r"ab12.default\logins.json",
    r"deep\er\and\deeper\file.bin",
];

/// Variantes de escritura de una ruta canonica `X:\a\b`.
fn spellings(canonical: &str) -> Vec<(&'static str, String)> {
    let tail = &canonical[3..];
    let drive = &canonical[..3];
    vec![
        ("identity", canonical.to_string()),
        ("upper", canonical.to_uppercase()),
        ("lower", canonical.to_lowercase()),
        ("slashes", canonical.replace('\\', "/")),
        ("dot", format!(r"{drive}.\{tail}")),
        ("dotdot", format!(r"{drive}x\..\{tail}")),
        ("verbatim", format!(r"\\?\{canonical}")),
        ("double", format!(r"{drive}{}", tail.replace('\\', r"\\"))),
        ("mixed", canonical.to_uppercase().replace('\\', "/")),
        ("trailing", format!(r"{canonical}\")),
    ]
}

#[test]
fn verdict_should_not_depend_on_how_the_path_is_spelled() {
    let mut cases = 0_usize;
    for ctx in [ctx_c_ana(), ctx_d_ana()] {
        for root in protected_roots(&ctx) {
            for suffix in SUFFIXES {
                let canonical = format!(r"{root}\{suffix}");
                let expected = verdict(&ctx, &canonical);
                for (label, spelled) in spellings(&canonical) {
                    assert_eq!(
                        verdict(&ctx, &spelled),
                        expected,
                        "variante {label} de {canonical:?}"
                    );
                    cases += 1;
                }
            }
        }
    }
    assert!(cases > 500, "solo se generaron {cases} casos");
}

#[test]
fn no_descendant_of_a_protected_root_should_be_allowed_by_default() {
    let mut cases = 0_usize;
    for ctx in [ctx_c_ana(), ctx_d_ana()] {
        for root in protected_roots(&ctx) {
            for suffix in SUFFIXES {
                let canonical = format!(r"{root}\{suffix}");
                let result = verdict(&ctx, &canonical);
                assert_ne!(result, Allowed(AllowRule::Default), "{canonical:?}");
                cases += 1;
            }
        }
    }
    assert!(cases > 100, "solo se generaron {cases} casos");
}

/// Raiz concreta de una excepcion: cada `*` se reemplaza por un nombre de perfil.
fn concrete_exception_root(ctx: &ProtectionContext, def: &ExceptionDef) -> Option<NormalizedPath> {
    let base = base_path(ctx, def.base)?;
    let parts: Vec<&str> = def
        .pattern
        .iter()
        .map(|segment| match segment {
            Seg::Lit(literal) => *literal,
            Seg::Any => "Profile 9",
        })
        .collect();
    Some(base.joined(&parts))
}

#[test]
fn every_exception_should_be_a_strict_descendant_of_its_protected_root() {
    for ctx in [ctx_c_ana(), ctx_d_ana()] {
        let mut checked = 0;
        for def in EXCEPTIONS {
            let root = concrete_exception_root(&ctx, def).unwrap();
            let covering = covering_roots(&ctx, &root);
            assert!(
                covering.contains(&def.parent),
                "{root}: la raiz {:?} no la cubre ({covering:?})",
                def.parent
            );
            // El padre debe ser ancestro estricto: no coincide con ninguna ancla ni raiz.
            assert_eq!(
                evaluate_rules(&ctx, &root),
                Protected(R::Anchor(AnchorKind::ExceptionRoot)),
                "{root}"
            );
            let child = root.joined(&["x.tmp"]);
            assert_eq!(
                evaluate_rules(&ctx, &child),
                Allowed(AllowRule::Exception(def.id)),
                "{child}"
            );
            checked += 1;
        }
        assert_eq!(checked, EXCEPTIONS.len());
    }
}

#[test]
fn no_exception_root_should_contain_or_equal_a_structural_anchor() {
    for ctx in [ctx_c_ana(), ctx_d_ana()] {
        for def in EXCEPTIONS {
            let root = concrete_exception_root(&ctx, def).unwrap();
            for (anchor, kind) in anchors(&ctx) {
                if kind.class() == crate::domain::protection::decision::AnchorClass::Structural {
                    assert!(
                        !anchor.starts_with(&root),
                        "la excepcion {root} contiene al ancla {kind:?} {anchor}"
                    );
                }
            }
        }
    }
}

#[test]
fn profile_rows_should_behave_the_same_for_unicode_and_spaced_user_names() {
    for user in ["ana", "José María", "Jürgen", "Zoë Çelik"] {
        let ctx = ctx_c_user(user);
        let base = format!(r"C:\Users\{user}");
        let ex_chrome = Allowed(AllowRule::Exception(ExceptionId::BrowserCache(
            Browser::Chrome,
        )));
        let table: Vec<(String, RuleVerdict)> = vec![
            (
                format!(r"{base}\AppData\Local\Temp\a.tmp"),
                Allowed(AllowRule::Default),
            ),
            (
                format!(r"{base}\Downloads\x.exe"),
                Allowed(AllowRule::Default),
            ),
            (
                format!(r"{base}\Downloads"),
                Protected(R::Anchor(AnchorKind::KnownFolder)),
            ),
            (base.clone(), Protected(R::Anchor(AnchorKind::UserProfile))),
            (
                format!(r"{base}\AppData\Local\Google\Chrome\User Data\Default\Cache\x"),
                ex_chrome,
            ),
            (
                format!(r"{base}\AppData\Local\Google\Chrome\User Data\Default\Network\Cookies"),
                Protected(R::BrowserProfile(Browser::Chrome)),
            ),
            (
                format!(r"{base}\AppData\Local\pc-optimizer\pc-optimizer.db"),
                Protected(R::AppDir),
            ),
            (
                r"C:\Users\bob\AppData\Local\Temp\a.tmp".to_string(),
                Protected(R::OtherUserProfile),
            ),
            (
                format!(r"{base}x\AppData\Local\Temp\a.tmp"),
                Protected(R::OtherUserProfile),
            ),
        ];
        check_table(&ctx, &table, user);
    }
}

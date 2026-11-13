use super::*;

fn ok(raw: &str) -> NormalizedPath {
    match normalize_lexical(raw) {
        Ok(path) => path,
        Err(rejection) => panic!("{raw:?} debia normalizar, dio {rejection:?}"),
    }
}

fn rejection(raw: &str) -> PathRejection {
    match normalize_lexical(raw) {
        Ok(path) => panic!("{raw:?} debia rechazarse, dio {path}"),
        Err(rejection) => rejection,
    }
}

// --- 3.1 normalizacion ---

#[test]
fn normalize_should_map_equivalent_spellings_to_the_same_path() {
    let canonical = ok(r"C:\a\b");
    for raw in [
        r"C:\a\b",
        "c:/a//b/",
        r"C:\a\.\b",
        r"C:\a\x\..\b",
        r"\\?\C:\a\b",
        r"C:\\a\b\",
        r"C:/a/b/.",
    ] {
        let path = ok(raw);
        assert_eq!(path, canonical, "caso {raw:?}");
        assert_eq!(path.to_string().to_uppercase(), r"C:\A\B", "caso {raw:?}");
    }
}

#[test]
fn normalize_should_keep_the_original_spelling_for_display() {
    assert_eq!(ok(r"c:\Users\Ana\x").to_string(), r"C:\Users\Ana\x");
    assert_eq!(ok(r"\\?\d:\Temp").to_string(), r"D:\Temp");
}

#[test]
fn normalize_should_accept_the_volume_root_with_no_components() {
    for raw in [r"C:\", "C:/", r"\\?\C:\", r"\\?\C:", r"C:\.", r"C:\a\.."] {
        let path = ok(raw);
        assert!(path.is_volume_root(), "caso {raw:?}");
        assert_eq!(path.to_string(), r"C:\", "caso {raw:?}");
    }
}

#[test]
fn normalize_should_reject_dotdot_above_the_root() {
    for raw in [r"C:\..\a", r"C:\a\..\..\b", r"C:\..", "C:/../Windows/x.dll"] {
        assert_eq!(rejection(raw), PathRejection::EscapesRoot, "caso {raw:?}");
    }
}

#[test]
fn normalize_should_reject_relative_segments_and_slashes_in_verbatim_paths() {
    assert_eq!(
        rejection(r"\\?\C:\a\..\b"),
        PathRejection::VerbatimNotNormalized
    );
    assert_eq!(
        rejection(r"\\?\C:\a\.\b"),
        PathRejection::VerbatimNotNormalized
    );
    assert_eq!(
        rejection(r"\\?\C:\Users\ana\..\bob\x.txt"),
        PathRejection::VerbatimNotNormalized
    );
    assert_eq!(rejection(r"\\?\C:\a/b"), PathRejection::InvalidCharacter);
    assert_eq!(
        rejection(r"\\?\C:\a\\b"),
        PathRejection::VerbatimNotNormalized
    );
}

// --- 3.2 rechazos (una fila por variante de PathRejection) ---

#[test]
fn normalize_should_reject_each_unsupported_form_with_its_reason() {
    let table: &[(&str, PathRejection)] = &[
        ("", PathRejection::Empty),
        ("C:\\a\0b", PathRejection::InvalidCharacter),
        (r"Temp\a.tmp", PathRejection::NotAbsolute),
        ("a.tmp", PathRejection::NotAbsolute),
        (r"\a", PathRejection::NotAbsolute),
        ("C:a", PathRejection::NotAbsolute),
        ("C:", PathRejection::NotAbsolute),
        (r"\\srv\sh\a", PathRejection::NetworkPath),
        ("//srv/sh/a", PathRejection::NetworkPath),
        (r"\\?\UNC\srv\sh\a", PathRejection::NetworkPath),
        (r"\\?\unc\srv\sh\a", PathRejection::NetworkPath),
        (r"\\.\C:\a", PathRejection::DevicePath),
        (r"\??\C:\a", PathRejection::DevicePath),
        (r"\\?\Volume{1234}\a", PathRejection::DevicePath),
        (r"\\?\GLOBALROOT\Device\x", PathRejection::DevicePath),
        ("//?/C:/a", PathRejection::DevicePath),
        (r"C:\a\a.txt:s", PathRejection::AlternateDataStream),
        (r"C:\a\a.txt::$DATA", PathRejection::AlternateDataStream),
        (r"C:\a\a?.txt", PathRejection::InvalidCharacter),
        (r"C:\a\a*", PathRejection::InvalidCharacter),
        (r"C:\a\<", PathRejection::InvalidCharacter),
        (r"C:\a\x>y", PathRejection::InvalidCharacter),
        ("C:\\a\\\"q\"", PathRejection::InvalidCharacter),
        (r"C:\a\p|q", PathRejection::InvalidCharacter),
        ("C:\\a\\x\u{1f}y", PathRejection::InvalidCharacter),
        (r"C:\Windows.\x", PathRejection::TrailingDotOrSpace),
        (r"C:\a\a.tmp ", PathRejection::TrailingDotOrSpace),
        (r"C:\a\...", PathRejection::TrailingDotOrSpace),
        (r"C:\a\nul", PathRejection::ReservedDeviceName),
        (r"C:\a\NUL.txt", PathRejection::ReservedDeviceName),
        (r"C:\a\com1.log", PathRejection::ReservedDeviceName),
        (r"C:\a\LPT9", PathRejection::ReservedDeviceName),
        ("C:\\a\\COM\u{b9}", PathRejection::ReservedDeviceName),
        (r"C:\a\Con.tar.gz", PathRejection::ReservedDeviceName),
        (r"C:\a\aux\b", PathRejection::ReservedDeviceName),
    ];
    for (raw, expected) in table {
        assert_eq!(rejection(raw), *expected, "caso {raw:?}");
    }
}

#[test]
fn normalize_should_accept_names_that_only_resemble_reserved_ones() {
    for raw in [
        r"C:\a\console",
        r"C:\a\com10",
        r"C:\a\nulo.txt",
        r"C:\a\lpt",
        r"C:\a\com",
    ] {
        assert!(normalize_lexical(raw).is_ok(), "caso {raw:?}");
    }
}

#[test]
fn normalize_should_validate_components_even_when_a_dotdot_removes_them() {
    assert_eq!(rejection(r"C:\nul\..\x"), PathRejection::ReservedDeviceName);
    assert_eq!(
        rejection(r"C:\a:s\..\x"),
        PathRejection::AlternateDataStream
    );
}

// --- 3.3 plegado, nombres cortos y forma ---

#[test]
fn folding_should_ignore_case_like_ntfs_for_the_basic_plane() {
    let cases = [
        (r"C:\Users\JÜRGEN\x", r"c:\users\jürgen\X"),
        (r"C:\ä", r"C:\Ä"),
        (r"C:\WiNdOwS\sYsTeM32", r"c:\windows\system32"),
        (r"C:\Ω", r"C:\ω"),
    ];
    for (a, b) in cases {
        assert_eq!(ok(a), ok(b), "caso {a:?} vs {b:?}");
    }
}

#[test]
fn folding_should_not_expand_normalize_nor_touch_other_planes() {
    let different = [
        (r"C:\STRASSE", r"C:\Straße"),
        ("C:\\caf\u{e9}", "C:\\cafe\u{301}"),
        ("C:\\\u{10400}", "C:\\\u{10428}"),
        (r"C:\a", r"C:\b"),
    ];
    for (a, b) in different {
        assert_ne!(ok(a), ok(b), "caso {a:?} vs {b:?}");
    }
}

#[test]
fn starts_with_should_compare_whole_components_after_folding() {
    let base = ok(r"C:\Users\ana");
    assert!(ok(r"c:\USERS\Ana\x").starts_with(&base));
    assert!(base.starts_with(&base));
    assert!(!ok(r"C:\Users\anabel\x").starts_with(&base));
    assert!(!ok(r"D:\Users\ana\x").starts_with(&base));
    assert!(ok(r"C:\Users\ana\x").is_strictly_under(&base));
    assert!(!base.is_strictly_under(&base));
}

#[test]
fn looks_short_should_recognise_8_3_aliases() {
    for name in [
        "PROGRA~1",
        "ANAMAR~1.TXT",
        "a~1",
        "ABCDEF~123456.abc",
        "progra~2",
    ] {
        assert!(looks_short(name), "caso {name:?}");
    }
    for name in [
        "foo~bar",
        "a~",
        "nombre-largo~1x.txt",
        "~1",
        "ABCDEFG~1",
        "a~1.long",
        "a.b~1",
        "plain",
    ] {
        assert!(!looks_short(name), "caso {name:?}");
    }
}

#[test]
fn same_shape_should_allow_only_case_and_8_3_differences() {
    let lexical = ok(r"C:\PROGRA~1\App\a.log");
    assert!(same_shape(&lexical, &ok(r"C:\Program Files\App\a.log")));
    assert!(same_shape(&ok(r"c:\temp\A.TMP"), &ok(r"C:\Temp\a.tmp")));
    assert!(!same_shape(&ok(r"C:\Temp\a.tmp"), &ok(r"C:\Windows\a.tmp")));
    assert!(!same_shape(&ok(r"C:\Temp\a.tmp"), &ok(r"C:\Temp\x\a.tmp")));
    assert!(!same_shape(&ok(r"C:\Temp\a.tmp"), &ok(r"D:\Temp\a.tmp")));
    assert!(!same_shape(&ok(r"C:\x\a.tmp"), &ok(r"C:\Windows\a.tmp")));
}

#[test]
fn prefix_and_parent_should_expose_ancestors_without_the_volume_root() {
    let path = ok(r"C:\a\b\c");
    assert_eq!(path.prefix(0), ok(r"C:\"));
    assert_eq!(path.prefix(2), ok(r"C:\a\b"));
    assert_eq!(path.parent(), Some(ok(r"C:\a\b")));
    assert_eq!(ok(r"C:\").parent(), None);
    assert_eq!(path.joined(&["D", "e"]), ok(r"C:\a\b\c\d\e"));
}

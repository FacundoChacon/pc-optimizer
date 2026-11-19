//! Configuracion de release verificada por tests (C-13, D-07, D-23): version unica, bundle y notas.

#[path = "support/text.rs"]
mod text;

use std::fmt;
use std::path::{Path, PathBuf};

/// Version `MAJOR.MINOR.PATCH` sin prefijo, sin sufijos y sin ceros a la izquierda.
fn parse_semver(text: &str) -> Result<(u64, u64, u64), String> {
    let parts: Vec<&str> = text.split('.').collect();
    if parts.len() != 3 {
        return Err(format!("{text:?} no es MAJOR.MINOR.PATCH"));
    }
    let mut numbers = [0u64; 3];
    for (slot, part) in numbers.iter_mut().zip(&parts) {
        let digits_only = !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit());
        if !digits_only || (part.len() > 1 && part.starts_with('0')) {
            return Err(format!("{text:?} no es MAJOR.MINOR.PATCH"));
        }
        *slot = part
            .parse()
            .map_err(|_| format!("{text:?} tiene un componente fuera de rango"))?;
    }
    Ok((numbers[0], numbers[1], numbers[2]))
}

/// Fuente cuya version difiere de la mayoritaria.
#[derive(Debug, PartialEq, Eq)]
struct VersionMismatch {
    source: String,
    found: String,
    expected: String,
}

impl fmt::Display for VersionMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} declara {} pero el resto de las fuentes declara {}",
            self.source, self.found, self.expected
        )
    }
}

/// Fuentes (nombre, version) que difieren de la version mayoritaria; en empate gana la primera.
fn version_mismatches(sources: &[(&str, &str)]) -> Vec<VersionMismatch> {
    let mut counts: Vec<(&str, usize)> = Vec::new();
    for (_, version) in sources {
        match counts.iter_mut().find(|(seen, _)| seen == version) {
            Some((_, count)) => *count += 1,
            None => counts.push((version, 1)),
        }
    }
    let mut expected = None;
    let mut best = 0;
    for (version, count) in &counts {
        if *count > best {
            best = *count;
            expected = Some(*version);
        }
    }
    let Some(expected) = expected else {
        return Vec::new();
    };
    sources
        .iter()
        .filter(|(_, version)| *version != expected)
        .map(|(source, version)| VersionMismatch {
            source: (*source).to_string(),
            found: (*version).to_string(),
            expected: expected.to_string(),
        })
        .collect()
}

/// Puntos obligatorios de las notas (D-17): etiqueta y textos que deben aparecer todos.
const NOTE_POINTS: [(&str, &[&str]); 10] = [
    ("advertencia de SmartScreen", &["SmartScreen", "sin firma"]),
    (
        "pasos de SmartScreen",
        &["Más información", "Ejecutar de todas formas"],
    ),
    ("UAC en cada arranque", &["UAC", "cada arranque"]),
    ("Windows 11 verificado", &["Windows 11"]),
    ("Windows 10 sin verificar", &["Windows 10", "no verificado"]),
    ("sin red ni telemetría", &["Sin red", "telemetría"]),
    ("WebView2", &["WebView2"]),
    ("carpeta de datos", &[r"%LOCALAPPDATA%\pc-optimizer"]),
    ("borrado de datos", &["Remove-Item"]),
    ("suma SHA-256", &["Get-FileHash", "SHA256SUMS.txt"]),
];

/// Etiquetas de los puntos de `NOTE_POINTS` que la nota no cubre.
fn missing_note_points(notes: &str) -> Vec<&'static str> {
    NOTE_POINTS
        .iter()
        .filter(|(_, needles)| needles.iter().any(|needle| !notes.contains(needle)))
        .map(|(label, _)| *label)
        .collect()
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")))
}

fn read_json(path: &Path) -> Result<serde_json::Value, String> {
    let name = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let text = text::read_text(path).map_err(|error| format!("no se pudo leer {name}: {error}"))?;
    serde_json::from_str(&text).map_err(|error| format!("{name} no es JSON valido: {error}"))
}

fn version_of(value: &serde_json::Value, place: &str) -> Result<String, String> {
    value
        .as_object()
        .and_then(|object| object.get("version"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("{place} no declara `version`"))
}

/// Versiones declaradas en el repo de `root` mas la de `Cargo.toml`, que llega por parametro.
fn declared_mismatches(root: &Path, cargo_version: &str) -> Result<Vec<VersionMismatch>, String> {
    let tauri = version_of(
        &read_json(&root.join("src-tauri/tauri.conf.json"))?,
        "tauri.conf.json",
    )?;
    let package = version_of(&read_json(&root.join("package.json"))?, "package.json")?;
    let lock = read_json(&root.join("package-lock.json"))?;
    let lock_root = version_of(&lock, "package-lock.json")?;
    let lock_package = lock
        .get("packages")
        .and_then(|packages| packages.get(""))
        .ok_or_else(|| "package-lock.json no tiene `packages[\"\"]`".to_string())
        .and_then(|entry| version_of(entry, "package-lock.json packages[\"\"]"))?;
    let sources = [
        ("tauri.conf.json", tauri.as_str()),
        ("package.json", package.as_str()),
        ("package-lock.json", lock_root.as_str()),
        ("package-lock.json packages[\"\"]", lock_package.as_str()),
        ("Cargo.toml", cargo_version),
    ];
    Ok(version_mismatches(&sources))
}

/// Desvios del bundle respecto de D-01, D-02 y D-04; vacio si cumple.
fn bundle_violations(config: &serde_json::Value) -> Vec<String> {
    let bundle = config.get("bundle");
    let nsis = bundle.and_then(|b| b.pointer("/windows/nsis"));
    let mut violations = Vec::new();
    if bundle.and_then(|b| b.get("targets")) != Some(&serde_json::json!(["nsis"])) {
        violations.push("bundle.targets debe ser exactamente [\"nsis\"]".to_string());
    }
    if nsis.and_then(|n| n.get("installMode")) != Some(&serde_json::json!("perMachine")) {
        violations.push("bundle.windows.nsis.installMode debe ser \"perMachine\"".to_string());
    }
    if nsis.and_then(|n| n.get("languages")) != Some(&serde_json::json!(["Spanish"])) {
        violations.push("bundle.windows.nsis.languages debe ser [\"Spanish\"]".to_string());
    }
    let publisher = bundle
        .and_then(|b| b.get("publisher"))
        .and_then(serde_json::Value::as_str);
    if publisher.is_none_or(|text| text.trim().is_empty()) {
        violations.push("bundle.publisher debe ser un texto no vacio".to_string());
    }
    violations
}

#[test]
fn parse_semver_should_accept_only_major_minor_patch() {
    assert_eq!(parse_semver("0.1.0"), Ok((0, 1, 0)));
    assert_eq!(parse_semver("12.30.456"), Ok((12, 30, 456)));
    for invalid in [
        "0.1",
        "v0.1.0",
        "0.1.0-",
        "0.1.0-rc1",
        "0.1.0.0",
        "01.1.0",
        "",
        "a.b.c",
    ] {
        assert!(parse_semver(invalid).is_err(), "debia rechazar {invalid:?}");
    }
}

#[test]
fn version_mismatches_should_name_the_source_and_both_versions() {
    let sources = [
        ("tauri.conf.json", "0.2.0"),
        ("package.json", "0.1.0"),
        ("package-lock.json", "0.1.0"),
        ("Cargo.toml", "0.1.0"),
    ];
    let mismatches = version_mismatches(&sources);
    assert_eq!(mismatches.len(), 1);
    assert_eq!(mismatches[0].source, "tauri.conf.json");
    assert_eq!(mismatches[0].found, "0.2.0");
    assert_eq!(mismatches[0].expected, "0.1.0");
    let message = mismatches[0].to_string();
    assert!(message.contains("tauri.conf.json"), "{message}");
    assert!(
        message.contains("0.2.0") && message.contains("0.1.0"),
        "{message}"
    );
}

#[test]
fn version_mismatches_should_report_two_disagreeing_sources_at_once() {
    let sources = [
        ("tauri.conf.json", "0.1.0"),
        ("package.json", "0.3.0"),
        ("package-lock.json", "0.1.0"),
        ("Cargo.toml", "0.2.0"),
        ("package-lock.json packages[\"\"]", "0.1.0"),
    ];
    let names: Vec<String> = version_mismatches(&sources)
        .into_iter()
        .map(|mismatch| mismatch.source)
        .collect();
    assert_eq!(names, ["package.json", "Cargo.toml"]);
}

#[test]
fn version_mismatches_should_be_empty_when_all_sources_agree() {
    let sources = [
        ("tauri.conf.json", "0.1.0"),
        ("package.json", "0.1.0"),
        ("package-lock.json", "0.1.0"),
        ("Cargo.toml", "0.1.0"),
    ];
    assert!(version_mismatches(&sources).is_empty());
    assert!(version_mismatches(&[]).is_empty());
}

fn write_repo(
    root: &Path,
    tauri: &str,
    package: &str,
    lock_root: &str,
    lock_package: Option<&str>,
) {
    std::fs::create_dir_all(root.join("src-tauri")).expect("crear src-tauri");
    std::fs::write(
        root.join("src-tauri/tauri.conf.json"),
        format!(r#"{{"productName":"x","version":"{tauri}"}}"#),
    )
    .expect("escribir tauri.conf.json");
    std::fs::write(
        root.join("package.json"),
        format!(r#"{{"name":"x","version":"{package}"}}"#),
    )
    .expect("escribir package.json");
    let packages = match lock_package {
        Some(version) => format!(r#""packages":{{"":{{"name":"x","version":"{version}"}}}}"#),
        None => r#""packages":{"":{"name":"x"}}"#.to_string(),
    };
    std::fs::write(
        root.join("package-lock.json"),
        format!(r#"{{"name":"x","version":"{lock_root}",{packages}}}"#),
    )
    .expect("escribir package-lock.json");
}

#[test]
fn declared_mismatches_should_name_package_json_when_it_is_the_only_one_out_of_sync() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_repo(dir.path(), "0.1.0", "0.1.1", "0.1.0", Some("0.1.0"));
    let mismatches = declared_mismatches(dir.path(), "0.1.0").expect("leer las versiones");
    assert_eq!(mismatches.len(), 1);
    assert_eq!(mismatches[0].source, "package.json");
    assert_eq!(mismatches[0].found, "0.1.1");
    assert_eq!(mismatches[0].expected, "0.1.0");
}

#[test]
fn declared_mismatches_should_catch_the_lock_root_and_the_cargo_version() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_repo(dir.path(), "0.1.0", "0.1.0", "0.1.0", Some("0.9.0"));
    let mismatches = declared_mismatches(dir.path(), "0.1.0").expect("leer las versiones");
    let names: Vec<&str> = mismatches.iter().map(|m| m.source.as_str()).collect();
    assert_eq!(names, ["package-lock.json packages[\"\"]"]);

    write_repo(dir.path(), "0.1.0", "0.1.0", "0.1.0", Some("0.1.0"));
    let mismatches = declared_mismatches(dir.path(), "0.2.0").expect("leer las versiones");
    assert_eq!(mismatches.len(), 1);
    assert_eq!(mismatches[0].source, "Cargo.toml");
}

#[test]
fn declared_mismatches_should_fail_with_a_description_when_the_lock_has_no_root_package() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_repo(dir.path(), "0.1.0", "0.1.0", "0.1.0", None);
    let error = declared_mismatches(dir.path(), "0.1.0").expect_err("lock sin version raiz");
    assert!(error.contains("package-lock.json"), "{error}");
    assert!(error.contains("packages"), "{error}");
}

#[test]
fn declared_mismatches_should_fail_with_the_missing_file_name() {
    let dir = tempfile::tempdir().expect("tempdir");
    let error = declared_mismatches(dir.path(), "0.1.0").expect_err("repo vacio");
    assert!(error.contains("tauri.conf.json"), "{error}");
}

#[test]
fn real_repo_should_declare_one_semantic_version_in_every_source() {
    let root = repo_root();
    let mismatches =
        declared_mismatches(&root, env!("CARGO_PKG_VERSION")).expect("leer las versiones reales");
    assert!(mismatches.is_empty(), "{mismatches:?}");
    assert!(parse_semver(env!("CARGO_PKG_VERSION")).is_ok());
}

fn bundle_of(
    targets: &str,
    install_mode: &str,
    languages: &str,
    publisher: &str,
) -> serde_json::Value {
    serde_json::from_str(&format!(
        r#"{{"bundle":{{"targets":{targets},"publisher":{publisher},"windows":{{"nsis":{{"installMode":{install_mode},"languages":{languages}}}}}}}}}"#
    ))
    .expect("JSON de la tabla")
}

#[test]
fn bundle_violations_should_accept_the_nsis_per_machine_spanish_bundle() {
    let config = bundle_of(
        r#"["nsis"]"#,
        r#""perMachine""#,
        r#"["Spanish"]"#,
        r#""PC Optimizer""#,
    );
    assert_eq!(bundle_violations(&config), Vec::<String>::new());
}

#[test]
fn bundle_violations_should_reject_each_deviation_naming_the_field() {
    let cases = [
        (
            r#"["nsis","msi"]"#,
            r#""perMachine""#,
            r#"["Spanish"]"#,
            r#""PC Optimizer""#,
            "targets",
        ),
        (
            r#"["msi"]"#,
            r#""perMachine""#,
            r#"["Spanish"]"#,
            r#""PC Optimizer""#,
            "targets",
        ),
        (
            r#"["nsis"]"#,
            r#""currentUser""#,
            r#"["Spanish"]"#,
            r#""PC Optimizer""#,
            "installMode",
        ),
        (
            r#"["nsis"]"#,
            r#""both""#,
            r#"["Spanish"]"#,
            r#""PC Optimizer""#,
            "installMode",
        ),
        (
            r#"["nsis"]"#,
            r#""perMachine""#,
            r#"["English"]"#,
            r#""PC Optimizer""#,
            "languages",
        ),
        (
            r#"["nsis"]"#,
            r#""perMachine""#,
            r#"["Spanish","English"]"#,
            r#""PC Optimizer""#,
            "languages",
        ),
        (
            r#"["nsis"]"#,
            r#""perMachine""#,
            r#"["Spanish"]"#,
            r#""  ""#,
            "publisher",
        ),
        (
            r#"["nsis"]"#,
            r#""perMachine""#,
            r#"["Spanish"]"#,
            "null",
            "publisher",
        ),
    ];
    for (targets, mode, languages, publisher, field) in cases {
        let violations = bundle_violations(&bundle_of(targets, mode, languages, publisher));
        assert_eq!(
            violations.len(),
            1,
            "{targets} {mode} {languages} {publisher}: {violations:?}"
        );
        assert!(violations[0].contains(field), "{violations:?}");
    }
}

#[test]
fn bundle_violations_should_report_every_missing_section_of_an_empty_config() {
    let violations = bundle_violations(&serde_json::json!({}));
    for field in ["targets", "installMode", "languages", "publisher"] {
        assert!(
            violations.iter().any(|v| v.contains(field)),
            "{field}: {violations:?}"
        );
    }
}

#[test]
fn real_tauri_conf_should_ship_only_the_nsis_per_machine_spanish_installer() {
    let config =
        read_json(&repo_root().join("src-tauri/tauri.conf.json")).expect("tauri.conf.json");
    assert_eq!(bundle_violations(&config), Vec::<String>::new());
}

fn full_notes() -> String {
    [
        "Instalador sin firma: SmartScreen puede avisar. Elegi Más información y luego Ejecutar de todas formas.",
        "La app pide UAC en cada arranque.",
        "Verificado en Windows 11. Windows 10 22H2 no verificado.",
        "Sin red ni telemetría: todo es local. WebView2 puede descargarse si falta.",
        r"Los datos quedan en %LOCALAPPDATA%\pc-optimizer; para borrarlos: Remove-Item -Recurse.",
        "Comprobá la suma con Get-FileHash contra SHA256SUMS.txt (detecta descargas corruptas, no un adjunto reemplazado).",
    ]
    .join("\n")
}

#[test]
fn missing_note_points_should_be_empty_for_a_complete_note() {
    assert_eq!(missing_note_points(&full_notes()), Vec::<&str>::new());
}

#[test]
fn missing_note_points_should_name_each_missing_point() {
    let cases = [
        ("SmartScreen", "advertencia de SmartScreen"),
        ("Ejecutar de todas formas", "pasos de SmartScreen"),
        ("UAC", "UAC en cada arranque"),
        ("cada arranque", "UAC en cada arranque"),
        ("Windows 11", "Windows 11 verificado"),
        ("sin firma", "advertencia de SmartScreen"),
        ("no verificado", "Windows 10 sin verificar"),
        ("telemetría", "sin red ni telemetría"),
        ("WebView2", "WebView2"),
        (r"%LOCALAPPDATA%\pc-optimizer", "carpeta de datos"),
        ("Remove-Item", "borrado de datos"),
        ("SHA256SUMS.txt", "suma SHA-256"),
        ("Get-FileHash", "suma SHA-256"),
    ];
    for (removed, expected_label) in cases {
        let note = full_notes().replace(removed, "");
        let missing = missing_note_points(&note);
        assert!(
            missing.contains(&expected_label),
            "sin {removed:?} debia faltar {expected_label:?}, falta {missing:?}"
        );
    }
    assert_eq!(missing_note_points("").len(), 10);
}

#[test]
fn real_release_notes_should_exist_for_the_declared_version_and_cover_every_point() {
    let root = repo_root();
    let tauri = read_json(&root.join("src-tauri/tauri.conf.json")).expect("tauri.conf.json");
    let version = version_of(&tauri, "tauri.conf.json").expect("version");
    let path = root
        .join(".github")
        .join("release-notes")
        .join(format!("v{version}.md"));
    let text = text::read_text(&path)
        .unwrap_or_else(|error| panic!("faltan las notas {}: {error}", path.display()));
    assert_eq!(missing_note_points(&text), Vec::<&str>::new());
}

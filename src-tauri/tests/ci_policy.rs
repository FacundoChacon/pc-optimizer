//! Politica de los workflows de GitHub Actions como test (C-13, D-14): analisis por lineas, sin crates YAML.

#[path = "support/text.rs"]
mod text;

/// Incumplimiento de una regla, con la linea (desde 1) y su texto.
#[derive(Debug, PartialEq, Eq)]
struct Violation {
    line: usize,
    text: String,
}

/// Lineas numeradas desde 1 sin el comentario final (`# ...`); las lineas de solo comentario quedan vacias.
fn code_lines(yaml: &str) -> impl Iterator<Item = (usize, &str)> {
    yaml.lines().enumerate().map(|(index, line)| {
        let code = match line.find(" #") {
            Some(position) => &line[..position],
            None => line,
        };
        let code = if code.trim_start().starts_with('#') {
            ""
        } else {
            code
        };
        (index + 1, code.trim_end())
    })
}

/// `uses:` de una linea de paso (`uses: x` o `- uses: x`), si la hay.
fn uses_value(code: &str) -> Option<&str> {
    let trimmed = code.trim_start();
    let trimmed = trimmed.strip_prefix("- ").unwrap_or(trimmed).trim_start();
    trimmed.strip_prefix("uses:").map(str::trim)
}

fn is_full_sha(text: &str) -> bool {
    text.len() == 40
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Acciones externas que no estan fijadas por SHA completo de commit (las locales `./` se aceptan).
fn unpinned_actions(yaml: &str) -> Vec<Violation> {
    code_lines(yaml)
        .filter_map(|(line, code)| {
            let value = uses_value(code)?;
            if value.starts_with("./") {
                return None;
            }
            let pinned = value
                .rsplit_once('@')
                .is_some_and(|(_, reference)| is_full_sha(reference));
            (!pinned).then(|| Violation {
                line,
                text: code.trim().to_string(),
            })
        })
        .collect()
}
const SHA: &str = "3d3c42e5aac5ba805825da76410c181273ba90b1";

/// El workflow debe declarar `permissions:` en la columna 0 (no basta con declararlo dentro de un job).
fn missing_top_level_permissions(yaml: &str) -> Vec<Violation> {
    if code_lines(yaml).any(|(_, code)| code.starts_with("permissions:")) {
        return Vec::new();
    }
    vec![Violation {
        line: 1,
        text: "falta `permissions:` en el nivel superior".to_string(),
    }]
}

/// Quita espacios y comillas simples o dobles de los extremos.
fn unquote(text: &str) -> &str {
    text.trim().trim_matches(['"', '\'']).trim()
}

/// Valores de permiso que declara una linea: `clave: valor`, `clave: "valor"` y, para `permissions`,
/// el estilo de flujo `{ a: b, c: d }`. Las claves de texto libre (`run`, `name`, `if`) no declaran permisos.
fn permission_values(code: &str) -> Vec<&str> {
    let trimmed = code.trim_start();
    let trimmed = trimmed.strip_prefix("- ").unwrap_or(trimmed);
    let Some((key, value)) = trimmed.split_once(':') else {
        return Vec::new();
    };
    let key = unquote(key);
    let plain_key = !key.is_empty()
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if !plain_key || matches!(key, "run" | "name" | "if") {
        return Vec::new();
    }
    let value = value.trim();
    match value.strip_prefix('{') {
        Some(inner) if key == "permissions" => inner
            .trim_end_matches('}')
            .split(',')
            .filter_map(|entry| entry.split_once(':'))
            .map(|(_, entry_value)| unquote(entry_value))
            .collect(),
        _ => vec![unquote(value)],
    }
}

/// Permisos `write` (o `write-all`) fuera del job `publish`; `write-all` no se admite en ningun lado.
/// Normaliza comillas y el estilo de flujo (`permissions: { contents: write }`).
fn write_permissions_outside_publish(yaml: &str) -> Vec<Violation> {
    let mut in_jobs = false;
    let mut job: Option<&str> = None;
    let mut violations = Vec::new();
    for (line, code) in code_lines(yaml) {
        if code.is_empty() {
            continue;
        }
        let indent = code.len() - code.trim_start().len();
        if indent == 0 {
            in_jobs = code.starts_with("jobs:");
            job = None;
        } else if in_jobs && indent == 2 && code.trim_end().ends_with(':') {
            job = Some(code.trim().trim_end_matches(':'));
        }
        let forbidden = permission_values(code).iter().any(|value| {
            value.eq_ignore_ascii_case("write-all")
                || (value.eq_ignore_ascii_case("write") && job != Some("publish"))
        });
        if forbidden {
            violations.push(Violation {
                line,
                text: code.trim().to_string(),
            });
        }
    }
    violations
}

/// Disparadores que ejecutan codigo de forks con permisos del repositorio.
fn forbidden_triggers(yaml: &str) -> Vec<Violation> {
    code_lines(yaml)
        .filter(|(_, code)| code.contains("pull_request_target"))
        .map(|(line, code)| Violation {
            line,
            text: code.trim().to_string(),
        })
        .collect()
}

/// Unicas expresiones `${{ }}` admitidas dentro de un `run:`: valores que no controla quien abre el PR.
/// Cualquier otro dato (rama, evento, entradas, salidas de pasos) va por `env:` y se lee como `$VAR`.
const SAFE_RUN_EXPRESSIONS: [&str; 6] = [
    "runner.temp",
    "runner.os",
    "github.sha",
    "github.workspace",
    "github.run_id",
    "github.run_number",
];

/// `true` si el texto trae una expresion `${{ }}` fuera de la lista admitida (o sin cerrar).
fn has_unsafe_expression(text: &str) -> bool {
    let mut rest = text;
    while let Some(start) = rest.find("${{") {
        let after = &rest[start + 3..];
        let Some(end) = after.find("}}") else {
            return true;
        };
        if !SAFE_RUN_EXPRESSIONS.contains(&after[..end].trim()) {
            return true;
        }
        rest = &after[end + 2..];
    }
    false
}

/// Expresiones `${{ }}` fuera de la lista admitida dentro de scripts `run:` (linea, bloque `|`/`>` o continuacion):
/// inyeccion de comandos con datos que controla quien abre el PR. En `if:`, `with:` y `env:` se admiten.
fn untrusted_expressions(yaml: &str) -> Vec<Violation> {
    let raw: Vec<&str> = yaml.lines().collect();
    let mut run_key_column: Option<usize> = None;
    let mut violations = Vec::new();
    for (line, code) in code_lines(yaml) {
        let raw_line = raw[line - 1];
        let indent = indent_of(raw_line);
        let mut offending = None;
        if let Some(column) = run_key_column {
            if raw_line.trim().is_empty() || (code.is_empty() && indent <= column) {
                continue;
            }
            if indent > column {
                // Dentro del script los comentarios tambien los expande GitHub: se mira la linea cruda.
                offending = Some(raw_line);
            } else {
                run_key_column = None;
            }
        }
        if run_key_column.is_none() {
            let trimmed = code.trim_start();
            let step = trimmed.strip_prefix("- ").unwrap_or(trimmed).trim_start();
            if step.starts_with("run:") {
                let column = code.len() - step.len();
                run_key_column = Some(column);
                offending = Some(&raw_line[column..]);
            }
        }
        if offending.is_some_and(has_unsafe_expression) {
            violations.push(Violation {
                line,
                text: raw_line.trim().to_string(),
            });
        }
    }
    violations
}

/// Excepciones de `cargo audit` escritas en el workflow (`--ignore` o IDs `RUSTSEC-`): viven en `audit.toml`, aprobadas.
fn audit_exceptions(yaml: &str) -> Vec<Violation> {
    code_lines(yaml)
        .filter(|(_, code)| {
            code.contains("RUSTSEC-")
                || code
                    .split_whitespace()
                    .any(|word| word == "--ignore" || word.starts_with("--ignore="))
        })
        .map(|(line, code)| Violation {
            line,
            text: code.trim().to_string(),
        })
        .collect()
}

fn indent_of(code: &str) -> usize {
    code.len() - code.trim_start().len()
}

/// Pasos `actions/checkout` cuyo bloque no declara `persist-credentials: false`.
fn checkout_with_persisted_credentials(yaml: &str) -> Vec<Violation> {
    let lines: Vec<(usize, &str)> = code_lines(yaml).collect();
    let mut violations = Vec::new();
    for (index, (line, code)) in lines.iter().enumerate() {
        let is_checkout =
            uses_value(code).is_some_and(|value| value.starts_with("actions/checkout@"));
        if !is_checkout {
            continue;
        }
        let mut start = index;
        while start > 0 && !lines[start].1.trim_start().starts_with("- ") {
            start -= 1;
        }
        let step_indent = indent_of(lines[start].1);
        let end = lines[index + 1..]
            .iter()
            .position(|(_, other)| !other.is_empty() && indent_of(other) <= step_indent)
            .map_or(lines.len(), |offset| index + 1 + offset);
        let safe = lines[start..end]
            .iter()
            .any(|(_, other)| other.trim() == "persist-credentials: false");
        if !safe {
            violations.push(Violation {
                line: *line,
                text: code.trim().to_string(),
            });
        }
    }
    violations
}

#[test]
fn pinned_actions_should_reject_tags_and_short_or_long_hashes_naming_the_line() {
    let rejected = [
        "      - uses: actions/checkout@v4",
        "      - uses: actions/checkout@main",
        "      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b",
        "      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b11",
        "      - uses: actions/checkout@3D3C42E5AAC5BA805825DA76410C181273BA90B1",
        "        uses: actions/checkout",
        "        uses: docker://alpine:3.20",
    ];
    for line in rejected {
        let yaml = format!("jobs:\n  a:\n    steps:\n{line}\n");
        let violations = unpinned_actions(&yaml);
        assert_eq!(violations.len(), 1, "debia rechazar {line:?}");
        assert_eq!(violations[0].line, 4, "{line:?}");
        assert!(violations[0].text.contains("uses:"), "{:?}", violations[0]);
    }
}

#[test]
fn pinned_actions_should_accept_full_hashes_with_comment_and_local_workflows() {
    let accepted = [
        format!("      - uses: actions/checkout@{SHA}"),
        format!("      - uses: actions/checkout@{SHA} # v7.0.1"),
        format!("        uses: actions/cache@{SHA}   # v6.1.0"),
        "    uses: ./.github/workflows/ci.yml".to_string(),
        "      - uses: ./.github/actions/local".to_string(),
        "      # uses: actions/checkout@v4 (comentado)".to_string(),
        "      - run: echo uses actions/checkout@v4".to_string(),
    ];
    for line in accepted {
        let yaml = format!("jobs:\n  a:\n    steps:\n{line}\n");
        assert_eq!(
            unpinned_actions(&yaml),
            Vec::new(),
            "debia aceptar {line:?}"
        );
    }
}

#[test]
fn pinned_actions_should_report_every_offending_line_in_order() {
    let yaml = format!(
        "jobs:\n  a:\n    steps:\n      - uses: actions/checkout@v4\n      - uses: actions/cache@{SHA}\n      - uses: actions/setup-node@v4\n"
    );
    let lines: Vec<usize> = unpinned_actions(&yaml).iter().map(|v| v.line).collect();
    assert_eq!(lines, [4, 6]);
}

#[test]
fn top_level_permissions_should_be_required_in_column_zero() {
    let without = "name: CI\non: push\njobs:\n  rust:\n    runs-on: x\n";
    assert_eq!(missing_top_level_permissions(without).len(), 1);
    let only_in_job = "name: CI\njobs:\n  rust:\n    permissions:\n      contents: read\n";
    assert_eq!(missing_top_level_permissions(only_in_job).len(), 1);
    let commented = "name: CI\n# permissions:\njobs:\n  rust:\n    runs-on: x\n";
    assert_eq!(missing_top_level_permissions(commented).len(), 1);
    let declared = "name: CI\npermissions:\n  contents: read\njobs:\n  rust:\n    runs-on: x\n";
    assert_eq!(missing_top_level_permissions(declared), Vec::new());
}

#[test]
fn write_permissions_should_only_be_allowed_in_the_publish_job() {
    let in_rust =
        "permissions:\n  contents: read\njobs:\n  rust:\n    permissions:\n      contents: write\n";
    let violations = write_permissions_outside_publish(in_rust);
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].line, 6);

    let other_scope = "permissions:\n  contents: read\njobs:\n  bundle:\n    permissions:\n      actions: write\n      id-token: write\n";
    assert_eq!(write_permissions_outside_publish(other_scope).len(), 2);

    let at_top_level = "permissions:\n  contents: write\njobs:\n  publish:\n    runs-on: x\n";
    assert_eq!(write_permissions_outside_publish(at_top_level).len(), 1);

    let write_all = "permissions: write-all\njobs:\n  publish:\n    permissions: write-all\n";
    assert_eq!(write_permissions_outside_publish(write_all).len(), 2);
}

#[test]
fn write_permissions_should_accept_publish_and_read_only_everywhere() {
    let ok = "permissions:\n  contents: read\njobs:\n  rust:\n    permissions:\n      contents: read\n  publish:\n    permissions:\n      contents: write\n  after:\n    runs-on: x\n";
    assert_eq!(write_permissions_outside_publish(ok), Vec::new());
    let wordy = "permissions:\n  contents: read\njobs:\n  rust:\n    steps:\n      - run: echo \"contents: written\"\n";
    assert_eq!(write_permissions_outside_publish(wordy), Vec::new());
}

#[test]
fn pull_request_target_should_be_rejected_but_not_in_comments_or_pull_request() {
    let bad = "on:\n  pull_request_target:\n    branches: [main]\n";
    assert_eq!(forbidden_triggers(bad).len(), 1);
    let inline = "on: [push, pull_request_target]\n";
    assert_eq!(forbidden_triggers(inline).len(), 1);
    assert_eq!(
        forbidden_triggers("on:\n  pull_request:\n  push:\n    branches: [main]\n"),
        Vec::new()
    );
    assert_eq!(
        forbidden_triggers("# nunca pull_request_target\non:\n  pull_request:\n"),
        Vec::new()
    );
}

#[test]
fn untrusted_expressions_should_reject_event_data_and_branch_names_in_scripts() {
    let rejected = [
        "      - run: echo \"${{ github.event.pull_request.title }}\"",
        "      - run: git checkout ${{ github.head_ref }}",
        "        run: echo ${{ github.event.head_commit.message }}",
        "      - run: echo ${{ github.event.issue.body }}",
    ];
    for line in rejected {
        let yaml = format!("jobs:\n  a:\n    steps:\n{line}\n");
        let violations = untrusted_expressions(&yaml);
        assert_eq!(violations.len(), 1, "debia rechazar {line:?}");
        assert_eq!(violations[0].line, 4, "{line:?}");
    }
}

#[test]
fn untrusted_expressions_should_accept_plain_conditions_and_safe_contexts() {
    let accepted = [
        "    if: github.event_name == 'pull_request'",
        "      - run: echo \"${{ github.sha }}\"",
        "        if: github.ref == 'refs/heads/main'",
        "      - run: echo ${{ runner.temp }}",
        "      # run: echo ${{ github.head_ref }}",
    ];
    for line in accepted {
        let yaml = format!("jobs:\n  a:\n    steps:\n{line}\n");
        assert_eq!(untrusted_expressions(&yaml), Vec::new(), "{line:?}");
    }
}

#[test]
fn audit_exceptions_should_be_rejected_in_workflows() {
    let by_flag = "      - run: cargo audit --ignore RUSTSEC-2024-0001\n";
    assert_eq!(audit_exceptions(by_flag).len(), 1);
    let by_id = "      - run: cargo audit -D warnings RUSTSEC-2023-0071\n";
    assert_eq!(audit_exceptions(by_id).len(), 1);
    let flag_only = "      - run: |\n          cargo audit --ignore foo\n";
    assert_eq!(audit_exceptions(flag_only).len(), 1);
    assert_eq!(audit_exceptions("      - run: cargo audit\n"), Vec::new());
    // `--ignored` es del arnes de tests, no una excepcion de auditoria.
    assert_eq!(
        audit_exceptions("      - run: cargo test -- --ignored --exact x\n"),
        Vec::new()
    );
    assert_eq!(
        audit_exceptions("      - run: cargo audit --ignore=RUSTSEC\n").len(),
        1
    );
    assert_eq!(
        audit_exceptions("      # cargo audit --ignore RUSTSEC-2024-0001\n"),
        Vec::new()
    );
}

#[test]
fn checkout_should_require_persist_credentials_false_in_its_own_step() {
    let missing = format!(
        "jobs:\n  a:\n    steps:\n      - uses: actions/checkout@{SHA}\n      - run: echo hola\n"
    );
    assert_eq!(checkout_with_persisted_credentials(&missing).len(), 1);

    let wrong_step = format!(
        "jobs:\n  a:\n    steps:\n      - uses: actions/checkout@{SHA}\n      - uses: actions/cache@{SHA}\n        with:\n          persist-credentials: false\n"
    );
    assert_eq!(checkout_with_persisted_credentials(&wrong_step).len(), 1);

    let true_value = format!(
        "jobs:\n  a:\n    steps:\n      - uses: actions/checkout@{SHA}\n        with:\n          persist-credentials: true\n"
    );
    assert_eq!(checkout_with_persisted_credentials(&true_value).len(), 1);
}

#[test]
fn checkout_should_accept_persist_credentials_false_before_or_after_uses() {
    let after = format!(
        "jobs:\n  a:\n    steps:\n      - uses: actions/checkout@{SHA} # v7.0.1\n        with:\n          persist-credentials: false\n      - run: echo hola\n"
    );
    assert_eq!(checkout_with_persisted_credentials(&after), Vec::new());
    let named = format!(
        "jobs:\n  a:\n    steps:\n      - name: Codigo\n        uses: actions/checkout@{SHA}\n        with:\n          persist-credentials: false\n"
    );
    assert_eq!(checkout_with_persisted_credentials(&named), Vec::new());
    let before = format!(
        "jobs:\n  a:\n    steps:\n      - with:\n          persist-credentials: false\n        uses: actions/checkout@{SHA}\n"
    );
    assert_eq!(checkout_with_persisted_credentials(&before), Vec::new());
}

fn is_yaml(path: &std::path::Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext == "yml" || ext == "yaml")
}

/// Archivos `action.yml` / `action.yaml` de las acciones compuestas, a cualquier profundidad.
fn collect_actions(dir: &std::path::Path, found: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        let path = entry.expect("entrada del directorio").path();
        if path.is_dir() {
            collect_actions(&path, found);
        } else if is_yaml(&path) && path.file_stem().is_some_and(|stem| stem == "action") {
            found.push(path);
        }
    }
}

/// Archivos que ejecuta GitHub bajo `<root>/.github`: workflows (`.yml` y `.yaml`) y acciones compuestas.
/// Devuelve (nombre relativo a `.github`, contenido), ordenado; `actions/` puede no existir.
fn policy_files(root: &std::path::Path) -> Vec<(String, String)> {
    let github = root.join(".github");
    let workflows = github.join("workflows");
    let entries = std::fs::read_dir(&workflows)
        .unwrap_or_else(|error| panic!("no se pudo leer {}: {error}", workflows.display()));
    let mut paths: Vec<std::path::PathBuf> = entries
        .map(|entry| entry.expect("entrada del directorio").path())
        .filter(|path| is_yaml(path))
        .collect();
    collect_actions(&github.join("actions"), &mut paths);
    let mut found: Vec<(String, String)> = paths
        .iter()
        .map(|path| {
            let text = text::read_text(path).expect("leer el archivo");
            let name = path.strip_prefix(&github).map_or_else(
                |_| String::new(),
                |rel| rel.to_string_lossy().replace("\\", "/"),
            );
            (name, text)
        })
        .collect();
    found.sort();
    found
}

/// Workflows reales del repo: (nombre de archivo, contenido); falla si no hay ninguno.
fn real_workflows() -> Vec<(String, String)> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let found = policy_files(&root);
    assert!(
        found.iter().any(|(name, _)| name.starts_with("workflows/")),
        "no hay workflows en {}",
        root.join(".github").join("workflows").display()
    );
    found
}

/// Todas las reglas sobre un archivo, con el nombre de cada una.
fn all_violations(yaml: &str) -> Vec<(&'static str, Vec<Violation>)> {
    vec![
        ("acciones sin fijar", unpinned_actions(yaml)),
        (
            "sin permissions superior",
            missing_top_level_permissions(yaml),
        ),
        (
            "write fuera de publish",
            write_permissions_outside_publish(yaml),
        ),
        ("disparador prohibido", forbidden_triggers(yaml)),
        ("expresion no confiable", untrusted_expressions(yaml)),
        ("excepcion de auditoria", audit_exceptions(yaml)),
        (
            "checkout con credenciales",
            checkout_with_persisted_credentials(yaml),
        ),
    ]
}

fn workflow(name: &str) -> String {
    real_workflows()
        .into_iter()
        .find(|(file, _)| file.strip_prefix("workflows/") == Some(name))
        .map(|(_, text)| text)
        .unwrap_or_else(|| panic!("falta .github/workflows/{name}"))
}

#[test]
fn real_workflows_should_satisfy_every_policy_rule() {
    for (name, yaml) in real_workflows() {
        let checks = all_violations(&yaml);
        for (rule, violations) in checks {
            assert_eq!(violations, Vec::new(), "{name}: {rule}");
        }
    }
}

#[test]
fn ci_workflow_should_declare_the_four_jobs_on_a_pinned_windows_runner() {
    let yaml = workflow("ci.yml");
    for job in ["rust", "frontend", "audit", "bundle"] {
        assert!(
            yaml.contains(&format!("\n  {job}:\n")),
            "falta el job {job}"
        );
    }
    assert_eq!(yaml.matches("runs-on: windows-2025").count(), 4);
    assert_eq!(yaml.matches("timeout-minutes:").count(), 4);
    for trigger in [
        "pull_request:",
        "push:",
        "workflow_dispatch:",
        "workflow_call:",
    ] {
        assert!(yaml.contains(trigger), "falta el disparador {trigger}");
    }
    assert!(yaml.contains("contents: read"));
    assert!(yaml.contains("name: Auditoría de crates de red"));
    assert!(yaml.contains("--test no_network_crates"));
}

#[test]
fn audit_config_should_exist_without_exceptions_and_require_approval_per_entry() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".cargo")
        .join("audit.toml");
    let text = text::read_text(&path)
        .unwrap_or_else(|error| panic!("no se pudo leer {}: {error}", path.display()));
    let entries: Vec<(usize, &str)> = code_lines(&text)
        .filter(|(_, code)| code.contains("RUSTSEC-"))
        .collect();
    assert_eq!(
        entries,
        Vec::new(),
        "audit.toml no admite excepciones sin aprobacion"
    );
    for required in ["aprobacion del dueño", "justificacion", "crate"] {
        assert!(
            text.contains(required),
            "el comentario debe exigir: {required}"
        );
    }
    assert!(text.contains("ignore = []"), "ignore debe empezar vacio");
}

#[test]
fn release_workflow_should_be_manual_draft_only_and_the_only_one_with_write_access() {
    let yaml = workflow("release.yml");
    let code: Vec<&str> = code_lines(&yaml).map(|(_, line)| line).collect();
    let has = |text: &str| code.iter().any(|line| line.contains(text));

    assert!(has("workflow_dispatch:"));
    for forbidden in ["pull_request", "push:", "schedule:", "workflow_call:"] {
        assert!(!has(forbidden), "release.yml no debe usar {forbidden}");
    }
    for job in ["preflight", "ci", "publish"] {
        assert!(
            yaml.contains(&format!("\n  {job}:\n")),
            "falta el job {job}"
        );
    }
    assert!(has("uses: ./.github/workflows/ci.yml"));
    assert!(has("use_cache: \"false\""));
    assert!(has("needs: [preflight, ci]"));
    assert_eq!(
        code.iter()
            .filter(|line| line.trim() == "contents: write")
            .count(),
        1
    );
    assert!(has("refs/heads/main"));
    assert!(has("--draft"));
    assert!(has("--verify-tag"));
    assert!(has("--notes-file"));
    for forbidden in [
        "--draft=false",
        "--latest",
        "gh release edit",
        "tauri-action",
    ] {
        assert!(!has(forbidden), "release.yml no debe usar {forbidden}");
    }

    let ci = workflow("ci.yml");
    assert!(
        !code_lines(&ci).any(|(_, line)| line.contains("contents: write")),
        "ci.yml no necesita escritura"
    );
}

fn write_repo_file(root: &std::path::Path, relative: &str, text: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().expect("carpeta padre")).expect("crear carpetas");
    std::fs::write(path, text).expect("escribir el archivo");
}

#[test]
fn policy_files_should_include_yaml_workflows_and_composite_actions() {
    let repo = tempfile::tempdir().expect("tempdir");
    let root = repo.path();
    write_repo_file(root, ".github/workflows/a.yml", "name: a\n");
    write_repo_file(root, ".github/workflows/b.yaml", "name: b\n");
    write_repo_file(root, ".github/workflows/notas.txt", "no es un workflow\n");
    write_repo_file(root, ".github/actions/uno/action.yml", "name: uno\n");
    write_repo_file(root, ".github/actions/dos/sub/action.yaml", "name: dos\n");
    write_repo_file(root, ".github/actions/dos/otro.yml", "no es action.yml\n");

    let names: Vec<String> = policy_files(root)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(
        names,
        [
            "actions/dos/sub/action.yaml",
            "actions/uno/action.yml",
            "workflows/a.yml",
            "workflows/b.yaml",
        ]
    );
}

#[test]
fn policy_files_should_work_without_an_actions_directory() {
    let repo = tempfile::tempdir().expect("tempdir");
    write_repo_file(repo.path(), ".github/workflows/a.yml", "name: a\n");
    assert_eq!(policy_files(repo.path()).len(), 1);
}

#[test]
fn violations_in_a_yaml_extension_workflow_and_in_a_composite_action_should_be_reported() {
    let repo = tempfile::tempdir().expect("tempdir");
    let root = repo.path();
    let bad = "permissions:\n  contents: read\njobs:\n  a:\n    steps:\n      - uses: actions/checkout@v4\n";
    write_repo_file(root, ".github/workflows/malo.yaml", bad);
    write_repo_file(root, ".github/actions/x/action.yml", bad);
    write_repo_file(
        root,
        ".github/workflows/bueno.yml",
        "permissions:\n  contents: read\n",
    );

    let reported: Vec<String> = policy_files(root)
        .into_iter()
        .filter(|(_, yaml)| {
            all_violations(yaml)
                .iter()
                .any(|(rule, found)| *rule == "acciones sin fijar" && !found.is_empty())
        })
        .map(|(name, _)| name)
        .collect();
    assert_eq!(reported, ["actions/x/action.yml", "workflows/malo.yaml"]);
}

#[test]
fn write_permissions_should_reject_flow_style_quoted_and_write_all_variants_outside_publish() {
    let rejected = [
        "      permissions: { contents: write }",
        "      permissions: {contents: write}",
        "      permissions: { contents: read, packages: write }",
        "      permissions: { \"contents\": \"write\" }",
        "      contents: \"write\"",
        "      contents: 'write'",
        "      \"contents\": write",
        "      permissions: write-all",
        "      permissions: \"write-all\"",
        "      permissions: 'write-all'",
        "      permissions: { all: write-all }",
    ];
    for line in rejected {
        let yaml = format!("permissions:\n  contents: read\njobs:\n  rust:\n{line}\n");
        let violations = write_permissions_outside_publish(&yaml);
        assert_eq!(violations.len(), 1, "debia rechazar {line:?}");
        assert_eq!(violations[0].line, 5, "{line:?}");
    }
}

#[test]
fn write_permissions_should_accept_flow_style_read_only_and_publish_write() {
    let accepted = [
        ("rust", "      permissions: { contents: read }"),
        ("rust", "      permissions: {}"),
        ("rust", "      permissions: read-all"),
        ("rust", "      contents: \"read\""),
        ("rust", "      - run: echo \"a: write\""),
        ("rust", "      - name: write"),
        ("publish", "      permissions: { contents: write }"),
        ("publish", "      contents: \"write\""),
    ];
    for (job, line) in accepted {
        let yaml = format!("permissions:\n  contents: read\njobs:\n  {job}:\n{line}\n");
        assert_eq!(
            write_permissions_outside_publish(&yaml),
            Vec::new(),
            "debia aceptar {job}: {line:?}"
        );
    }
    let publish_all =
        "permissions:\n  contents: read\njobs:\n  publish:\n    permissions: write-all\n";
    assert_eq!(write_permissions_outside_publish(publish_all).len(), 1);
}

#[test]
fn untrusted_expressions_should_reject_everything_outside_the_allowlist_in_run_lines() {
    let rejected = [
        "      - run: echo ${{ github.ref_name }}",
        "      - run: echo ${{ github.head_ref }}",
        "      - run: echo ${{ github.event.pull_request.title }}",
        "      - run: echo ${{ inputs.version }}",
        "      - run: echo ${{ steps.version.outputs.version }}",
        "      - run: echo ${{ needs.preflight.outputs.version }}",
        "      - run: echo ${{ secrets.TOKEN }}",
        "      - run: echo ${{ env.VERSION }}",
        "      - run: echo ${{ github.sha || github.head_ref }}",
        "      - run: echo ${{ format('{0}', github.ref_name) }}",
        "      - run: echo ${{github.ref_name}}",
        "      - run: echo \"${{ github.sha }}\" ${{ inputs.x }}",
        "        run: echo ${{ inputs.x }}",
        "      - run: echo ${{ github.sha",
        "      - run: echo hi # ${{ github.head_ref }}",
    ];
    for line in rejected {
        let yaml = format!("jobs:\n  a:\n    steps:\n{line}\n");
        let violations = untrusted_expressions(&yaml);
        assert_eq!(violations.len(), 1, "debia rechazar {line:?}");
        assert_eq!(violations[0].line, 4, "{line:?}");
    }
}

#[test]
fn untrusted_expressions_should_accept_the_allowlist_and_any_expression_outside_run() {
    let accepted = [
        "      - run: echo ${{ runner.temp }}",
        "      - run: echo ${{ runner.os }} ${{ github.sha }}",
        "      - run: echo ${{github.sha}}",
        "      - run: echo ${{ github.run_id }}-${{ github.run_number }}",
        "      - run: cd ${{ github.workspace }}",
        "        if: github.ref_name == 'main'",
        "        if: ${{ github.head_ref != '' }}",
        "          path: ${{ inputs.path }}",
        "          V: ${{ needs.preflight.outputs.version }}",
        "          version: ${{ steps.version.outputs.version }}",
        "        name: Paso ${{ github.ref_name }}",
        "      - run: echo sin expresiones",
        "      # run: echo ${{ inputs.x }}",
    ];
    for line in accepted {
        let yaml = format!("jobs:\n  a:\n    steps:\n{line}\n");
        assert_eq!(untrusted_expressions(&yaml), Vec::new(), "{line:?}");
    }
}

#[test]
fn untrusted_expressions_should_scan_multiline_run_blocks_until_they_end() {
    let block = "jobs:\n  a:\n    steps:\n      - name: x\n        run: |\n          echo ok\n\n          echo \"${{ inputs.name }}\"\n          # ${{ github.head_ref }}\n      - name: y\n        env:\n          V: ${{ inputs.name }}\n        run: echo \"$V\"\n";
    let lines: Vec<usize> = untrusted_expressions(block)
        .iter()
        .map(|v| v.line)
        .collect();
    assert_eq!(lines, [8, 9]);

    for marker in ["|", "|-", "|+", ">", ">-", "|2"] {
        let yaml = format!(
            "jobs:\n  a:\n    steps:\n      - run: {marker}\n          echo ${{{{ steps.a.outputs.b }}}}\n"
        );
        assert_eq!(untrusted_expressions(&yaml).len(), 1, "{marker}");
    }

    let plain_continuation =
        "jobs:\n  a:\n    steps:\n      - run: echo uno\n          ${{ inputs.x }}\n";
    assert_eq!(untrusted_expressions(plain_continuation).len(), 1);

    let ends_by_dedent = "jobs:\n  a:\n    steps:\n      - run: |\n          echo ok\n        env:\n          V: ${{ steps.a.outputs.b }}\n      - uses: x\n        with:\n          v: ${{ inputs.v }}\n";
    assert_eq!(untrusted_expressions(ends_by_dedent), Vec::new());

    let allowed_in_block = "jobs:\n  a:\n    steps:\n      - run: |\n          cp x ${{ runner.temp }}/y\n          echo ${{ github.sha }}\n";
    assert_eq!(untrusted_expressions(allowed_in_block), Vec::new());
}

#[test]
fn bundle_job_should_remove_stale_installers_before_building() {
    let yaml = workflow("ci.yml");
    let bundle = yaml
        .split("\n  bundle:\n")
        .nth(1)
        .expect("falta el job bundle");
    let clean = bundle
        .find("rm -rf src-tauri/target/release/bundle")
        .expect("el job bundle debe borrar src-tauri/target/release/bundle");
    let build = bundle
        .find("npm run tauri build")
        .expect("el job bundle debe construir con tauri build");
    assert!(clean < build, "el borrado debe ir antes de construir");
    assert!(
        bundle.contains("test \"${#installers[@]}\" -eq 1"),
        "se mantiene la comprobacion de un unico instalador"
    );
}

#[test]
fn audit_job_should_skip_installing_cargo_audit_when_the_cache_restored_it() {
    // El cache restaura solo el binario (sin .crates.toml): un `cargo install` sin
    // condicion falla con "binary `cargo-audit.exe` already exists".
    let yaml = workflow("ci.yml");
    let audit = yaml
        .split("\n  audit:\n")
        .nth(1)
        .and_then(|rest| rest.split("\n  bundle:\n").next())
        .expect("falta el job audit");
    assert!(
        audit.contains("id: cargo-audit-cache"),
        "el paso de cache de cargo-audit necesita id"
    );
    let install = audit
        .split("- name: Instalar cargo-audit")
        .nth(1)
        .and_then(|rest| rest.split("- name:").next())
        .expect("falta el paso Instalar cargo-audit");
    assert!(
        install.contains("if: steps.cargo-audit-cache.outputs.cache-hit != 'true'"),
        "la instalacion debe saltarse cuando el cache restauro el binario"
    );
}

#[test]
fn release_preflight_should_tell_a_free_tag_from_an_api_failure() {
    let yaml = workflow("release.yml");
    let preflight = yaml
        .split("\n  ci:\n")
        .next()
        .expect("falta el job preflight");
    assert!(
        preflight.contains("HTTP 404"),
        "solo un 404 significa tag libre"
    );
    assert!(
        !preflight.contains("> /dev/null 2>&1; then"),
        "cualquier error de la API no puede tomarse por tag inexistente"
    );
}

#[test]
fn release_publish_should_delete_the_tag_it_created_when_a_later_step_fails() {
    let yaml = workflow("release.yml");
    let publish = yaml
        .split("\n  publish:\n")
        .nth(1)
        .expect("falta el job publish");
    assert!(publish.contains("id: tag"), "el paso del tag necesita id");
    assert!(
        publish.contains("if: failure() && steps.tag.outcome == 'success'"),
        "la limpieza solo corre si este run creo el tag"
    );
    assert!(publish.contains("-X DELETE \"repos/$GITHUB_REPOSITORY/git/refs/tags/v$VERSION\""));
    let create = publish.find("gh release create").expect("release create");
    let cleanup = publish.find("-X DELETE").expect("limpieza");
    assert!(
        create < cleanup,
        "la limpieza va despues de crear el release"
    );
}

const LF_WORKFLOW: &str = "permissions:\n  contents: read\njobs:\n  rust:\n    steps:\n      - uses: actions/checkout@v4\n  frontend:\n    steps: []\n  audit:\n    steps: []\n  bundle:\n    steps: []\n";

#[test]
fn policy_files_should_read_a_crlf_workflow_exactly_like_its_lf_version() {
    let lf_repo = tempfile::tempdir().expect("tempdir");
    let crlf_repo = tempfile::tempdir().expect("tempdir");
    write_repo_file(lf_repo.path(), ".github/workflows/ci.yml", LF_WORKFLOW);
    write_repo_file(
        crlf_repo.path(),
        ".github/workflows/ci.yml",
        &LF_WORKFLOW.replace('\n', "\r\n"),
    );
    let lf = policy_files(lf_repo.path());
    let crlf = policy_files(crlf_repo.path());
    assert_eq!(crlf, lf);
    for job in ["rust", "frontend", "audit", "bundle"] {
        assert!(crlf[0].1.contains(&format!("\n  {job}:\n")), "{job}");
    }
    assert_eq!(
        all_violations(&crlf[0].1)
            .into_iter()
            .map(|(rule, found)| (rule, found.len()))
            .collect::<Vec<_>>(),
        all_violations(&lf[0].1)
            .into_iter()
            .map(|(rule, found)| (rule, found.len()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn policy_files_should_normalize_mixed_line_endings_and_leave_lf_untouched() {
    let repo = tempfile::tempdir().expect("tempdir");
    write_repo_file(
        repo.path(),
        ".github/workflows/mixto.yml",
        "a: 1\r\nb: 2\nc: 3\r\n",
    );
    write_repo_file(repo.path(), ".github/workflows/lf.yml", "a: 1\nb: 2\n");
    let found = policy_files(repo.path());
    assert_eq!(found[0].1, "a: 1\nb: 2\n");
    assert_eq!(found[1].1, "a: 1\nb: 2\nc: 3\n");
}

#[test]
fn read_text_should_strip_carriage_returns_from_a_file_on_disk() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("notas.md");
    std::fs::write(&path, "uno\r\ndos\r\n").expect("escribir");
    assert_eq!(text::read_text(&path).expect("leer"), "uno\ndos\n");
}

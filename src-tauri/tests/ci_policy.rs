//! Politica de los workflows de GitHub Actions como test (C-13, D-14): analisis por lineas, sin crates YAML.

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

/// Permisos `write` (o `write-all`) fuera del job `publish`; `write-all` no se admite en ningun lado.
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
        let Some((_, value)) = code.split_once(':') else {
            continue;
        };
        let value = value.trim();
        let forbidden = value == "write-all" || (value == "write" && job != Some("publish"));
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

/// Expresiones `${{ }}` con datos que controla quien abre el PR (evento, nombre de rama): inyeccion en scripts.
fn untrusted_expressions(yaml: &str) -> Vec<Violation> {
    code_lines(yaml)
        .filter(|(_, code)| {
            code.contains("${{")
                && (code.contains("github.event.") || code.contains("github.head_ref"))
        })
        .map(|(line, code)| Violation {
            line,
            text: code.trim().to_string(),
        })
        .collect()
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

/// Workflows reales del repo: (nombre de archivo, contenido); falla si no hay ninguno.
fn real_workflows() -> Vec<(String, String)> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(".github")
        .join("workflows");
    let entries = std::fs::read_dir(&dir)
        .unwrap_or_else(|error| panic!("no se pudo leer {}: {error}", dir.display()));
    let mut found = Vec::new();
    for entry in entries {
        let path = entry.expect("entrada del directorio").path();
        if path.extension().is_some_and(|ext| ext == "yml") {
            let text = std::fs::read_to_string(&path).expect("leer el workflow");
            let name = path
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            found.push((name, text));
        }
    }
    assert!(!found.is_empty(), "no hay workflows en {}", dir.display());
    found.sort();
    found
}

fn workflow(name: &str) -> String {
    real_workflows()
        .into_iter()
        .find(|(file, _)| file == name)
        .map(|(_, text)| text)
        .unwrap_or_else(|| panic!("falta .github/workflows/{name}"))
}

#[test]
fn real_workflows_should_satisfy_every_policy_rule() {
    for (name, yaml) in real_workflows() {
        let checks = [
            ("acciones sin fijar", unpinned_actions(&yaml)),
            (
                "sin permissions superior",
                missing_top_level_permissions(&yaml),
            ),
            (
                "write fuera de publish",
                write_permissions_outside_publish(&yaml),
            ),
            ("disparador prohibido", forbidden_triggers(&yaml)),
            ("expresion no confiable", untrusted_expressions(&yaml)),
            ("excepcion de auditoria", audit_exceptions(&yaml)),
            (
                "checkout con credenciales",
                checkout_with_persisted_credentials(&yaml),
            ),
        ];
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
    let text = std::fs::read_to_string(&path)
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

//! Baseline de seguridad ejecutable: CSP, capabilities y API global de Tauri.
//!
//! Lee `tauri.conf.json` y `capabilities/*.json` del repo (configuracion versionada, no
//! datos del usuario). Las reglas son funciones puras con tests de tabla.

use serde_json::{Value, json};
use std::path::PathBuf;

fn manifest_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn read_json(relative: &str) -> Result<Value, String> {
    let path = manifest_path(relative);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Fuentes de red remotas (esquemas) que nunca pueden aparecer en una CSP.
const REMOTE_SCHEMES: [&str; 4] = ["http:", "https:", "ws:", "wss:"];
/// Unico origen "de red" permitido en produccion: el transporte IPC local de Tauri en Windows.
const IPC_HTTP_ORIGIN: &str = "http://ipc.localhost";
/// Unicas fuentes permitidas en `connect-src` de produccion (canal local al proceso Rust).
const ALLOWED_CONNECT_SOURCES: [&str; 2] = ["ipc:", IPC_HTTP_ORIGIN];

type Directives = Vec<(String, Vec<String>)>;

/// Normaliza una CSP (forma objeto o string) a pares directiva -> fuentes.
fn parse_csp(csp: &Value) -> Option<Directives> {
    match csp {
        Value::Object(map) => Some(
            map.iter()
                .map(|(name, value)| {
                    let sources = match value {
                        Value::String(s) => s.split_whitespace().map(str::to_string).collect(),
                        Value::Array(items) => items
                            .iter()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect(),
                        _ => Vec::new(),
                    };
                    (name.clone(), sources)
                })
                .collect(),
        ),
        Value::String(text) => Some(
            text.split(';')
                .filter_map(|part| {
                    let mut tokens = part.split_whitespace();
                    let name = tokens.next()?.to_string();
                    Some((name, tokens.map(str::to_string).collect()))
                })
                .collect(),
        ),
        _ => None,
    }
}

fn sources_of<'a>(directives: &'a Directives, name: &str) -> Option<&'a Vec<String>> {
    directives.iter().find(|(n, _)| n == name).map(|(_, s)| s)
}

fn is_remote(source: &str) -> bool {
    REMOTE_SCHEMES
        .iter()
        .any(|scheme| source.starts_with(scheme))
}

/// Violaciones de la CSP de produccion; vacio si cumple la baseline.
fn csp_violations(csp: &Value) -> Vec<String> {
    let Some(directives) = parse_csp(csp) else {
        return vec!["csp: ausente o con forma invalida (null)".to_string()];
    };
    let mut violations = Vec::new();

    if !sources_of(&directives, "default-src").is_some_and(|s| s.iter().any(|x| x == "'self'")) {
        violations.push("default-src: debe incluir 'self'".to_string());
    }
    if sources_of(&directives, "object-src").map(Vec::as_slice) != Some(&["'none'".to_string()]) {
        violations.push("object-src: debe ser 'none'".to_string());
    }
    for (name, sources) in &directives {
        for source in sources {
            let forbidden = match source.as_str() {
                "'unsafe-eval'" | "'unsafe-inline'" | "*" => true,
                "data:" => name == "script-src",
                other => is_remote(other) && other != IPC_HTTP_ORIGIN,
            };
            if forbidden {
                violations.push(format!("{name}: fuente prohibida {source}"));
            }
        }
    }
    if let Some(sources) = sources_of(&directives, "connect-src") {
        for source in sources {
            if !ALLOWED_CONNECT_SOURCES.contains(&source.as_str()) {
                violations.push(format!("connect-src: fuente no permitida {source}"));
            }
        }
    }
    violations
}

/// Origenes de red permitidos en desarrollo ademas del canal IPC: solo el dev server local.
const DEV_ALLOWED_PREFIXES: [&str; 2] = ["http://localhost:", "ws://localhost:"];

/// Violaciones de la CSP de desarrollo; vacio si solo relaja lo que necesita el dev server.
fn dev_csp_violations(csp: &Value) -> Vec<String> {
    let Some(directives) = parse_csp(csp) else {
        return vec!["devCsp: ausente o con forma invalida (null)".to_string()];
    };
    let mut violations = Vec::new();
    for (name, sources) in &directives {
        for source in sources {
            let local_dev_origin = DEV_ALLOWED_PREFIXES.iter().any(|prefix| {
                source.strip_prefix(prefix).is_some_and(|port| {
                    !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit())
                })
            });
            let forbidden = match source.as_str() {
                "'unsafe-eval'" | "*" => true,
                other => is_remote(other) && other != IPC_HTTP_ORIGIN && !local_dev_origin,
            };
            if forbidden {
                violations.push(format!("{name}: fuente prohibida en devCsp {source}"));
            }
        }
    }
    violations
}

/// Prefijos de plugins que jamas deben otorgarse a la ventana (fs/shell/red/proceso/updater/opener).
const FORBIDDEN_PERMISSION_PREFIXES: [&str; 6] =
    ["shell:", "fs:", "http:", "process:", "updater:", "opener:"];
/// Prefijo de los permisos que `tauri-build` genera para cada comando propio.
const COMMAND_PERMISSION_PREFIX: &str = "allow-";

/// Identificadores de permisos de una capability (forma string u objeto `{ identifier }`).
fn permission_ids(capability: &Value) -> Vec<String> {
    capability["permissions"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| match item {
                    Value::String(id) => Some(id.clone()),
                    other => other["identifier"].as_str().map(str::to_string),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Violaciones de una capability: permisos de plugins peligrosos y ventanas distintas de `main`.
fn forbidden_permissions(capability: &Value) -> Vec<String> {
    let mut violations: Vec<String> = permission_ids(capability)
        .into_iter()
        .filter(|id| {
            FORBIDDEN_PERMISSION_PREFIXES
                .iter()
                .any(|p| id.starts_with(p))
        })
        .map(|id| format!("permiso prohibido: {id}"))
        .collect();
    if capability["windows"] != json!(["main"]) {
        violations.push(format!(
            "windows debe ser [\"main\"], es {}",
            capability["windows"]
        ));
    }
    violations
}

/// Diferencias entre los permisos `allow-*` otorgados y los comandos propios (kebab-case).
fn permission_set_mismatches(capabilities: &[Value], commands: &[&str]) -> Vec<String> {
    let granted: std::collections::BTreeSet<String> = capabilities
        .iter()
        .flat_map(permission_ids)
        .filter(|id| id.starts_with(COMMAND_PERMISSION_PREFIX))
        .collect();
    let expected: std::collections::BTreeSet<String> = commands
        .iter()
        .map(|c| format!("{COMMAND_PERMISSION_PREFIX}{}", c.replace('_', "-")))
        .collect();
    let extra = granted
        .difference(&expected)
        .map(|p| format!("permiso no esperado: {p}"));
    let missing = expected
        .difference(&granted)
        .map(|p| format!("permiso faltante: {p}"));
    extra.chain(missing).collect()
}

#[cfg(test)]
mod csp_tests {
    use super::*;

    fn valid_csp() -> Value {
        json!({
            "default-src": "'self'",
            "script-src": "'self'",
            "style-src": "'self'",
            "img-src": "'self'",
            "font-src": "'self'",
            "connect-src": "ipc: http://ipc.localhost",
            "object-src": "'none'",
            "base-uri": "'self'",
            "form-action": "'none'",
            "frame-ancestors": "'none'"
        })
    }

    fn with(directive: &str, value: &str) -> Value {
        let mut csp = valid_csp();
        csp[directive] = json!(value);
        csp
    }

    #[test]
    fn csp_violations_should_be_empty_for_valid_csp() {
        assert_eq!(csp_violations(&valid_csp()), Vec::<String>::new());
    }

    #[test]
    fn csp_violations_should_accept_string_form() {
        let csp = json!(
            "default-src 'self'; script-src 'self'; connect-src ipc: http://ipc.localhost; object-src 'none'"
        );
        assert_eq!(csp_violations(&csp), Vec::<String>::new());
    }

    #[test]
    fn csp_violations_should_report_each_forbidden_source() {
        let cases = [
            (
                with("script-src", "'self' 'unsafe-eval'"),
                "script-src",
                "unsafe-eval",
            ),
            (
                with("script-src", "'self' 'unsafe-inline'"),
                "script-src",
                "unsafe-inline",
            ),
            (
                with("style-src", "'self' 'unsafe-inline'"),
                "style-src",
                "unsafe-inline",
            ),
            (
                with("img-src", "'self' https://cdn.example"),
                "img-src",
                "https://cdn.example",
            ),
            (with("script-src", "'self' data:"), "script-src", "data:"),
            (with("font-src", "*"), "font-src", "*"),
            (
                with(
                    "connect-src",
                    "ipc: http://ipc.localhost https://api.example",
                ),
                "connect-src",
                "https://api.example",
            ),
            (
                with(
                    "connect-src",
                    "ipc: http://ipc.localhost ws://localhost:1420",
                ),
                "connect-src",
                "ws://localhost:1420",
            ),
            (with("connect-src", "ipc: https:"), "connect-src", "https:"),
        ];
        for (csp, directive, source) in cases {
            let violations = csp_violations(&csp);
            assert!(
                violations
                    .iter()
                    .any(|v| v.contains(directive) && v.contains(source)),
                "esperaba violacion de {directive} {source}, obtuve {violations:?}"
            );
        }
    }

    #[test]
    fn csp_violations_should_require_object_src_none_and_default_src_self() {
        let mut no_object = valid_csp();
        no_object.as_object_mut().unwrap().remove("object-src");
        assert!(
            csp_violations(&no_object)
                .iter()
                .any(|v| v.contains("object-src"))
        );

        assert!(
            csp_violations(&with("object-src", "'self'"))
                .iter()
                .any(|v| v.contains("object-src"))
        );

        let mut no_default = valid_csp();
        no_default.as_object_mut().unwrap().remove("default-src");
        assert!(
            csp_violations(&no_default)
                .iter()
                .any(|v| v.contains("default-src"))
        );
    }

    #[test]
    fn csp_violations_should_flag_missing_csp() {
        assert!(!csp_violations(&Value::Null).is_empty());
    }
}

#[test]
fn real_tauri_conf_should_have_strict_production_csp() {
    let conf = read_json("tauri.conf.json").unwrap();
    let violations = csp_violations(&conf["app"]["security"]["csp"]);
    assert!(
        violations.is_empty(),
        "CSP de produccion invalida: {violations:?}"
    );
}

#[cfg(test)]
mod dev_csp_tests {
    use super::*;

    fn valid_dev_csp() -> Value {
        json!({
            "default-src": "'self'",
            "script-src": "'self' 'unsafe-inline'",
            "style-src": "'self' 'unsafe-inline'",
            "img-src": "'self' data:",
            "font-src": "'self'",
            "connect-src": "ipc: http://ipc.localhost ws://localhost:1420 http://localhost:1420",
            "object-src": "'none'"
        })
    }

    fn with(directive: &str, value: &str) -> Value {
        let mut csp = valid_dev_csp();
        csp[directive] = json!(value);
        csp
    }

    #[test]
    fn dev_csp_violations_should_accept_localhost_dev_server_and_ipc() {
        assert_eq!(dev_csp_violations(&valid_dev_csp()), Vec::<String>::new());
    }

    #[test]
    fn dev_csp_violations_should_accept_string_form() {
        let csp = json!("default-src 'self'; connect-src ipc: ws://localhost:1420");
        assert_eq!(dev_csp_violations(&csp), Vec::<String>::new());
    }

    #[test]
    fn dev_csp_violations_should_reject_unsafe_eval_and_remote_origins() {
        let cases = [
            (with("script-src", "'self' 'unsafe-eval'"), "unsafe-eval"),
            (
                with("connect-src", "ipc: https://evil.example"),
                "https://evil.example",
            ),
            (
                with("img-src", "'self' http://cdn.example"),
                "http://cdn.example",
            ),
            (
                with("connect-src", "ws://evil.example:1420"),
                "ws://evil.example:1420",
            ),
            (
                with("connect-src", "http://localhost.evil.example:1420"),
                "localhost.evil.example",
            ),
            (with("font-src", "*"), "*"),
        ];
        for (csp, needle) in cases {
            let violations = dev_csp_violations(&csp);
            assert!(
                violations.iter().any(|v| v.contains(needle)),
                "esperaba violacion con {needle}, obtuve {violations:?}"
            );
        }
    }

    #[test]
    fn dev_csp_violations_should_flag_missing_dev_csp() {
        assert!(!dev_csp_violations(&Value::Null).is_empty());
    }
}

#[test]
fn real_tauri_conf_should_have_localhost_only_dev_csp() {
    let conf = read_json("tauri.conf.json").unwrap();
    let violations = dev_csp_violations(&conf["app"]["security"]["devCsp"]);
    assert!(violations.is_empty(), "devCsp invalida: {violations:?}");
}

#[test]
fn real_tauri_conf_should_disable_global_tauri_and_freeze_prototype() {
    let conf = read_json("tauri.conf.json").unwrap();
    assert_eq!(conf["app"]["withGlobalTauri"], json!(false));
    assert_eq!(conf["app"]["security"]["freezePrototype"], json!(true));
}

#[cfg(test)]
mod capability_tests {
    use super::*;

    fn capability(windows: Value, permissions: Value) -> Value {
        json!({ "identifier": "main-window", "windows": windows, "permissions": permissions })
    }

    #[test]
    fn forbidden_permissions_should_be_empty_for_minimal_capability() {
        let cap = capability(
            json!(["main"]),
            json!(["allow-get-app-info", "core:event:default"]),
        );
        assert_eq!(forbidden_permissions(&cap), Vec::<String>::new());
    }

    #[test]
    fn forbidden_permissions_should_report_each_dangerous_plugin_permission() {
        for permission in [
            "shell:allow-execute",
            "fs:default",
            "http:default",
            "process:default",
            "updater:default",
            "opener:default",
        ] {
            let cap = capability(json!(["main"]), json!(["allow-get-app-info", permission]));
            let violations = forbidden_permissions(&cap);
            assert!(
                violations.iter().any(|v| v.contains(permission)),
                "esperaba violacion de {permission}, obtuve {violations:?}"
            );
        }
    }

    #[test]
    fn forbidden_permissions_should_inspect_object_form_permissions() {
        let cap = capability(
            json!(["main"]),
            json!([{ "identifier": "fs:allow-read-file", "allow": [] }]),
        );
        assert!(
            forbidden_permissions(&cap)
                .iter()
                .any(|v| v.contains("fs:allow-read-file"))
        );
    }

    #[test]
    fn forbidden_permissions_should_require_main_window_only() {
        for windows in [
            json!(["other"]),
            json!(["main", "other"]),
            json!(["*"]),
            json!([]),
        ] {
            let cap = capability(windows.clone(), json!([]));
            assert!(
                forbidden_permissions(&cap)
                    .iter()
                    .any(|v| v.contains("windows")),
                "windows = {windows}"
            );
        }
    }

    #[test]
    fn permission_set_mismatches_should_be_empty_when_sets_match() {
        let caps = [capability(
            json!(["main"]),
            json!(["allow-get-app-info", "allow-two-words"]),
        )];
        assert_eq!(
            permission_set_mismatches(&caps, &["get_app_info", "two_words"]),
            Vec::<String>::new()
        );
    }

    #[test]
    fn permission_set_mismatches_should_report_extra_permission() {
        let caps = [capability(
            json!(["main"]),
            json!(["allow-get-app-info", "allow-delete-everything"]),
        )];
        let mismatches = permission_set_mismatches(&caps, &["get_app_info"]);
        assert!(
            mismatches
                .iter()
                .any(|m| m.contains("allow-delete-everything")),
            "{mismatches:?}"
        );
    }

    #[test]
    fn permission_set_mismatches_should_report_missing_permission() {
        let caps = [capability(json!(["main"]), json!(["allow-get-app-info"]))];
        let mismatches = permission_set_mismatches(&caps, &["get_app_info", "other_command"]);
        assert!(
            mismatches.iter().any(|m| m.contains("allow-other-command")),
            "{mismatches:?}"
        );
    }
}

fn load_capabilities() -> Result<Vec<Value>, String> {
    let dir = manifest_path("capabilities");
    let entries = std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut capabilities = Vec::new();
    for entry in entries {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            let relative = format!(
                "capabilities/{}",
                path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default()
            );
            capabilities.push(read_json(&relative)?);
        }
    }
    Ok(capabilities)
}

#[test]
fn real_capabilities_should_have_no_forbidden_permissions() {
    let capabilities = load_capabilities().unwrap();
    assert!(
        !capabilities.is_empty(),
        "no hay capabilities en src-tauri/capabilities"
    );
    for capability in &capabilities {
        let violations = forbidden_permissions(capability);
        assert!(violations.is_empty(), "capability invalida: {violations:?}");
    }
}

#[test]
fn real_capabilities_should_grant_exactly_the_app_commands() {
    let capabilities = load_capabilities().unwrap();
    let mismatches = permission_set_mismatches(
        &capabilities,
        pc_optimizer_lib::commands::names::APP_COMMANDS,
    );
    assert!(
        mismatches.is_empty(),
        "permisos y APP_COMMANDS difieren: {mismatches:?}"
    );
}

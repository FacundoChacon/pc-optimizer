//! Auditoria ejecutable del grafo de dependencias efectivamente compilado para Windows x64:
//! sin crates de red/telemetria y con SQLite embebido (`bundled`).
//!
//! No se parsea `Cargo.lock` porque es independiente de plataforma y reporta crates que
//! solo existen en iOS/Android (p. ej. `reqwest` via `tauri`).

use serde_json::{Value, json};
use std::collections::BTreeSet;

/// Crates de clientes/servidores HTTP, WebSocket, QUIC, TLS, telemetria y plugins de Tauri
/// de red/shell/fs. Ampliar o reducir esta lista requiere aprobacion del dueno.
/// Excluidos a proposito (tipos/utilidades sin I/O de red usados por tauri/wry):
/// `http`, `url`, `cookie`, `mime`, `percent-encoding`, `tokio`.
const NETWORK_DENYLIST: [&str; 31] = [
    "reqwest",
    "hyper",
    "hyper-util",
    "hyper-rustls",
    "hyper-tls",
    "h2",
    "h3",
    "quinn",
    "ureq",
    "isahc",
    "curl",
    "curl-sys",
    "surf",
    "attohttpc",
    "minreq",
    "ehttp",
    "tungstenite",
    "tokio-tungstenite",
    "async-tungstenite",
    "rustls",
    "native-tls",
    "openssl",
    "openssl-sys",
    "sentry",
    "opentelemetry",
    "tauri-plugin-http",
    "tauri-plugin-updater",
    "tauri-plugin-websocket",
    "tauri-plugin-upload",
    "tauri-plugin-shell",
    "tauri-plugin-fs",
];
/// Plataforma objetivo del producto; filtra el grafo para ignorar dependencias de otras plataformas.
const TARGET_TRIPLE: &str = "x86_64-pc-windows-msvc";
/// Crate nativo de SQLite que debe compilarse con la biblioteca embebida.
const SQLITE_SYS_CRATE: &str = "libsqlite3-sys";
const SQLITE_BUNDLED_FEATURE: &str = "bundled";

/// Mapa id -> nombre de los paquetes de la salida de `cargo metadata`.
fn package_name<'a>(metadata: &'a Value, id: &str) -> Option<&'a str> {
    metadata["packages"]
        .as_array()?
        .iter()
        .find(|p| p["id"] == id)
        .and_then(|p| p["name"].as_str())
}

fn resolved_nodes(metadata: &Value) -> Vec<&Value> {
    metadata["resolve"]["nodes"]
        .as_array()
        .map(|nodes| nodes.iter().collect())
        .unwrap_or_default()
}

/// Crates del grafo resuelto que estan en la denylist (ordenados, sin duplicados).
fn forbidden_crates(metadata: &Value, denylist: &[&str]) -> Vec<String> {
    resolved_nodes(metadata)
        .into_iter()
        .filter_map(|node| package_name(metadata, node["id"].as_str()?))
        .filter(|name| denylist.contains(name))
        .map(str::to_string)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// Features activadas de `crate_name` en el grafo resuelto (ordenadas, sin duplicados).
fn enabled_features(metadata: &Value, crate_name: &str) -> Vec<String> {
    resolved_nodes(metadata)
        .into_iter()
        .filter(|node| {
            node["id"]
                .as_str()
                .and_then(|id| package_name(metadata, id))
                == Some(crate_name)
        })
        .flat_map(|node| node["features"].as_array().into_iter().flatten())
        .filter_map(|f| f.as_str().map(str::to_string))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn cargo_metadata() -> Result<Value, String> {
    let cargo = option_env!("CARGO").unwrap_or("cargo");
    let manifest = format!("{}/Cargo.toml", env!("CARGO_MANIFEST_DIR"));
    let output = std::process::Command::new(cargo)
        .args([
            "metadata",
            "--format-version",
            "1",
            "--locked",
            "--filter-platform",
            TARGET_TRIPLE,
        ])
        .args(["--manifest-path", &manifest])
        .output()
        .map_err(|e| format!("no se pudo ejecutar cargo metadata: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo metadata fallo: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    serde_json::from_slice(&output.stdout).map_err(|e| format!("salida invalida: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph(nodes: &[(&str, &[&str])]) -> Value {
        let packages: Vec<Value> = nodes
            .iter()
            .map(|(name, _)| json!({ "id": format!("{name} 1.0.0"), "name": name }))
            .collect();
        let resolved: Vec<Value> = nodes
            .iter()
            .map(|(name, features)| json!({ "id": format!("{name} 1.0.0"), "features": features }))
            .collect();
        json!({ "packages": packages, "resolve": { "nodes": resolved } })
    }

    #[test]
    fn forbidden_crates_should_be_empty_for_clean_graph() {
        let g = graph(&[("serde", &[]), ("tauri", &[]), ("rusqlite", &[])]);
        assert_eq!(
            forbidden_crates(&g, &NETWORK_DENYLIST),
            Vec::<String>::new()
        );
    }

    #[test]
    fn forbidden_crates_should_list_every_offender_sorted() {
        let g = graph(&[("tauri", &[]), ("rustls", &[]), ("reqwest", &[])]);
        assert_eq!(
            forbidden_crates(&g, &NETWORK_DENYLIST),
            vec!["reqwest", "rustls"]
        );
    }

    #[test]
    fn forbidden_crates_should_not_report_allowed_lookalikes() {
        let g = graph(&[
            ("http", &[]),
            ("url", &[]),
            ("hyperlocal-types", &[]),
            ("tokio", &[]),
        ]);
        assert_eq!(
            forbidden_crates(&g, &NETWORK_DENYLIST),
            Vec::<String>::new()
        );
    }

    #[test]
    fn forbidden_crates_should_report_each_offender_once() {
        let mut g = graph(&[("ureq", &[])]);
        g["packages"]
            .as_array_mut()
            .unwrap()
            .push(json!({ "id": "ureq 2.0.0", "name": "ureq" }));
        g["resolve"]["nodes"]
            .as_array_mut()
            .unwrap()
            .push(json!({ "id": "ureq 2.0.0", "features": [] }));
        assert_eq!(forbidden_crates(&g, &NETWORK_DENYLIST), vec!["ureq"]);
    }

    #[test]
    fn enabled_features_should_return_features_of_the_requested_crate_only() {
        let g = graph(&[
            (SQLITE_SYS_CRATE, &["bundled", "min_sqlite_version_3_14_0"]),
            ("serde", &["derive"]),
        ]);
        assert_eq!(
            enabled_features(&g, SQLITE_SYS_CRATE),
            vec!["bundled", "min_sqlite_version_3_14_0"]
        );
    }

    #[test]
    fn enabled_features_should_not_contain_bundled_when_system_sqlite_is_used() {
        let g = graph(&[(SQLITE_SYS_CRATE, &["min_sqlite_version_3_14_0"])]);
        assert!(
            !enabled_features(&g, SQLITE_SYS_CRATE).contains(&SQLITE_BUNDLED_FEATURE.to_string())
        );
    }

    #[test]
    fn enabled_features_should_be_empty_for_unknown_crate() {
        let g = graph(&[("serde", &["derive"])]);
        assert_eq!(enabled_features(&g, SQLITE_SYS_CRATE), Vec::<String>::new());
    }
}

#[test]
fn windows_dependency_graph_should_have_no_network_crates() {
    let metadata = cargo_metadata().unwrap();
    let offenders = forbidden_crates(&metadata, &NETWORK_DENYLIST);
    assert!(
        offenders.is_empty(),
        "crates de red/telemetria prohibidos en el grafo de Windows: {offenders:?}"
    );
}

#[test]
fn windows_dependency_graph_should_bundle_sqlite() {
    let metadata = cargo_metadata().unwrap();
    let features = enabled_features(&metadata, SQLITE_SYS_CRATE);
    assert!(
        features.iter().any(|f| f == SQLITE_BUNDLED_FEATURE),
        "{SQLITE_SYS_CRATE} sin feature `{SQLITE_BUNDLED_FEATURE}`: {features:?}"
    );
}

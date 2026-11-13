use tauri_build::{AppManifest, Attributes, WindowsAttributes};

include!("src/commands/names.rs");

/// Manifest generado por el linker para TODOS los binarios (app y tests).
///
/// `tauri-build` embebe su manifest solo en el binario de la app (`rustc-link-arg-bins`),
/// asi que los binarios de test cargan sin la dependencia de Common Controls v6 que
/// necesitan `muda`/`tao` y fallan con STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139).
/// Cargo no ofrece un `rustc-link-arg` solo para tests, y combinar el manifest de
/// `tauri-build` con uno propio da CVT1100 (recurso duplicado). Por eso se desactiva el
/// manifest de `tauri-build` y se deja que el linker genere uno equivalente al default
/// (Common Controls v6, `asInvoker`). La elevacion de la app la agrega `emit_app_elevation` (C-05, D-01).
fn emit_linker_manifest() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    println!("cargo::rustc-link-arg=/MANIFEST:EMBED");
    println!(
        "cargo::rustc-link-arg=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
    );
}

/// Eleva solo el binario de la app y solo en release (C-05, D-01 opcion A).
///
/// `rustc-link-arg-bins` no alcanza a los binarios de test ni al arnes de `cargo test`,
/// y en desarrollo el manifest sigue `asInvoker` para poder correr `tauri dev` sin elevar.
/// El test `tests/manifest.rs` verifica el nivel resultante contra el binario real.
fn emit_app_elevation() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows")
        || std::env::var("PROFILE").as_deref() != Ok("release")
    {
        return;
    }
    println!("cargo::rustc-link-arg-bins=/MANIFESTUAC:level='requireAdministrator'");
}

fn main() {
    let attributes = Attributes::new()
        .app_manifest(AppManifest::new().commands(APP_COMMANDS))
        .windows_attributes(WindowsAttributes::new_without_app_manifest());
    if let Err(error) = tauri_build::try_build(attributes) {
        panic!("tauri-build failed: {error:#}");
    }
    emit_linker_manifest();
    emit_app_elevation();
}

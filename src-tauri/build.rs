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
/// (Common Controls v6, `asInvoker`). El modelo de elevacion se decide en C-05.
fn emit_linker_manifest() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    println!("cargo::rustc-link-arg=/MANIFEST:EMBED");
    println!(
        "cargo::rustc-link-arg=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
    );
}

fn main() {
    let attributes = Attributes::new()
        .app_manifest(AppManifest::new().commands(APP_COMMANDS))
        .windows_attributes(WindowsAttributes::new_without_app_manifest());
    if let Err(error) = tauri_build::try_build(attributes) {
        panic!("tauri-build failed: {error:#}");
    }
    emit_linker_manifest();
}

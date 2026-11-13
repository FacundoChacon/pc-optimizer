# pc-optimizer

Aplicación de escritorio para Windows que analiza almacenamiento y rendimiento de hardware, detecta archivos inservibles o pesados, sugiere mejoras y corre como asistente continuo en la bandeja del sistema.

- **Stack:** Tauri 2 (Rust) + React + Vite + TypeScript + Tailwind
- **Diseño:** dashboard técnico oscuro (sidebar, gauges y gráficos en vivo)
- **Seguridad:** la app nunca borra de forma permanente — todo va a la Papelera con vista previa y confirmación.
- **Flujo de trabajo:** OpenSpec (OPSX) — ver `openspec/` y `CHANGES.md`.

> En construcción.

## Desarrollo

### Prerequisitos

- Windows 10/11 con WebView2 (incluido en Windows 11).
- Node.js >= 22.12 y npm.
- Rust estable (toolchain `msvc`, >= 1.95) y Visual Studio Build Tools (carga de trabajo "Desarrollo de escritorio con C++").

> En Git Bash `cargo` no está en el `PATH` por defecto: usá PowerShell o ejecutá `export PATH="$PATH:$HOME/.cargo/bin"` antes de los comandos de Rust.

### Comandos

```
npm ci
npm test
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features --locked -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --locked
npm run tauri dev
```

`npm run tauri dev` abre la ventana de la app con recarga en caliente desde una terminal **no elevada**.

### Elevación

Por ahora la app NO solicita privilegios de administrador (el manifest es `asInvoker`). El modelo de privilegios (siempre elevada, `requireAdministrator`) llega en el change C-05.

### Variables de entorno

- `PCOPT_DATA_DIR`: directorio de datos alternativo (ruta absoluta). Solo se respeta en builds de desarrollo y tests.
- `PCOPT_LOG`: nivel de log (`error`, `warn`, `info`, `debug`, `trace`; por defecto `info`).

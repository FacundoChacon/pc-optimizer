# pc-optimizer

Aplicación de escritorio para Windows que analiza almacenamiento y rendimiento de hardware, detecta archivos inservibles o pesados, sugiere mejoras y corre como asistente continuo en la bandeja del sistema.

- **Stack:** Tauri 2 (Rust) + React + Vite + TypeScript + Tailwind
- **Diseño:** dashboard técnico oscuro (sidebar, gauges y gráficos en vivo)
- **Seguridad:** la app nunca borra de forma permanente — todo va a la Papelera con vista previa y confirmación.
- **Flujo de trabajo:** OpenSpec (OPSX) — ver `openspec/` y `CHANGES.md`.

> En construcción.

## Instalación

1. Descargá `PC-Optimizer_<versión>_x64-setup.exe` y `SHA256SUMS.txt` de la [página de Releases](https://github.com/FacundoChacon/pc-optimizer/releases) de este repositorio.
2. Comprobá la descarga en PowerShell (la suma debe coincidir, sin distinguir mayúsculas):

   ```powershell
   Get-FileHash .\PC-Optimizer_<versión>_x64-setup.exe -Algorithm SHA256
   Get-Content .\SHA256SUMS.txt
   ```

   La suma detecta una descarga corrupta; no prueba por sí sola quién publicó el archivo, porque `SHA256SUMS.txt` viaja en el mismo release.
3. Ejecutá el instalador: instala para todo el equipo (Program Files) y pide permisos de administrador.

**SmartScreen.** El instalador está sin firma (no hay certificado de firma de código en v0.1), así que Windows SmartScreen puede mostrar "Windows protegió su PC". Para continuar: **Más información** y luego **Ejecutar de todas formas**.

**UAC en cada arranque.** La app siempre corre con permisos de administrador y Windows muestra el aviso de UAC cada vez que se abre.

**Sistemas.** Verificado en Windows 11. Windows 10 22H2: no verificado en v0.1. WebView2: si falta, el instalador puede descargarlo de Microsoft; la app en sí no usa la red ni envía telemetría.

### Desinstalación y datos

Desinstalá desde Configuración de Windows, Aplicaciones. La desinstalación conserva tus datos (historial y ajustes) en `%LOCALAPPDATA%\pc-optimizer`. Para borrarlos a mano después de desinstalar:

```powershell
Remove-Item -Recurse -Force "$env:LOCALAPPDATA\pc-optimizer"
```

## Integración continua

En cada pull request y en cada push a `main` corre `.github/workflows/ci.yml` (Windows): formato y clippy, auditoría de crates de red, la suite de Rust, los tests y el build del frontend, `cargo audit` y `npm audit`, y el empaquetado del instalador NSIS con su `SHA256SUMS.txt`. El release (`.github/workflows/release.yml`) lo dispara solo el dueño, desde `main`, y deja un borrador. La política de los workflows (acciones fijadas por SHA, permisos mínimos) la verifica `src-tauri/tests/ci_policy.rs`.

## Licencia

MIT, ver [LICENSE](LICENSE).

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

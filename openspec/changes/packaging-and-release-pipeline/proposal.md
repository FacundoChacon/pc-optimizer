# Proposal

## Why

C-13 cierra v0.1 (gobernanza LOW en el roadmap, pero todo lo que **publica** —tags, releases— o toca **permisos de CI** queda sujeto al dueño). Hoy el repo público no tiene `.github/`: ningún gate corre fuera de la máquina del dueño, nadie demostró que un crate de red o un test fallido pongan algo en rojo, el instalador nunca se generó ni se instaló, y la elevación obligatoria (`requireAdministrator`, C-05) solo se verificó sobre el binario de `cargo test --release`, no sobre el ejecutable que realmente se entrega. Además quedan dos deudas que deben saldarse antes de publicar: la medición de D-21 de C-07 en release (`C:` completo ≤ 5 min; en debug: 420 s, 1,53 M archivos, pico 127 MB) y el comportamiento del instalador NSIS que lanza la app al terminar (error 740 si lo hace sin poder elevar, nota de C-05).

## What Changes

- **Pipeline de CI en GitHub Actions** (`windows-2025`, permisos `contents: read`, acciones de primera parte fijadas por SHA completo, sin secretos propios): formato, clippy `-D warnings`, `cargo test` (incluido el test `no_network_crates` como paso con nombre propio), test `manifest` en perfil release, `npm test`, `npm run build`, `cargo audit` y `npm audit` con política de severidad explícita, y un job que genera el instalador y verifica el manifest del ejecutable empaquetado. Corre en cada PR, en cada push a `main` y a pedido.
- **Gate verificable**: el pipeline se demuestra en rojo ante (a) un test fallido y (b) un crate de la denylist, con PRs borrador descartables que nunca se fusionan (requiere autorización del dueño para pushear).
- **Política del pipeline como test**: un test de integración Rust sin dependencias nuevas falla si un workflow usa una acción no fijada por SHA, omite `permissions:` en el nivel superior, usa `pull_request_target` o concede escritura fuera del job de publicación.
- **Instalador NSIS** en español, instalación **por máquina** (Program Files, instalador elevado), sin firma de código (DD-11); el ejecutable instalado conserva `requireAdministrator`. MSI fuera de v0.1 (recomendación; ver `design.md`).
- **Versión única 0.1.0**: `tauri.conf.json`, `Cargo.toml` y `package.json` sincronizados y verificados por test; el tag debe coincidir con la versión.
- **Release en BORRADOR** (decidido por el dueño 2026-11-19): un workflow manual desde `main` corre todos los gates, construye el instalador, crea el tag `v0.1.0` en el commit construido y un **release borrador** con el instalador, `SHA256SUMS.txt` y notas (sin firma → aviso de SmartScreen; requiere administrador; verificado solo en Windows 11; dónde quedan los datos; sin red ni telemetría). El dueño lo publica a mano. El agente nunca dispara el workflow ni publica.
- **Verificación manual guiada de v0.1 sobre el instalador** con el dueño: instalación (SmartScreen, UAC), arranque elevado, recorrido de las funciones de v0.1, ausencia de conexiones de red del proceso, medición de D-21 en release, cancelación de UAC y desinstalación. **Windows 10 22H2 queda como no verificado** (el dueño no tiene una máquina con Windows 10, decidido 2026-11-19): SU-05 y PA-08 siguen abiertas con esa nota.
- Documentación: README (instalación, SmartScreen, desinstalación y datos), KB 02/08/09/10 y `CHANGES.md`.
- Decisiones que requieren aprobación del dueño **[DUEÑO]** en `design.md` (formato del instalador, modo de instalación, botón "Ejecutar" del final, idioma/WebView2/metadatos, datos al desinstalar, disparadores y acciones del CI, herramientas nuevas en CI, política de auditoría, demostración en rojo, workflow de release, contenido del release, protección de `main`, D-21 y smoke).

Fuera de alcance: firma de código (PA-09), auto-update (PA-11), MSI, Microsoft Store, verificación en Windows 10, atestaciones de procedencia (seguimiento propio), Dependabot/Renovate, cambios de comportamiento de la app (solo empaquetado y verificación), mejoras de rendimiento del escáner si D-21 no se cumple (se vuelve al dueño).

## Capabilities

### New Capabilities
- `ci-pipeline`: gates automáticos sobre cada PR y `main` (calidad Rust, tests, frontend, auditorías, red y manifest), política de seguridad de los workflows verificada por test y demostración del gate en rojo.
- `release-distribution`: instalador de Windows (formato, modo de instalación, idioma, comportamiento al terminar y al desinstalar), versión única de la app, workflow de release en borrador con tag y sumas de verificación, contenido de las notas y verificación manual guiada del instalador.

### Modified Capabilities
- `security-baseline`: el requisito "Ejecutable de release con elevación obligatoria" se extiende al ejecutable **empaquetado** en el instalador e **instalado** en el equipo (no solo al binario de `cargo test --release`).

## Impact

- **Nuevos archivos**: `.github/workflows/ci.yml` (reutilizable), `.github/workflows/release.yml`, `.github/release-notes/v0.1.0.md`, `src-tauri/tests/ci_policy.rs`, `src-tauri/tests/release_config.rs` (versión, bundle y notas), configuración versionada de `cargo-audit` (`src-tauri/.cargo/audit.toml`, lista de ignorados vacía o justificada); test ignorado nuevo en `src-tauri/tests/manifest.rs` (ejecutable empaquetado/instalado).
- **Configuración**: `src-tauri/tauri.conf.json` → `bundle.targets: ["nsis"]`, `bundle.windows.nsis` (por máquina, español), metadatos del bundle. Sin cambios de CSP, capabilities ni plugins (`security_baseline` sigue verde).
- **Código de la app**: ninguno. **Dependencias**: ninguna nueva en npm ni en Cargo (`package-lock.json` y `Cargo.lock` sin cambios). Herramienta solo de CI: `cargo-audit` (versión fijada).
- **GitHub**: Actions habilitado en el repo público; `GITHUB_TOKEN` con `contents: read` en CI y `contents: write` solo en el job que crea tag y borrador. Reglas de protección de `main` las configura el dueño (opcional).
- **Docs** (en el apply): `README.md`, `CHANGES.md` (C-13 y cierre de v0.1), KB 02 (distribución real), KB 08 §CI / Build, KB 09 (DD-11 con detalles y SU-05 con nota de Windows 10), KB 10 (PA-08 sigue abierta con la nota).
- **Dependencia de orden**: C-10 y C-11 ya están en `main`; C-12 se implementa en paralelo. El CI puede construirse antes, pero el smoke de v0.1 y el release requieren C-12 fusionado en `main`.

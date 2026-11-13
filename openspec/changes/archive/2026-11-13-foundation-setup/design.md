# Design

## Context

Repo greenfield: solo existen `knowledge-base/`, `CHANGES.md`, `CLAUDE.md`/`AGENTS.md`, `README.md`, `.gitignore` y `openspec/`. Motivación en `proposal.md` (Why); requisitos en `specs/{app-shell,security-baseline,runtime-config}/spec.md`.

Restricciones que moldean el diseño:

- **Máquina**: Windows 11, Node 24 / npm 11, Rust 1.100 stable (msvc), VS 2026 Build Tools, WebView2. `cargo` vive en `%USERPROFILE%\.cargo\bin` y **no está en el PATH de Git Bash** por defecto (comprobado): las tareas deben usar PowerShell o exportar `PATH="$PATH:$HOME/.cargo/bin"`.
- **Reglas duras** (`CLAUDE.md`): sin `unwrap`/`expect` fuera de tests, dominio puro, `unsafe` aislado con `// SAFETY:`, `fmt` + `clippy -D warnings`, sin plugins `fs`/`shell`, sin crates de red, tests sin disco real del usuario, TS `strict` sin `any`, IPC solo vía `src/lib/ipc.ts`, strings de UI centralizados en español.
- **Skills aplicadas**: `tauri-v2` (handler registrado, `Result<T, E: Serialize>`, permisos explícitos), `tauri-security` (deny-by-default, capabilities mínimas), `rust-best-practices` (thiserror, gate `clippy --all-targets --all-features --locked -D warnings`), `rust-testing` (tests de tabla, `tempfile`, `#[cfg(test)]` al pie), `vitest` (config en `vite.config.ts`, `mockIPC` de `@tauri-apps/api/mocks`, `vitest run` en CI), `frontend-dev` (cambios mínimos, verificar con `npm run build`; **nunca commitear**: lo hace el dueño).

## Goals / Non-Goals

**Goals:**
- Scaffold 100 % reproducible y no interactivo, con versiones exactas y lockfiles.
- Que cada garantía de seguridad de C-01 sea un **test que falla** si se rompe (no un checklist manual).
- Dejar el patrón base que copiarán los changes siguientes: comando IPC fino → `Result<T, AppError>` → `src/lib/ipc.ts` tipado → componente con strings del catálogo.
- Ciclo de desarrollo (`npm run tauri dev`, `cargo test`) usable desde una terminal **no elevada**.

**Non-Goals:**
- Manifest `requireAdministrator` / modelo de privilegios (C-05, ver D-07).
- SQLite, migraciones, puertos y fakes (C-02); routing, settings, UI kit y modos (C-04); empaquetado firmado/CI (C-13); librería de gráficos (PA-01).
- Backend de logging: C-01 solo **resuelve** el nivel (`runtime-config`); qué crate de logging lo consume se decide cuando haya algo que loguear.
- ESLint (la prohibición de `any` explícito queda cubierta por `strict` + revisión; ver Open Questions).

## Decisions

### D-01 — Scaffold manual (no `create-tauri-app`)

**Decisión**: escribir a mano cada archivo con el contenido de este diseño; generar los íconos con el CLI de Tauri ya instalado como devDependency (`npx tauri icon src-tauri/app-icon.svg`), a partir de un SVG placeholder versionado.

**Por qué**: `create-tauri-app@4.7.5` exige directorio vacío o `--force`, que sobrescribiría `README.md`/`.gitignore`/`CLAUDE.md`; además su plantilla `react-ts` usa rangos `^`, `crate-type` móvil, CSP `null`, sin Tailwind ni Vitest, y habría que reescribir casi todo. El scaffold manual es determinístico, sin prompts, y deja explícita cada línea de configuración de seguridad.

**Alternativas**: (a) `npx --yes create-tauri-app@4.7.5 tmp -t react-ts -m npm --identifier ... -y` en un directorio temporal y copiar → más pasos, mismo resultado tras reescribir; queda como plan B solo para obtener íconos si `tauri icon` con SVG fallara. (b) `npm create vite` + `tauri init` → `tauri init` es interactivo por defecto.

### D-02 — Versiones exactas (consultadas el 2026-11-12 con `npm view` / `cargo search` / `cargo info`)

npm (`package.json` sin `^`/`~`, `.npmrc` con `save-exact=true` y `engine-strict=true`, `engines.node >=22.12.0`):

| Paquete | Versión | Tipo |
|---|---|---|
| `@tauri-apps/api` | 2.12.4 | dep |
| `react` / `react-dom` | 19.3.1 | dep |
| `@tauri-apps/cli` | 2.12.4 | dev |
| `vite` | 8.3.8 | dev |
| `@vitejs/plugin-react` | 6.3.0 | dev (peer `vite ^8`) |
| `typescript` | 6.0.3 | dev (ver D-03) |
| `tailwindcss` / `@tailwindcss/vite` | 4.3.5 | dev |
| `vitest` | 5.1.0 | dev (peer `vite ^6‖^7‖^8`, node `^22.12‖^24`) |
| `jsdom` | 31.0.0 | dev (requiere node ^24.15; la máquina se actualizó a 24.22.1) |
| `@testing-library/react` | 16.3.3 | dev |
| `@testing-library/dom` | 10.4.2 | dev (peer obligatorio de RTL 16) |
| `@testing-library/jest-dom` | 7.0.1 | dev |
| `@types/react` / `@types/react-dom` | 19.3.0 | dev |
| `@types/node` | 24.19.2 | dev |

Rust (`Cargo.toml` con requisitos `=x.y.z`; `Cargo.lock` versionado; `--locked` en todos los gates):

| Crate | Versión | Notas |
|---|---|---|
| `tauri` | =2.12.5 | features por defecto; dev-dep con `features = ["test"]` |
| `tauri-build` | =2.7.4 | build-dep |
| `serde` (+`derive`) / `serde_json` | =1.0.229 / =1.0.154 | |
| `thiserror` | =2.0.21 | |
| `sysinfo` | =0.40.3 | `default-features = false`, `features = ["system", "disk", "component"]` (sin `network`, `user`, `gpu`) |
| `walkdir` | =2.5.0 | |
| `trash` | =5.3.0 | usa `windows ^0.62` → sin duplicado |
| `rusqlite` | =0.42.0 | `features = ["bundled"]` |
| `blake3` | =1.8.7 | |
| `windows` | =0.62.2 | **sin features** (C-05/C-14 agregan las que usen); alineada con la que ya trae `tauri` |
| `tempfile` | =3.27.0 | dev-dep |

`rust-version = "1.95"` (máximo MSRV del grafo: `sysinfo` 1.95; tauri 1.90). `edition = "2024"` (resolver 3: las features de dev-deps como `tauri/test` no se filtran al binario).

**Agregar ahora todas las dependencias de runtime (como pide CHANGES.md)**, no solo las que C-01 usa:
- rustc no emite warnings por dependencias no usadas (`unused_crate_dependencies` es allow-by-default) y no corremos `cargo-udeps`/`machete`; el "ruido" se limita a tiempo de compilación inicial.
- El test de ausencia de red (D-05) audita **el grafo real final** desde el día 1, no uno parcial.
- Tras GATE 1, C-03/C-04/C-05 corren en paralelo en ramas distintas: si cada uno agregara crates, `Cargo.toml`/`Cargo.lock` serían un foco seguro de conflictos de merge. Fijarlos acá lo evita.
- `windows`-sys **no** se agrega: `windows` ya cubre Win32/COM y es la que usa tauri; se reconsidera solo si un adapter lo necesita.

### D-03 — TypeScript 6.0.3 en lugar de 7.0.2

`typescript@latest` es 7.0.2 (compilador nativo). Se fija la última línea JS (6.0.3) por compatibilidad del tooling (Vite/Vitest/editores) con la API de TS; el proyecto solo usa `tsc` para type-check. Migrar a 7.x es un cambio aislado posterior. Config: `strict`, `noUncheckedIndexedAccess`, `noImplicitOverride`, `noFallthroughCasesInSwitch`, `noUnusedLocals`, `noUnusedParameters`, `exactOptionalPropertyTypes`, `moduleResolution: "bundler"`, `module: "ESNext"`, `target: "ES2022"`, `jsx: "react-jsx"`, `noEmit`. Proyecto con referencias (`tsconfig.json` → `tsconfig.app.json` + `tsconfig.node.json`) y `npm run build` = `tsc -b && vite build`.

### D-04 — CSP estricta y capabilities mínimas

`src-tauri/tauri.conf.json` (contenido objetivo; los íconos los completa `tauri icon`):

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "PC Optimizer",
  "version": "0.1.0",
  "identifier": "com.pcoptimizer.desktop",
  "build": {
    "beforeDevCommand": "npm run dev",
    "devUrl": "http://localhost:1420",
    "beforeBuildCommand": "npm run build",
    "frontendDist": "../dist"
  },
  "app": {
    "withGlobalTauri": false,
    "windows": [
      { "label": "main", "title": "PC Optimizer", "width": 1200, "height": 800,
        "minWidth": 960, "minHeight": 600, "center": true }
    ],
    "security": {
      "freezePrototype": true,
      "capabilities": ["main-window"],
      "csp": {
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
      },
      "devCsp": {
        "default-src": "'self'",
        "script-src": "'self' 'unsafe-inline'",
        "style-src": "'self' 'unsafe-inline'",
        "img-src": "'self' data:",
        "font-src": "'self'",
        "connect-src": "ipc: http://ipc.localhost ws://localhost:1420 http://localhost:1420",
        "object-src": "'none'",
        "base-uri": "'self'",
        "form-action": "'none'",
        "frame-ancestors": "'none'"
      }
    }
  },
  "bundle": { "active": true, "targets": ["nsis", "msi"], "icon": ["icons/32x32.png", "icons/128x128.png", "icons/128x128@2x.png", "icons/icon.ico"] }
}
```

`src-tauri/capabilities/main-window.json`:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "main-window",
  "description": "Ventana principal: solo comandos propios habilitados explícitamente. Sin plugins shell/fs/http.",
  "windows": ["main"],
  "platforms": ["windows"],
  "permissions": ["allow-get-app-info"]
}
```

Notas de diseño:
- **`connect-src` "vacío" de CHANGES.md se interpreta como "sin destinos de red"**: un `'none'` literal bloquearía el transporte IPC de Tauri 2 en Windows (`fetch` a `http://ipc.localhost`, documentado por Tauri), forzando el fallback a `postMessage` con violaciones de CSP en consola. `ipc:`/`http://ipc.localhost` son el canal local al proceso Rust, no red.
- Tauri agrega automáticamente hashes/nonces de sus scripts/estilos a la CSP en build. Por eso producción **no** necesita `unsafe-inline`: Vite emite CSS/JS como archivos y React aplica `style={}` vía CSSOM (no bloqueado por CSP).
- `vite.config.ts` usa `build.assetsInlineLimit: 0` para que Vite nunca genere URIs `data:` y `img-src 'self'` alcance.
- `devCsp` solo relaja lo que el dev server de Vite necesita (preamble inline de React Refresh, `<style>` inyectados por HMR, websocket en `localhost:1420`). Nunca `unsafe-eval` ni orígenes no-localhost (validado por test).
- **Sin `core:default`**: C-01 no usa APIs JS de Tauri (eventos, ventana, path). Cada permiso core se agregará en el change que lo use (p. ej. `core:event:default` en C-06), con justificación.
- **Comandos propios bajo ACL**: `build.rs` declara los comandos con `tauri_build::AppManifest::new().commands(APP_COMMANDS)`, lo que genera los permisos `allow-<comando>` y hace que un comando no otorgado en una capability sea rechazado. La lista vive en `src-tauri/src/commands/names.rs` (`pub const APP_COMMANDS: &[&str] = &["get_app_info"];`), incluida con `include!` desde `build.rs` y expuesta como módulo en la lib, de modo que el test de baseline compara ese conjunto con los permisos de la capability (fuente única).
- `withGlobalTauri: false` y `freezePrototype: true` como endurecimiento adicional (riesgo y reversión en Risks).

### D-05 — Test ejecutable de "sin crates de red"

**Hallazgo clave**: `tauri 2.12.5` declara `reqwest ^0.13` como dependencia **solo para iOS/Android** (`cfg(any(target_os = "android", all(target_vendor = "apple", not(target_os = "macos"))))`). `Cargo.lock` es independiente de plataforma y la incluye, así que **parsear `Cargo.lock` daría un falso positivo** permanente. Por eso el test usa el grafo resuelto filtrado por plataforma.

`src-tauri/tests/no_network_crates.rs`:
1. Ejecuta `$CARGO metadata --format-version 1 --locked --filter-platform x86_64-pc-windows-msvc --manifest-path $CARGO_MANIFEST_DIR/Cargo.toml` (`CARGO` vía `option_env!("CARGO")`, fallback `"cargo"`), parsea con `serde_json`.
2. Función pura `forbidden_crates(metadata: &serde_json::Value, denylist: &[&str]) -> Vec<String>`: recorre `resolve.nodes[]`, mapea `id → name` con `packages[]`, devuelve los nombres que están en la denylist (ordenados, sin duplicados). Incluye dependencias normales, de build y de dev del paquete (más estricto).
3. Función pura `enabled_features(metadata, crate_name) -> Vec<String>` para el requisito de SQLite: `libsqlite3-sys` debe tener `bundled`.
4. Tests unitarios de las funciones puras con fixtures JSON mínimos inline (grafo limpio → vacío; grafo con `reqwest` y `rustls` → ambos reportados; nombre parecido no listado como `http`/`hyperlocal-types` → no reportado; `libsqlite3-sys` con/sin `bundled`) y un test de integración que corre el comando real y exige lista vacía.

**Denylist** (constante en el test, comentada): `reqwest`, `hyper`, `hyper-util`, `hyper-rustls`, `hyper-tls`, `h2`, `h3`, `quinn`, `ureq`, `isahc`, `curl`, `curl-sys`, `surf`, `attohttpc`, `minreq`, `ehttp`, `tungstenite`, `tokio-tungstenite`, `async-tungstenite`, `rustls`, `native-tls`, `openssl`, `openssl-sys`, `sentry`, `opentelemetry`, `tauri-plugin-http`, `tauri-plugin-updater`, `tauri-plugin-websocket`, `tauri-plugin-upload`, `tauri-plugin-shell`, `tauri-plugin-fs`. **Excluidos a propósito** (tipos/utilidades sin I/O de red, usados por tauri/wry): `http`, `url`, `cookie`, `mime`, `percent-encoding`, `tokio`.

Nota operativa: la primera ejecución de `cargo metadata` puede descargar manifests de crates de otras plataformas al cache de cargo; es tooling de desarrollo, no la app. Ampliar o reducir la denylist requiere aprobación (regla dura).

### D-06 — Tests de baseline de seguridad y arquitectura

- `src-tauri/tests/security_baseline.rs`: lee `tauri.conf.json` y `capabilities/*.json` del repo (no son datos de usuario; la regla "sin disco real" aplica a datos del usuario). Funciones puras con tests de tabla:
  - `csp_violations(directives) -> Vec<String>` (acepta CSP en forma de objeto o string): exige `default-src 'self'`, `object-src 'none'`; prohíbe `unsafe-eval`, `unsafe-inline`, `*`, `data:` en `script-src`, y cualquier `http:`/`https:`/`ws:`/`wss:` salvo `http://ipc.localhost`; `connect-src` ⊆ {`ipc:`, `http://ipc.localhost`}.
  - `dev_csp_violations(directives)`: prohíbe `unsafe-eval` y orígenes que no sean `ipc:`, `http://ipc.localhost`, `http://localhost:*`, `ws://localhost:*`.
  - `forbidden_permissions(capability) -> Vec<String>`: prefijos `shell:`, `fs:`, `http:`, `process:`, `updater:`, `opener:`; `windows == ["main"]`.
  - Test de conjunto exacto: permisos `allow-*` de las capabilities == `APP_COMMANDS` en kebab-case; `withGlobalTauri == false`.
- `src-tauri/tests/architecture.rs`: recorre `src-tauri/src/domain/**/*.rs`; función pura `forbidden_imports(source) -> Vec<String>` detecta `std::fs`, `std::process`, `std::net`, `windows::`/`use windows`, `rusqlite`, `tauri::`/`use tauri`. Tests de tabla con fuentes inline (limpio, cada import prohibido, mención en comentario `//` ignorada).
- Gate de lints en `Cargo.toml` (convierte reglas duras en errores de compilación):

```toml
[lints.rust]
unsafe_op_in_unsafe_fn = "deny"

[lints.clippy]
unwrap_used = "deny"
expect_used = "deny"
undocumented_unsafe_blocks = "deny"
```

  `src-tauri/clippy.toml`: `allow-unwrap-in-tests = true`, `allow-expect-in-tests = true`. Los helpers fuera de `#[test]` en `tests/` devuelven `Result` y usan `?`.

### D-07 — Elevación (`requireAdministrator`) diferida a C-05

**Verificado en código fuente**: `tauri-build 2.7.4` embebe el manifest de la app vía `tauri-winres 0.3.7` → `embed_resource::compile` (3.0.12), que emite `cargo:rustc-link-arg-bins=…`. Es decir, el manifest se enlaza **solo en binarios**, no en binarios de test.

Consecuencias:
- Si C-01 embebiera `requireAdministrator`, `npm run tauri dev` desde una terminal no elevada fallaría al lanzar el `.exe` con **error 740** (`ERROR_ELEVATION_REQUIRED`), rompiendo el ciclo de desarrollo y el smoke manual. `cargo test` **no** se vería afectado (los test binaries no llevan el manifest), contra lo que se temía.
- **Decisión**: C-01 no define manifest propio (se usa el default de tauri-build: Common Controls v6, `asInvoker` implícito). El archivo `pc-optimizer.exe.manifest` y la estrategia de elevación (p. ej. embeber `requireAdministrator` solo en builds release vía `WindowsAttributes::app_manifest` condicionado al perfil, o exigir terminal elevada en dev) los decide C-05, gobernanza HIGH. El spec `security-baseline` fija explícitamente "sin elevación forzada" hasta entonces.
- Al manifest de C-05 hay que **incluir la dependencia de Common Controls v6** (la doc de `app_manifest` lo exige si se usan diálogos), porque reemplaza al default.

### D-08 — Primer comando IPC y contrato de error

- `src-tauri/src/error.rs`: `AppError` (`thiserror`) con variante inicial `Internal(String)`; `Serialize` manual a `{ "kind": "internal", "message": "<Display>" }`. Cada change agrega variantes con `kind` estable en `snake_case`.
- `src-tauri/src/commands/app_info.rs`: función pura `app_info() -> AppInfo { name, version }` (desde `env!("CARGO_PKG_NAME"/"CARGO_PKG_VERSION")`) y comando fino `#[tauri::command] fn get_app_info() -> Result<AppInfo, AppError>`.
- `src-tauri/src/lib.rs`: `pub fn app_builder<R: tauri::Runtime>(b: tauri::Builder<R>) -> tauri::Builder<R>` registra `generate_handler![commands::get_app_info]` y `manage(RuntimeConfig)`; `pub fn run() -> Result<(), AppError>` usa `tauri::Builder::default()` + `generate_context!()`; `main.rs` (`#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]`) llama `run()` y ante error escribe en stderr y sale con código 1 (sin `expect`). `[lib] name = "pc_optimizer_lib"`, `crate-type = ["rlib"]` (solo escritorio; `cdylib`/`staticlib` son para móvil).
- Smoke IPC en `#[cfg(test)]` de `lib.rs` con `tauri::test::{mock_builder, mock_context, noop_assets, get_ipc_response}`: construye la app con `app_builder(mock_builder())`, crea la ventana `main` y verifica que `get_app_info` responde `{ name, version }`.
- Frontend: `src/lib/ipc.ts` es el **único** importador de `@tauri-apps/api/core`; exporta tipos espejo (`AppInfo`, `AppError { kind: string; message: string }`), `toAppError(e: unknown): AppError` (normaliza formas desconocidas a `kind: "unknown"`) y `getAppInfo(): Promise<AppInfo>` que rechaza siempre con `AppError`.
- Catálogo: `src/shared/i18n/es.ts` (`export const es = { app: { title, versionLabel, loading, ipcError } } as const`). `src/app/App.tsx` muestra título y versión, o el error del catálogo.

### D-09 — Configuración de runtime

`src-tauri/src/config.rs` (puro, sin leer el entorno): `resolve_runtime_config(lookup: impl Fn(&str) -> Option<String>, allow_dev_overrides: bool) -> RuntimeConfig { data_dir_override: Option<PathBuf>, log_level: LogLevel }`. `lib.rs` pasa `|k| std::env::var(k).ok()` y `cfg!(debug_assertions)`. Reglas en `specs/runtime-config/spec.md` (trim, vacío = ausente, ruta no absoluta = ignorada, `PCOPT_LOG` case-insensitive con default `info`).

**Desvío consciente de CHANGES.md**: `PCOPT_LOG` se respeta también en release (solo cambia verbosidad y es útil para soporte); el "solo dev/tests" se aplica a `PCOPT_DATA_DIR`, que podría redirigir dónde una app elevada escribe datos.

### D-10 — Estructura y frontend

```
src-tauri/
  Cargo.toml  Cargo.lock  build.rs  clippy.toml  tauri.conf.json  app-icon.svg
  capabilities/main-window.json   icons/ (generados)
  src/ main.rs  lib.rs  error.rs  config.rs
       commands/{mod.rs, names.rs, app_info.rs}
       domain/mod.rs        (solo doc: "puro, sin I/O")
       adapters/mod.rs      (solo doc)
  tests/ no_network_crates.rs  security_baseline.rs  architecture.rs
src/
  main.tsx  styles.css  vite-env.d.ts
  app/App.tsx   features/.gitkeep   shared/i18n/es.ts   lib/ipc.ts
  tests/ setup.ts  ipc.test.ts  App.test.tsx
index.html  vite.config.ts  tsconfig.json  tsconfig.app.json  tsconfig.node.json  package.json  .npmrc
```

- `src/lib/ipc.ts` sigue la regla dura de `CLAUDE.md`, que prevalece sobre la KB (`08` y C-04 hablan de `src/shared/ipc`); ver Open Questions.
- `vite.config.ts`: `plugins: [react(), tailwindcss()]`, `clearScreen: false`, `server: { port: 1420, strictPort: true, watch: { ignored: ["**/src-tauri/**"] } }`, `envPrefix: ["VITE_", "TAURI_ENV_"]`, `build: { target: "chrome120", assetsInlineLimit: 0 }`, `test: { environment: "jsdom", setupFiles: ["src/tests/setup.ts"], include: ["src/tests/**/*.test.{ts,tsx}"], restoreMocks: true }`.
- `styles.css`: `@import "tailwindcss";` + `@theme` con tokens oscuros (`--color-surface`, `--color-surface-raised`, `--color-content`, `--color-content-muted`, `--color-accent`) y `:root { color-scheme: dark; }`. `index.html` con `lang="es"`. Sin fuentes remotas.
- Scripts npm: `dev` (`vite`), `build` (`tsc -b && vite build`), `preview`, `test` (`vitest run`), `test:watch` (`vitest`), `typecheck` (`tsc -b`), `tauri` (`tauri`).

### D-11 — Comandos de verificación (gates del change)

Desde la raíz (PowerShell, o Git Bash con `~/.cargo/bin` en PATH):

```
npm ci
npm test
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features --locked -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --locked
```

Más un smoke manual: `npm run tauri dev` desde terminal **no elevada** → ventana "PC Optimizer", versión visible, consola de DevTools sin violaciones de CSP ni errores de IPC.

## Risks / Trade-offs

- **[Test binaries sin manifest Common Controls v6]** Como el manifest solo va a binarios (D-07), un test binary que enlaza tauri/muda podría fallar al cargar con `STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139)`. → Detectarlo en la primera corrida de `cargo test` (tarea 5.1). Mitigaciones en orden: (1) en `build.rs`, solo Windows, emitir `cargo:rustc-link-arg-tests=/MANIFEST:EMBED` y `/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'`, verificando que cubra también el test binary de la lib; (2) si no lo cubre, mover el smoke de IPC a `tests/` y documentarlo. Registrar la solución adoptada en el resumen del apply.
- **[`devCsp` rompe HMR/React Refresh]** Si Tauri inyecta hashes/nonces en dev, `'unsafe-inline'` queda ignorado y el preamble de React Refresh se bloquea. → Ajustar **solo** `devCsp` (nunca `csp`), manteniendo el test de dev CSP verde; si no hay salida, desactivar Fast Refresh en dev y documentarlo.
- **[Sin `core:default`]** Algún comportamiento interno de la ventana podría requerir un permiso core. → El smoke manual lo detecta; agregar solo el permiso puntual (no el set default) y justificarlo en el resumen.
- **[`freezePrototype: true`]** Una librería futura que extienda prototipos fallaría en runtime. → Hoy React 19 no lo hace; si un change lo necesita, se discute ahí.
- **[Versiones muy nuevas (Vite 8 / Vitest 5 / React 19.3 / TS 6)]** Posibles incompatibilidades de peers. → Peers verificados con `npm view`; `npm install` debe terminar sin `ERESOLVE`. Si falla, bajar al par compatible más cercano y registrar el cambio en este design.
- **[`cargo metadata` y caché]** La primera corrida del test de red puede necesitar descargar manifests. → Aceptado (tooling); el test no usa `--offline`.
- **[Tiempo de compilación inicial]** Agregar `rusqlite bundled`, `blake3`, `windows`, `sysinfo` ahora alarga el primer build (C compilado con MSVC). → Se paga una vez; evita conflictos en las ramas paralelas de GATE 1.
- **[Desvío KB ↔ CLAUDE.md en la ruta del cliente IPC]** → Se sigue la regla dura; C-04 debe alinear KB/CHANGES o la regla.

## Migration Plan

No aplica (greenfield). Rollback = descartar la rama `feat/foundation-setup`.

## Open Questions

- **Ruta del cliente IPC**: ✅ RESUELTO por el dueño — `src/lib/ipc.ts` (KB 08 y C-04 actualizados).
- **Lint de `any` explícito**: `strict` no prohíbe `any` escrito a mano. ¿Agregar ESLint + `@typescript-eslint/no-explicit-any` (en C-04 o C-13)? No cambia C-01.
- **Identificador de bundle** `com.pcoptimizer.desktop` es una suposición (sin dominio propio); cambiarlo antes del primer release (C-13) no afecta specs.

## Apply notes (implementación)

- **STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139) confirmado** en el primer `cargo test` con el smoke IPC (`tauri::test`). `cargo::rustc-link-arg-tests` NO es aceptado por cargo 1.100 ("invalid instruction") y un `rustc-link-arg` global junto al manifest de `tauri-build` produce CVT1100 (recurso duplicado) en el binario de la app. Solución adoptada en `build.rs`: `WindowsAttributes::new_without_app_manifest()` + `rustc-link-arg=/MANIFEST:EMBED` y `/MANIFESTDEPENDENCY` (Common Controls v6) para todos los binarios. El manifest resultante es equivalente al default (`asInvoker`, sin elevación); C-05 debe reemplazar esta estrategia al introducir `requireAdministrator`.
- **`jsdom`**: se instaló 29.1.1 por node 24.11.1; tras actualizar node a 24.22.1 se subió a 31.0.0 (ver D-02).
- Cargo 1.100 emite el lint `cargo::unused_dependencies` (warn) para las dependencias aún sin uso; se silencia con `[lints.cargo] unused_dependencies = "allow"` en `Cargo.toml` hasta que C-02+ las consuman (quitar entonces).

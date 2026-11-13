# Proposal

## Why

El repositorio solo contiene la base de conocimiento y el roadmap: no existe todavía ninguna aplicación que compilar, testear ni endurecer. `foundation-setup` (C-01, raíz del camino crítico) crea el esqueleto Tauri 2 + React sobre el que se apoyan los 26 changes restantes, y fija **desde el primer commit** las garantías que el proyecto promete (sin red, superficie IPC mínima, CSP estricta, dominio puro) como tests ejecutables, para que ningún change posterior pueda erosionarlas en silencio.

## What Changes

- Scaffold reproducible (sin prompts interactivos) de Tauri 2 + React + Vite + TypeScript (`strict`) + Tailwind, con tema oscuro base y la estructura `src-tauri/src/{domain,adapters,commands}` y `src/{app,features,shared}` + `src/lib/ipc.ts`.
- Versiones **exactas** de todas las dependencias npm y crates (incluidas las que usarán changes posteriores: `sysinfo`, `walkdir`, `trash`, `rusqlite` con `bundled`, `blake3`, `windows`), con `package-lock.json` y `Cargo.lock` versionados.
- `tauri.conf.json` con CSP estricta (sin `unsafe-eval`/`unsafe-inline`, sin orígenes remotos; `connect-src` limitado al canal IPC local de Tauri) y `capabilities/` mínimas: sin plugins `shell`/`fs`/`http`, solo el comando propio habilitado de forma explícita.
- Primer comando IPC `get_app_info` (nombre + versión) y contrato de error serializable `AppError`, consumidos desde `src/lib/ipc.ts`; catálogo de strings en español.
- Configuración de runtime por variables de entorno `PCOPT_DATA_DIR` (solo builds de desarrollo/tests) y `PCOPT_LOG`.
- Tooling de calidad: `cargo test`, `cargo clippy -D warnings` (con lints que prohíben `unwrap`/`expect` fuera de tests y `unsafe` sin `// SAFETY:`), `cargo fmt --check`, Vitest + Testing Library, scripts `npm test` / `npm run build`.
- Tests de baseline de seguridad **ejecutables**: auditoría del grafo de dependencias contra una denylist de crates de red, validación de CSP/capabilities, `rusqlite` con SQLite embebido, y pureza de `domain/` (sin imports de I/O).
- **Fuera de alcance (diferido)**: el manifest `requireAdministrator` se difiere a C-05 `privilege-context-and-profile-check` (ver design.md, D-07). También quedan fuera: SQLite/migraciones y puertos (C-02), shell de UI/routing/settings (C-04), empaquetado/CI (C-13), librería de gráficos (PA-01).

## Capabilities

### New Capabilities
- `app-shell`: arranque de la aplicación de escritorio, ventana principal con tema oscuro y textos desde el catálogo en español, consulta de información de la app por IPC y contrato de error serializable entre backend y UI.
- `security-baseline`: garantías de seguridad verificables de forma automática: CSP estricta, capabilities mínimas sin plugins peligrosos, ausencia de dependencias con red/telemetría y pureza de la capa de dominio.
- `runtime-config`: resolución de configuración de ejecución desde variables de entorno (override del directorio de datos solo en desarrollo/tests, nivel de log) con valores por defecto seguros.

### Modified Capabilities
<!-- Ninguna: no existen specs previas en openspec/specs/. -->

## Impact

- **Código nuevo**: `package.json`, `package-lock.json`, `.npmrc`, `vite.config.ts`, `tsconfig*.json`, `index.html`, `src/**`, `src-tauri/**` (Cargo.toml, Cargo.lock, build.rs, clippy.toml, tauri.conf.json, capabilities/, icons/, src/, tests/).
- **Archivos existentes**: `.gitignore` (agrega `coverage/` y artefactos de icon generator si aplica); `README.md` (sección de comandos de desarrollo).
- **Dependencias**: Tauri 2.12.x, React 19, Vite 8, Vitest 5, Tailwind 4, TypeScript 6; crates sysinfo/walkdir/trash/rusqlite/blake3/windows/thiserror/serde. Ninguna con red.
- **Gobernanza**: LOW (scaffold y tooling); la configuración de seguridad (CSP/capabilities) queda fijada por tests y cualquier ampliación futura requiere aprobación explícita (regla dura del proyecto).
- **Desbloquea**: C-02 `domain-ports-and-sqlite-store` (GATE 0).

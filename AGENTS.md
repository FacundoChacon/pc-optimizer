# pc-optimizer — Instrucciones para Agentes

> Este archivo (y su copia `CLAUDE.md`) es lo PRIMERO que todo agente lee al entrar al repo.
> Generado a partir de `knowledge-base/` y `CHANGES.md`. No editar a mano sin re-sincronizar ambos archivos.

**pc-optimizer**: app de escritorio para Windows 10/11 (Tauri 2 + Rust + React) que analiza almacenamiento y rendimiento, detecta y elimina basura de forma segura (siempre a la Papelera, con preview y log), aplica mejoras reversibles y puede correr como asistente en la bandeja. Local, sin backend, sin cuentas, sin telemetría. Prioridad: seguridad > correctitud > mantenibilidad > velocidad.

---

## Stack Tecnológico

| Capa | Tecnologías | Versión mínima |
|------|-------------|----------------|
| Shell de escritorio | Tauri 2 | 2.x |
| Backend (Rust) | `sysinfo`, `walkdir`, `trash`, `rusqlite`, `blake3`, `windows` / `windows-sys` (Win32, WMI) | Rust estable reciente (**Suposición:** MSRV = última estable al iniciar) |
| Frontend | React + Vite + TypeScript + Tailwind | React 18+, Vite 5+, TS 5+ (**Suposición**) |
| Persistencia | SQLite local (vía `rusqlite`) | SQLite 3.x bundled |
| Gráficos | **TBD** (pregunta abierta PA-01, ver 10) | — |
| Tests | Vitest (frontend), `cargo test` (Rust). Strict TDD | — |
| Distribución | Instalador NSIS/MSI (Tauri bundler) vía GitHub Releases | — |
| SO objetivo | Windows 10 y 11 (profundidad Win10 abierta, ver 10) | Win10 22H2 (**Suposición**) |

Arquitectura hexagonal: `src-tauri/src/{commands, domain, adapters}` (dominio puro, adaptadores de SO detrás de traits, comandos IPC finos) + frontend por features.

**Privilegios**: la app corre SIEMPRE elevada (manifest `requireAdministrator`), sin drivers de kernel. Ver [13_monitoreo_hardware_y_privilegios.md](knowledge-base/13_monitoreo_hardware_y_privilegios.md).

**Prerequisitos en esta máquina**: Rust stable (msvc) + Visual Studio 2026 Build Tools + WebView2 ya instalados.

Detalle completo: [knowledge-base/02_descripcion_general.md](knowledge-base/02_descripcion_general.md)

---

## Base de Conocimiento

La fuente de verdad del dominio vive en `knowledge-base/` (índice: [README.md](knowledge-base/README.md)). **Leé el archivo relevante ANTES de implementar.**

| Archivo | Cuándo leerlo |
|---------|---------------|
| [01_vision_y_objetivos.md](knowledge-base/01_vision_y_objetivos.md) | Entender propósito, alcance por versión y fuera de alcance |
| [02_descripcion_general.md](knowledge-base/02_descripcion_general.md) | Stack, arquitectura, integraciones Windows, contrato IPC |
| [03_actores_y_roles.md](knowledge-base/03_actores_y_roles.md) | Modos de UI, contexto de privilegios, matriz de capacidades |
| [04_modelo_de_datos.md](knowledge-base/04_modelo_de_datos.md) | Entidades SQLite, ERD, migraciones, retención |
| [05_reglas_de_negocio.md](knowledge-base/05_reglas_de_negocio.md) | Reglas codificadas (RN-XX) |
| [06_funcionalidades.md](knowledge-base/06_funcionalidades.md) | Historias de usuario por épica |
| [07_flujos_principales.md](knowledge-base/07_flujos_principales.md) | Flujos E2E |
| [08_arquitectura_propuesta.md](knowledge-base/08_arquitectura_propuesta.md) | Patrones, gobernanza, estructura de directorios, seguridad Tauri, testing |
| [09_decisiones_y_supuestos.md](knowledge-base/09_decisiones_y_supuestos.md) | Decisiones DD-xx y supuestos SU-xx |
| [10_preguntas_abiertas.md](knowledge-base/10_preguntas_abiertas.md) | Inconsistencias a resolver ANTES de codear |
| [11_seguridad_limpieza_y_rutas_protegidas.md](knowledge-base/11_seguridad_limpieza_y_rutas_protegidas.md) | Whitelist de limpieza, rutas protegidas, over-the-shoulder, UIPI (LO MÁS CRÍTICO) |
| [12_ui_modos_simple_avanzado.md](knowledge-base/12_ui_modos_simple_avanzado.md) | Modos simple/avanzado, pantallas, accesibilidad, i18n |
| [13_monitoreo_hardware_y_privilegios.md](knowledge-base/13_monitoreo_hardware_y_privilegios.md) | Always-admin, autostart por Task Scheduler, sensores, muestreo |

**Preguntas abiertas de prioridad Alta aún sin resolver** (decidir antes del change que las toque):
- **PA-01**: librería de gráficos (Recharts, uPlot, visx, ECharts, canvas propio) para gráficos en vivo cada 1-2 s con bajo overhead — afecta v0.1 (monitor en vivo, C-06).
- **PA-03**: cómo detectar de forma fiable apps "sin uso" (Prefetch/UserAssist/last access son señales débiles) — afecta v0.2 (C-19).
- Ya resueltas: PA-02 (vaciar Papelera: sí, con doble confirmación, nunca automática) y PA-14 (over-the-shoulder: bloqueo solo de categorías por-usuario, con aviso).

---

## Skills Disponibles

Fuente de verdad: `.atl/skill-registry.md`.

| Agente | Rol | Skills que carga |
|--------|-----|------------------|
| **Backend Rust/Tauri** | Comandos IPC, dominio, adapters, SQLite, APIs Windows | `tauri-v2`, `tauri-security`, `rust-best-practices`, `rust-testing`, `sqlite-database-expert`, `winmd-api-search` |
| **Frontend React** | UI, tests, visualización, revisión UX | `frontend-dev`, `vitest`, `dataviz`, `ux-ui-revisor` |
| **Seguridad / Debug** | Auditoría de seguridad, causa raíz de bugs | `security-debugger`, `tauri-security` |
| **CI/CD** | Pipelines, build Tauri, releases | `cicd-dev` |
| **Orquestación OPSX** | Explorar, proponer, aplicar, archivar changes | `openspec-explore`, `openspec-propose`, `openspec-apply-change`, `openspec-archive-change`, `openspec-sync-specs`, `openspec-update-change` |

Cargá la skill correspondiente al contexto ANTES de escribir código.

> Los compact rules de cada skill los resuelve el orquestador desde `.atl/skill-registry.md` (generado por `skill-registry`; no versionado — no está en el repo).

---

## Roadmap de Changes

El plan de implementación completo está en [CHANGES.md](CHANGES.md). Resumen:

- **Total**: 27 changes en 3 hitos (5 fases):
  - **v0.1 Núcleo de limpieza**: C-01 a C-13 (13 changes).
  - **v0.2 Sistema y mejoras**: C-14 a C-20 (7 changes).
  - **v0.3 Asistente continuo**: C-21 a C-27 (7 changes).
- **Camino crítico** (8): `C-01 → C-02 → C-03 → C-08 → C-09 → C-18 → C-26 → C-27`.
- **Primer change**: `C-01` (`foundation-setup`).

**Antes de cualquier `/opsx:propose`**: leé [CHANGES.md](CHANGES.md), identificá las dependencias del change y los archivos de "Leer antes".

---

## Reglas Duras

> Reglas globales ya definidas en `~/.claude/CLAUDE.md` (orquestador, governance, TDD, engram): el proyecto las hereda. Acá viven solo las reglas **específicas de este proyecto** + las universales que el global no cubra.

Son contrato; romperlas es un defecto.

**Rust (src-tauri)**
- NUNCA `unwrap()`/`expect()` fuera de tests → `Result` con errores tipados (`thiserror`); los comandos Tauri devuelven `Result<T, AppError>` serializable.
- NUNCA importar `std::fs`, `windows`, `rusqlite` ni `tauri` en `src-tauri/src/domain` → el acceso al SO va por traits (ports) en `adapters/`, con fakes en tests.
- `unsafe` solo en adapters de Windows, en bloques mínimos, cada uno con comentario `// SAFETY:` que justifique la invariante.
- `cargo fmt --check` y `cargo clippy -- -D warnings` deben pasar antes de cerrar un change.

**Seguridad de limpieza (gobernanza HIGH)**
- NUNCA `std::fs::remove_*` sobre archivos de usuario → todo borrado pasa por el adapter de limpieza, que valida contra `protected_paths` y mueve a la Papelera (`trash`). Única excepción declarada: vaciar la Papelera (RN-LIM-06), con doble confirmación y nunca automática.
- NUNCA agregar categorías de basura ni mejoras de sistema fuera de la whitelist → requiere `/opsx:propose` aprobado por el dueño.
- NUNCA exponer plugins `fs`/`shell` al webview, ampliar capabilities/CSP, ni agregar dependencias con red o telemetría → sin aprobación explícita.
- NUNCA tests contra el disco real del usuario → `tempdir` fixtures; tests que dependen de Windows real van con `#[ignore]` y se corren a mano.

**Frontend (React + TS)**
- NUNCA `any` → `tsconfig` en `strict`; usar `unknown` + narrowing. Componentes en `PascalCase`.
- NUNCA `invoke()` suelto en componentes → todo IPC por `src/lib/ipc.ts`, tipado espejo de los comandos Rust.
- NUNCA strings de UI hardcodeados en componentes → catálogo centralizado en español (i18n-ready).

**Git**
- Cada change OPSX en su branch `feat/<change-name>`; commits convencionales (`feat:`, `fix:`, `chore:`, `test:`…) cuando el change queda verificado (tests en verde). Push y PR SOLO cuando el dueño lo pide.
- NUNCA versionar `.engram/` ni `.atl/` (repo público) → NUNCA correr `engram sync` ni `git add .engram` en este repo.

---

## Flujo de Trabajo

```
1. Leer la KB relevante (knowledge-base/)        → entender el dominio
2. Identificar el change en CHANGES.md           → respetar dependencias
3. /opsx:propose <change>                        → proposal + design + specs + tasks
4. /opsx:apply                                   → implementar las tasks (cargando skills, Strict TDD)
5. /opsx:archive <change> + marcar [x]           → cerrar el change
```

Primer change: `foundation-setup` (`/opsx:propose foundation-setup`).

Aplicar TODAS las reglas duras en cada paso. Ante conflicto entre la KB y este archivo, las reglas duras prevalecen.

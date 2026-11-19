# Descripción General

## Stack tecnológico

| Capa | Tecnologías | Versión mínima |
|------|-------------|----------------|
| Shell de escritorio | Tauri 2 | 2.x |
| Backend (Rust) | `sysinfo`, `walkdir`, `trash`, `rusqlite`, `blake3`, `windows` / `windows-sys` (Win32, WMI) | Rust estable reciente (**Suposición:** MSRV = última estable al iniciar) |
| Frontend | React + Vite + TypeScript + Tailwind | React 18+, Vite 5+, TS 5+ (**Suposición**) |
| Persistencia | SQLite local (vía `rusqlite`) | SQLite 3.x bundled |
| Gráficos | **TBD** (pregunta abierta, ver 10) | — |
| Tests | Vitest (frontend), `cargo test` (Rust). Strict TDD | — |
| Distribución | Instalador NSIS (Tauri bundler; MSI reevaluado más adelante) vía GitHub Releases | — |
| SO objetivo | Windows 10 y 11 (profundidad Win10 abierta, ver 10) | Win10 22H2 (**Suposición**) |

## Arquitectura general

App local de proceso único (Tauri) con webview. Estilo hexagonal: dominio puro en Rust, adaptadores de SO detrás de traits, comandos IPC finos, frontend por features.

```
┌──────────────────────── Webview (React/TS) ────────────────────────┐
│  features/: dashboard · disk · cleanup · duplicates · startup ·    │
│  improvements · monitor · alerts · reports · settings              │
└───────────────▲──────────────────────────────┬─────────────────────┘
                │ eventos (métricas en vivo)   │ invoke (allowlist IPC)
┌───────────────┴──────────────────────────────▼─────────────────────┐
│ src-tauri/src/commands   (capa IPC fina, validación de input)      │
├────────────────────────────────────────────────────────────────────┤
│ src-tauri/src/domain     (PURO: reglas, scoring, protección rutas, │
│                           clasificación basura, undo)              │
├────────────────────────────────────────────────────────────────────┤
│ src-tauri/src/adapters   windows · fs · sqlite · trash · scheduler │
│                          (traits en domain, fakes en tests)        │
└──────────────┬─────────────────────────────────────────────────────┘
               ▼
   Windows (UAC, Task Scheduler, Papelera, registro, WMI) · SQLite local
```

Justificación: separar lógica pura de efectos permite Strict TDD sobre reglas críticas de seguridad sin tocar el disco real. Detalle en 08.

## Principios transversales

- Local-first: sin red, sin cuentas, sin telemetría.
- Siempre ejecuta como administrador (manifest `requireAdministrator`); ver 13.
- Nunca borrado permanente: Papelera (ver 11).
- Dos modos de UI sobre el mismo backend (ver 12).

## Integraciones externas

| Servicio | Propósito | Tipo |
|----------|-----------|------|
| Windows UAC | Elevación al iniciar | Manifest |
| Windows Task Scheduler | Autostart de bandeja con "highest privileges" (opt-in) y, posiblemente, escaneos con la app cerrada | API COM / `schtasks` (**Suposición**) |
| Papelera de Windows | Destino de todo borrado | Crate `trash` / Shell API |
| Registro de Windows | Programas de inicio, Uninstall keys, UserAssist | Win32 (`windows-sys`) |
| WMI | Zonas térmicas ACPI, SMART, info de hardware | `windows` crate (COM) |
| NVML / ADLX | Temperatura GPU NVIDIA / AMD si hay driver | Carga dinámica opcional |
| GitHub Releases | Distribución manual del instalador | Descarga web (fuera de la app) |

La app no hace llamadas de red.

## API (IPC Tauri)

No hay API REST. Contrato entre UI y backend = comandos Tauri (allowlist) y eventos. Resumen por recurso:

| Recurso | Comandos (ejemplos) | Notas |
|---------|--------------------|-------|
| scan | `start_scan`, `cancel_scan`, `get_scan_result` | Emite eventos de progreso |
| cleanup | `preview_cleanup`, `confirm_cleanup`, `get_cleanup_log` | Preview obligatorio antes de confirm |
| duplicates | `start_duplicate_scan`, `get_last_duplicate_scan`, `list_duplicate_groups`, `preview_duplicate_cleanup`, `confirm_duplicate_cleanup` | El usuario elige qué conservar; cancelación con `cancel_scan` / `cancel_cleanup`; la resolución del grupo ocurre al confirmar |
| improvements | `list_improvements`, `apply_improvement`, `undo_improvement` | Lista cerrada |
| monitor | `subscribe_metrics` (evento), `get_history` | Muestreo 1-2 s |
| alerts/rules | `get/set_alert_rules`, `get/set_cleanup_rules`, `get/set_schedule` | Rutas protegidas siempre aplican |
| reports | `export_report(format)` | HTML / PDF |
| settings | `get_settings`, `set_settings` | Modo simple/avanzado, umbrales |
| system | `get_privilege_context` | Detecta over-the-shoulder |

## Distribución

- Instalador **NSIS por máquina** (Program Files, UAC al instalar; MSI reevaluado más adelante) por GitHub Releases, **sin firma** en v0.x. El pipeline crea el tag y un release **borrador**; lo publica el dueño. Advertencia SmartScreen documentada en el README del repo **y** en las notas del release (C-13).
- Sin auto-update en v0.x.

## Nota C-12: comandos del historial de limpieza

`list_cleanup_runs` (limpiezas agrupadas por `scan_id`, o una por cada fila sin `scan_id`, paginadas por cursor, sin rutas) y `list_cleanup_entries` (ítems de una limpieza por clave, con `offset`/`limit`). Ambos son de solo lectura, no reciben rutas y no agregan plugins ni permisos `core:*` (31 permisos `allow-*`). Con contexto `mismatch` el segundo devuelve las rutas nulas (`pathsHidden`). Reemplazan al `get_cleanup_log` anticipado en C-09 D-24.

## Nota C-07: comandos reales del análisis de disco

`start_folder_scan`, `get_last_folder_scan`, `list_folder_children`, `list_large_files`, `pick_folder_root`, `reveal_in_explorer` (más `cancel_scan` existente). El selector y el Explorador usan COM del shell en hilos STA propios, sin plugins `dialog`/`shell`. La interfaz nunca envía rutas: raíz por tipo o `pickId`, elementos por id del último análisis.

## Nota C-06: comandos y flujo del monitor en vivo

`start_monitor_stream` (recibe un `Channel` de la invocación y responde con la ventana actual y el resumen) y `stop_monitor_stream` (por `subscriptionId`). El muestreo corre en un hilo propio desde el arranque y entrega cada muestra al único suscriptor por ese `Channel`: sin `emit`/`listen`, sin permisos `core:*`, sin polling desde la UI. Las respuestas no llevan rutas ni datos de usuario. El monitor no es un `ActiveJob` y no usa la base.

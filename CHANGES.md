# CHANGES — Secuencia de Implementación

> Índice canónico de todos los changes del proyecto **pc-optimizer** (Windows desktop: Tauri 2 + Rust + React + Vite + TS + Tailwind + SQLite local).
> Cada change es atómico: un agente puede implementarlo en una sesión (~4-6 horas), con Strict TDD (Vitest + cargo test).
> **Leer este archivo antes de ejecutar cualquier `/opsx:propose`.**
> Orden por hito: **v0.1 Núcleo de limpieza → v0.2 Sistema y mejoras → v0.3 Asistente continuo**.

**Niveles de governance** (de `knowledge-base/08_arquitectura_propuesta.md`): ALTO = HIGH (borrado de archivos, rutas protegidas, tweaks del sistema, privilegios: proponer y esperar revisión antes de escribir), MEDIO = MEDIUM (scanners, monitor, scheduler, alertas, benchmark, health score: checkpoints), BAJO = LOW (UI, catálogos, tipos, reportes: autonomía si pasan los tests).

**Decisiones resueltas que todo change debe respetar**:
- PA-02: vaciar la Papelera es la ÚNICA acción permanente declarada, con doble confirmación, nunca automática (solo C-10).
- PA-14: la elevación over-the-shoulder bloquea únicamente las categorías por-usuario, con advertencia explicativa (C-05, consumida por C-08/C-09).
- El frontend nunca decide qué es borrable; solo referencia IDs generados por el backend. Sin red, sin telemetría, sin drivers de kernel.

---

## Cómo usar este documento

1. Identificá el siguiente change cuyo estado es `[ ]` y cuyas dependencias están todas en `[x]`.
2. Leé los archivos de su lista **Leer antes** en `knowledge-base/` (y las reglas RN-* citadas en `05_reglas_de_negocio.md`).
3. Ejecutá `/opsx:propose <kebab-name>` (p. ej. `/opsx:propose foundation-setup`), luego `/opsx:apply`.
4. Al terminar, `/opsx:archive <kebab-name>` para sincronizar specs y cerrar el change.
5. Marcá el checkbox de **Estado** (`[ ]` → `[x]`) en este archivo.

---

## Árbol de dependencias

```
C-01 foundation-setup
 └── C-02 domain-ports-and-sqlite-store
      ├── C-03 protected-paths-domain
      │    ├── C-07 disk-usage-scanner            (+C-04)
      │    ├── C-08 junk-detection                (+C-04, C-05)
      │    │    ├── C-09 cleanup-preview-and-trash
      │    │    │    ├── C-10 recycle-bin-empty
      │    │    │    ├── C-11 duplicate-finder
      │    │    │    ├── C-18 one-click-improvements   (+C-16)
      │    │    │    └── C-21 custom-cleanup-rules     (+C-03)
      │    │    └── C-12 dashboard-simple-advanced     (+C-06, C-07)
      │    │         └── C-13 packaging-and-release-pipeline (+C-10, C-11)  ← cierre v0.1
      ├── C-04 ui-shell-and-settings
      │    ├── C-06 live-monitor
      │    │    ├── C-14 hardware-sensors-and-smart
      │    │    └── C-15 metrics-history
      │    └── C-19 unused-apps-suggestions
      └── C-05 privilege-context-and-profile-check
           ├── C-16 improvements-snapshot-engine
           │    ├── C-17 startup-programs-manager
           │    └── C-18 one-click-improvements
           └── C-22 tray-mode-and-autostart
                ├── C-23 scheduled-scans            (+C-08)
                ├── C-24 alerts-and-thresholds      (+C-14, C-15)
                └── C-25 weekly-trends              (+C-07, C-15, C-23)

C-20 health-score           (C-08, C-12, C-14, C-17)
C-26 benchmark-before-after (C-17, C-18)
C-27 report-export          (C-20, C-25, C-26)
```

### Paralelismo por fase

Por orden de hito, los changes de v0.2/v0.3 no se inician hasta cerrar el hito anterior, salvo que se indique lo contrario.

```
GATE 0: C-01 ✓
  → C-02 domain-ports-and-sqlite-store           [Agente A]   (sin paralelismo)

GATE 1: C-02 ✓                                    ← FORK
  → C-03 protected-paths-domain                  [Agente A]
  → C-05 privilege-context-and-profile-check     [Agente B]
  → C-04 ui-shell-and-settings                   [Agente C]

GATE 2: C-03 ✓ + C-04 ✓ + C-05 ✓                  ← FORK
  → C-08 junk-detection                          [Agente A]
  → C-07 disk-usage-scanner                      [Agente B]
  → C-06 live-monitor                            [Agente C]

GATE 3: C-08 ✓
  → C-09 cleanup-preview-and-trash               [Agente A]
  → C-12 dashboard-simple-advanced               [Agente C — si C-06 ✓ y C-07 ✓]

GATE 4: C-09 ✓
  → C-10 recycle-bin-empty                       [Agente A]
  → C-11 duplicate-finder                        [Agente B]
  → C-14 hardware-sensors-and-smart              [Agente C — si C-06 ✓; no toca el release]
  → C-15 metrics-history                         [Agente B — si C-06 ✓; no toca el release]

GATE 5: C-10 ✓ + C-11 ✓ + C-12 ✓
  → C-13 packaging-and-release-pipeline          [Agente A]   (cierre v0.1)

GATE 6: v0.1 cerrado ✓                            ← FORK (v0.2)
  → C-16 improvements-snapshot-engine            [Agente A]
  → C-19 unused-apps-suggestions                 [Agente B]

GATE 7: C-16 ✓
  → C-17 startup-programs-manager                [Agente A]
  → C-18 one-click-improvements                  [Agente B — si C-09 ✓]

GATE 8: C-08 ✓ + C-12 ✓ + C-14 ✓ + C-17 ✓
  → C-20 health-score                            [Agente C]

GATE 9: v0.2 cerrado ✓                            ← FORK (v0.3)
  → C-21 custom-cleanup-rules                    [Agente A]
  → C-22 tray-mode-and-autostart                 [Agente B]
  → C-26 benchmark-before-after                  [Agente C — si C-17 ✓ y C-18 ✓]

GATE 10: C-22 ✓
  → C-23 scheduled-scans                         [Agente B]
  → C-24 alerts-and-thresholds                   [Agente C — si C-14 ✓ y C-15 ✓]

GATE 11: C-23 ✓ + C-15 ✓ + C-07 ✓
  → C-25 weekly-trends                           [Agente B]

GATE 12: C-20 ✓ + C-25 ✓ + C-26 ✓
  → C-27 report-export                           [Agente A]
```

### Camino crítico (8 changes — mínimo irreducible hasta v0.3)

`C-01 → C-02 → C-03 → C-08 → C-09 → C-18 → C-26 → C-27`

(C-18 exige además C-16, que sale de C-02 + C-05: no alarga el camino.)

Camino crítico hasta **v0.1 publicable** (7 changes): `C-01 → C-02 → C-03 → C-08 → C-09 → C-10* / C-11* → C-13` (*co-finales: ambos requieren C-09 y bloquean C-13).

### Plan óptimo con 3 agentes

| Paso | Agente A (Backend núcleo / seguridad) | Agente B (Backend sistema / scanners) | Agente C (Frontend / monitoreo) |
|------|---------------------------------------|---------------------------------------|----------------------------------|
| 1 | C-01 foundation-setup | — | — |
| 2 | C-02 domain-ports-and-sqlite-store | — | — |
| 3 | C-03 protected-paths-domain | C-05 privilege-context-and-profile-check | C-04 ui-shell-and-settings |
| 4 | C-08 junk-detection | C-07 disk-usage-scanner | C-06 live-monitor |
| 5 | C-09 cleanup-preview-and-trash | C-15 metrics-history | C-12 dashboard-simple-advanced |
| 6 | C-10 recycle-bin-empty | C-11 duplicate-finder | C-14 hardware-sensors-and-smart |
| 7 | C-13 packaging-and-release-pipeline (v0.1) | — | — |
| 8 | C-16 improvements-snapshot-engine | C-19 unused-apps-suggestions | — |
| 9 | C-17 startup-programs-manager | C-18 one-click-improvements | — |
| 10 | C-21 custom-cleanup-rules | C-22 tray-mode-and-autostart | C-20 health-score |
| 11 | C-26 benchmark-before-after | C-23 scheduled-scans | C-24 alerts-and-thresholds |
| 12 | — | C-25 weekly-trends | — |
| 13 | C-27 report-export | — | — |

---

## FASE 1 — Fundaciones (v0.1)

> C-03, C-04 y C-05 son independientes entre sí una vez listo C-02.

### [C-01] `foundation-setup`
- **Estado**: `[x]` completado (archivado 2026-11-13)
- **Scope**:
  - Scaffold Tauri 2 + React + Vite + TS + Tailwind (tema oscuro base); estructura `src-tauri/src/{domain,adapters,commands}` y `src/{app,features,shared}`.
  - `tauri.conf.json`: CSP estricta (`default-src 'self'`, sin `unsafe-eval`, `connect-src` vacío); `capabilities/` mínimas sin plugins `shell`/`fs`.
  - Dependencias Rust: sysinfo, walkdir, trash, rusqlite, blake3, windows; sin crates HTTP.
  - Tooling de tests: `cargo test`, Vitest + Testing Library; scripts `npm test`, `npm run build`, `cargo clippy`.
  - Variables `PCOPT_DATA_DIR`, `PCOPT_LOG` (solo dev/tests); `.gitignore` con `.engram/`.
  - Tests: smoke de arranque (cargo + Vitest), verificación de ausencia de crates de red.
- **Dependencias**: ninguna
- **Governance**: BAJO (LOW)
- **Leer antes**:
  - `knowledge-base/08_arquitectura_propuesta.md` §Estructura de directorios, §Seguridad, §Testing
  - `knowledge-base/09_decisiones_y_supuestos.md` §DD-01, DD-10, DD-12
  - `knowledge-base/02_descripcion_general.md`

### [C-02] `domain-ports-and-sqlite-store`
- **Estado**: `[x]` completado (archivado 2026-11-13)
- **Scope**:
  - `domain/ports.rs`: traits `FileSystem`, `Trash`, `Registry`, `Sensors`, `Store` con fakes para tests.
  - Adaptador SQLite (`rusqlite`) con migraciones versionadas por `PRAGMA user_version`; timestamps UTC epoch ms.
  - Migración 001: tablas `settings`, `scan` (retención últimos 50), `cleanup_log` (append-only: sin UPDATE/DELETE desde la app).
  - Seed: `ui_mode=simple`, `autostart_tray=false`, idioma `es`.
  - DB corrupta/migración fallida: mensaje claro, respaldo y recreación.
  - Tests: migraciones idempotentes, seed, append-only del log, fakes de puertos.
- **Dependencias**: `C-01`
- **Governance**: MEDIO (MEDIUM)
- **Leer antes**:
  - `knowledge-base/04_modelo_de_datos.md` §scan, §cleanup_log, §settings, §Seed data
  - `knowledge-base/08_arquitectura_propuesta.md` §Patrones aplicados
  - `knowledge-base/09_decisiones_y_supuestos.md` §DD-08, DD-12

### [C-03] `protected-paths-domain`
- **Estado**: `[ ]` pendiente
- **Scope**:
  - `domain/protection.rs`: `is_protected(path)` puro, fail-closed, algoritmo de 7 pasos (canonicalizar `\\?\`, `..`, 8.3, mayúsculas, reparse points; falla de canonicalización = protegida).
  - Rutas protegidas: `C:\Windows` (excepciones solo `Windows\Temp` y `SoftwareDistribution\Download`), Program Files (x86), `C:\ProgramData` (solo cachés whitelisted), perfiles de otros usuarios, placeholders OneDrive (`skipped_cloud`), `$Recycle.Bin`, System Volume Information, pagefile/hiberfil, raíces de volumen, perfiles de navegador.
  - Función de re-verificación reutilizable para anti-TOCTOU.
  - Tests de tabla: `..`, mezcla de mayúsculas, `\\?\`, 8.3, nombres con espacios/Unicode, junctions/symlinks no seguidos, regla apuntando a `C:\`, `C:\Users`, `C:\Windows`.
- **Dependencias**: `C-02`
- **Governance**: ALTO (HIGH)
- **Leer antes**:
  - `knowledge-base/11_seguridad_limpieza_y_rutas_protegidas.md` §Rutas protegidas, §Algoritmo, §Casos borde
  - `knowledge-base/05_reglas_de_negocio.md` §RN-PROT
  - `knowledge-base/09_decisiones_y_supuestos.md` §DD-06, SU-09

### [C-04] `ui-shell-and-settings`
- **Estado**: `[ ]` pendiente
- **Scope**:
  - `src/app`: shell, routing, providers; cliente IPC en `src/lib/ipc.ts`.
  - Catálogo de strings en español `src/shared/i18n` (RN-GLB-03); UI kit con variantes `simple`/`advanced`.
  - Modo simple por defecto y modo avanzado (sidebar densa), mismo tema oscuro; toggle persistido en `settings.ui_mode` sin reinicio (US-001, US-002).
  - Pantalla Configuración v0.1: modo y autostart (deshabilitado hasta C-22) (US-024); comandos IPC `get_settings`/`set_setting`.
  - Accesibilidad base (WCAG 2.2 AA: foco visible, teclado, reduced-motion, icono+texto para estado).
  - Tests Vitest: cambio de modo, render desde catálogo, persistencia vía IPC mockeado.
  - Estado de la base de datos (Ready/Unavailable) y acción "Respaldar y recrear" con confirmación y mensaje claro cuando está corrupta o no disponible (el archivo dañado se conserva como `.corrupt-<ms>.bak`; nunca se recrea automáticamente). Decidido por el dueño en C-02.
- **Dependencias**: `C-02`
- **Governance**: BAJO (LOW)
- **Leer antes**:
  - `knowledge-base/12_ui_modos_simple_avanzado.md`
  - `knowledge-base/06_funcionalidades.md` §US-001, US-002, US-024
  - `knowledge-base/08_arquitectura_propuesta.md` §Estructura de directorios

### [C-05] `privilege-context-and-profile-check`
- **Estado**: `[ ]` pendiente
- **Scope**:
  - `pc-optimizer.exe.manifest` con `requireAdministrator`; UAC cancelado = la app no inicia.
  - Adaptador `adapters/windows/privileges`: usuario del token vs usuario de la sesión interactiva; si no se puede determinar, se trata como mismatch.
  - Comando `get_privilege_context` -> `{match | mismatch}`; en mismatch: advertencia explicativa y bloqueo SOLO de categorías por-usuario y acciones HKCU (PA-14), sin deshabilitar el resto.
  - Directorio de datos en el perfil de la sesión interactiva (SU-04).
  - UI: banner de advertencia en mismatch; selector de carpeta como alternativa a drag & drop (UIPI).
  - Tests: decisión match/mismatch/desconocido con fakes; el bloqueo por-usuario no afecta categorías de sistema.
- **Dependencias**: `C-02`
- **Governance**: ALTO (HIGH)
- **Leer antes**:
  - `knowledge-base/13_monitoreo_hardware_y_privilegios.md` §Modelo de privilegios, §Over-the-shoulder
  - `knowledge-base/07_flujos_principales.md` §Flujo 1
  - `knowledge-base/05_reglas_de_negocio.md` §RN-PRIV
  - `knowledge-base/10_preguntas_abiertas.md` (IN-03)

---

## FASE 2 — Análisis, limpieza y monitoreo (v0.1)

### [C-06] `live-monitor`
- **Estado**: `[ ]` pendiente
- **Scope**:
  - Muestreo en hilo propio cada 1-2 s de CPU, RAM y disco (`sysinfo`/PDH tras trait `Sensors`); ring buffer en memoria (~5 min).
  - Eventos Tauri (event stream, sin polling desde la UI); impacto de rendimiento acotado; huecos tras suspensión = ausencia de datos.
  - UI: resumen en modo simple; gauges y gráficos en vivo en modo avanzado. Decidir librería de gráficos (PA-01): render eficiente y tema oscuro.
  - Tests: ring buffer, agregación en dominio, fake de sensores, render de gauges.
- **Dependencias**: `C-02, C-04`
- **Governance**: MEDIO (MEDIUM)
- **Leer antes**:
  - `knowledge-base/13_monitoreo_hardware_y_privilegios.md` §Fuentes de métricas, §Muestreo y retención
  - `knowledge-base/06_funcionalidades.md` §US-003
  - `knowledge-base/05_reglas_de_negocio.md` §RN-MON-01, RN-MON-06
  - `knowledge-base/10_preguntas_abiertas.md` (PA-01)

### [C-07] `disk-usage-scanner`
- **Estado**: `[ ]` pendiente
- **Scope**:
  - Migración 002: `folder_snapshot` (UNIQUE(scan_id, path), índice (path, scan_id), `parent_id` autorreferencia).
  - Recorrido con `walkdir` sin seguir junctions/symlinks; progreso por eventos y cancelación; carpetas inaccesibles reportadas sin romper el escaneo.
  - Comandos `start_scan(kind=folders)`, `cancel_scan`; treemap/lista por carpeta y top de archivos pesados (US-006, US-007) ordenable por tamaño y fecha.
  - Acciones: revelar en Explorador o enviar a revisión de limpieza (nunca borra directo).
  - UI: resumen + top 5 (simple); treemap/lista completa (avanzado).
  - Tests: árbol temporal, junction no seguida, cancelación, carpeta sin permisos.
- **Dependencias**: `C-02, C-03, C-04`
- **Governance**: MEDIO (MEDIUM)
- **Leer antes**:
  - `knowledge-base/06_funcionalidades.md` §US-006, US-007
  - `knowledge-base/04_modelo_de_datos.md` §folder_snapshot
  - `knowledge-base/05_reglas_de_negocio.md` §RN-PROT-04
  - `knowledge-base/12_ui_modos_simple_avanzado.md` §Mapa de pantallas

### [C-08] `junk-detection`
- **Estado**: `[ ]` pendiente
- **Scope**:
  - Migración 003: `junk_finding` (CHECK `category` en el enum whitelist, `protected_flag`).
  - `domain/junk.rs`: whitelist `temp_user`, `temp_windows`, `browser_cache_{chrome,edge,firefox,brave}` (solo directorios de caché), `old_logs` (>= 30 días), `wu_cache`, `thumbnail_cache`, `crash_dumps`, `recycle_bin` (solo detección y tamaño).
  - Cada candidato pasa por `is_protected`; los protegidos se persisten como omitidos.
  - Con `privilege_context=mismatch`, las categorías por-usuario no se escanean y se informa por qué.
  - `start_scan(kind=junk)` con progreso; UI: tarjetas por categoría con tamaño (simple), rutas (avanzado). US-008.
  - Tests: tabla por categoría, ruta protegida omitida, mismatch bloquea por-usuario, junction no seguida.
- **Dependencias**: `C-03, C-04, C-05`
- **Governance**: ALTO (HIGH)
- **Leer antes**:
  - `knowledge-base/11_seguridad_limpieza_y_rutas_protegidas.md` §Categorías limpiables
  - `knowledge-base/04_modelo_de_datos.md` §junk_finding
  - `knowledge-base/05_reglas_de_negocio.md` §RN-LIM-02, RN-PROT-01, RN-PROT-02
  - `knowledge-base/07_flujos_principales.md` §Flujo 2

### [C-09] `cleanup-preview-and-trash`
- **Estado**: `[ ]` pendiente
- **Scope**:
  - `domain/cleanup.rs`: `preview_cleanup` (ítems, cantidad, tamaño total, `preview_id`) y `confirm_cleanup(preview_id)`; el frontend solo envía IDs de hallazgos.
  - Re-verificación `is_protected` justo antes de mover (anti-TOCTOU), incluido archivo reemplazado por symlink.
  - Mueve cada ítem a la Papelera (crate `trash`); outcomes `moved_to_trash`, `skipped_locked`, `skipped_protected`, `skipped_cloud`, `error` con detalle; un error no detiene el resto; volumen sin Papelera se omite y reporta.
  - Escritura en `cleanup_log` por ítem; resumen (liberado, omitidos); respeta bloqueo por-usuario en mismatch.
  - UI: preview con confirmación explícita; cerrar el preview no ejecuta nada. US-009.
  - Tests: fakes de FS/Trash, todos los outcomes, TOCTOU, log append-only, mismatch.
- **Dependencias**: `C-08`
- **Governance**: ALTO (HIGH)
- **Leer antes**:
  - `knowledge-base/11_seguridad_limpieza_y_rutas_protegidas.md` §Flujo de seguridad, §Registro
  - `knowledge-base/07_flujos_principales.md` §Flujo 2
  - `knowledge-base/05_reglas_de_negocio.md` §RN-LIM-01, RN-LIM-03..05, RN-LIM-07
  - `knowledge-base/09_decisiones_y_supuestos.md` §DD-05

### [C-10] `recycle-bin-empty`
- **Estado**: `[ ]` pendiente
- **Scope**:
  - Comando `empty_recycle_bin`: única acción permanente declarada (PA-02); nunca automática, nunca invocable por scheduler ni por mejoras.
  - Doble confirmación propia en UI (dos pasos distintos, con tamaño a liberar); bloqueada en mismatch (la Papelera es por-usuario).
  - Registro en `cleanup_log` con outcome `recycle_bin_emptied`. US-010.
  - Tests: no ejecuta sin ambas confirmaciones, no corre en mismatch, log escrito, ninguna otra ruta del código usa borrado permanente.
- **Dependencias**: `C-09`
- **Governance**: ALTO (HIGH)
- **Leer antes**:
  - `knowledge-base/06_funcionalidades.md` §US-010
  - `knowledge-base/05_reglas_de_negocio.md` §RN-LIM-06
  - `knowledge-base/10_preguntas_abiertas.md` (IN-02)
  - `knowledge-base/11_seguridad_limpieza_y_rutas_protegidas.md` §Categorías limpiables

### [C-11] `duplicate-finder`
- **Estado**: `[ ]` pendiente
- **Scope**:
  - Migración 004: `duplicate_group` (índices (scan_id, size_bytes), (blake3)) y `duplicate_file` (`kept` solo por decisión del usuario).
  - `domain/duplicates.rs`: etapas tamaño -> hash parcial -> blake3 completo; excluye < 1 MB por defecto (SU-06); sin seguir links.
  - Nunca auto-borrado ni auto-selección de `kept`; siempre queda al menos una copia; el borrado delega en el flujo de C-09.
  - UI: grupos con elección guiada (simple); hashes, rutas y filtros (avanzado). US-011.
  - Tests: grupos de 2/3 copias, falso positivo en hash parcial, intento de borrar todas las copias rechazado.
- **Dependencias**: `C-09`
- **Governance**: ALTO (HIGH)
- **Leer antes**:
  - `knowledge-base/05_reglas_de_negocio.md` §RN-DUP-01..06
  - `knowledge-base/04_modelo_de_datos.md` §duplicate_group / duplicate_file
  - `knowledge-base/09_decisiones_y_supuestos.md` §DD-13, SU-06
  - `knowledge-base/06_funcionalidades.md` §US-011

### [C-12] `dashboard-simple-advanced`
- **Estado**: `[ ]` pendiente
- **Scope**:
  - Dashboard v0.1 modo simple: tarjetas (Espacio, Basura, Memoria) con una acción primaria ("Limpiar ahora") y textos llanos desde el catálogo.
  - Dashboard modo avanzado: sidebar, gauges y métricas crudas de C-06, rutas y detalle; el score completo llega en C-20.
  - Pantalla de log de limpieza (rutas completas en avanzado).
  - Tests Vitest: una acción primaria por pantalla, alternancia de modo, estado "no disponible" explícito.
- **Dependencias**: `C-06, C-07, C-08`
- **Governance**: BAJO (LOW)
- **Leer antes**:
  - `knowledge-base/12_ui_modos_simple_avanzado.md`
  - `knowledge-base/06_funcionalidades.md` §US-001, US-002
  - `knowledge-base/10_preguntas_abiertas.md` (IN-01)

### [C-13] `packaging-and-release-pipeline`
- **Estado**: `[ ]` pendiente
- **Scope**:
  - Bundle NSIS/MSI (`npm run tauri build`) con manifest elevado; sin firma en v0.x; release en GitHub (tag v0.1.0).
  - CI: `cargo test`, `cargo clippy`, `npm test`, `npm run build`, `cargo audit`, `npm audit`, verificación de ausencia de crates de red (`cargo tree`).
  - Verificación manual guiada de v0.1 en Windows 10 22H2 y 11 (SU-05).
  - Tests: pipeline en rojo ante crate de red o test fallido (gate verificable).
- **Dependencias**: `C-10, C-11, C-12`
- **Governance**: BAJO (LOW)
- **Leer antes**:
  - `knowledge-base/08_arquitectura_propuesta.md` §CI / Build
  - `knowledge-base/09_decisiones_y_supuestos.md` §DD-11, SU-05
  - `knowledge-base/01_vision_y_objetivos.md`

---

## FASE 3 — Monitoreo avanzado (v0.2)

### [C-14] `hardware-sensors-and-smart`
- **Estado**: `[ ]` pendiente
- **Scope**:
  - Adaptadores tras trait `Sensors`: SMART vía WMI (`MSStorageDriver_FailurePredictStatus`, `MSFT_PhysicalDisk`), temperatura de disco NVMe/SATA, GPU vía NVML/ADLX cargados dinámicamente y opcionales, zona térmica ACPI (`MSAcpi_ThermalZoneTemperature`) si existe.
  - Sin drivers de kernel (WinRing0 rechazado); lecturas lentas no bloquean la UI.
  - "No disponible" con explicación breve, nunca valores inventados; SMART crítico genera alerta en UI. US-004.
  - Tests: fakes por adaptador, ausencia de sensor, SMART crítico.
- **Dependencias**: `C-06`
- **Governance**: MEDIO (MEDIUM)
- **Leer antes**:
  - `knowledge-base/13_monitoreo_hardware_y_privilegios.md` §Fuentes de métricas, §Gobernanza de adaptadores
  - `knowledge-base/05_reglas_de_negocio.md` §RN-MON-04, RN-MON-05, RN-SAL-04
  - `knowledge-base/09_decisiones_y_supuestos.md` §DD-04

### [C-15] `metrics-history`
- **Estado**: `[ ]` pendiente
- **Scope**:
  - Migración 005: `metric_sample_minute` (purga rolling 24 h) y `metric_hourly` (~90 días), `temp_max` nullable.
  - Agregador en dominio: muestras 1-2 s -> minuto -> hora; huecos como ausencia de datos.
  - Gráficos de historial 24 h en modo avanzado. US-005.
  - Tests: agregación, purga a las 24 h, hueco tras suspensión, temperatura nula.
- **Dependencias**: `C-06`
- **Governance**: MEDIO (MEDIUM)
- **Leer antes**:
  - `knowledge-base/04_modelo_de_datos.md` §metric_sample_minute, §metric_hourly
  - `knowledge-base/13_monitoreo_hardware_y_privilegios.md` §Muestreo y retención
  - `knowledge-base/05_reglas_de_negocio.md` §RN-MON-02, RN-MON-03

---

## FASE 4 — Sistema y mejoras (v0.2)

### [C-16] `improvements-snapshot-engine`
- **Estado**: `[ ]` pendiente
- **Scope**:
  - Migración 006: `improvement`, `improvement_snapshot` (`prev_state_json`, `reverted_at`).
  - `domain/improvements.rs`: catálogo cerrado en código (`startup.disable:<entry>`, `power.plan:<guid>`, `visual.preset:<name>`, `cleanup.whitelist:<category>`) con riesgo e impacto estimado.
  - Motor apply/undo: snapshot persistido ANTES de aplicar (sin snapshot no se aplica); undo exacto comparando estado actual; si cambió externamente, aviso y confirmación (RN-MEJ-04); falla al aplicar descarta el snapshot.
  - Acciones HKCU bloqueadas bajo mismatch (C-05).
  - Tests con fake de `Registry`: snapshot-antes-de-aplicar, undo exacto, estado externo cambiado, fallo de aplicación.
  - Define el port `Registry` (diferido desde C-02).
- **Dependencias**: `C-02, C-05`
- **Governance**: ALTO (HIGH)
- **Leer antes**:
  - `knowledge-base/07_flujos_principales.md` §Flujo 3
  - `knowledge-base/05_reglas_de_negocio.md` §RN-MEJ-01..05
  - `knowledge-base/04_modelo_de_datos.md` §improvement, §improvement_snapshot
  - `knowledge-base/09_decisiones_y_supuestos.md` §DD-07

### [C-17] `startup-programs-manager`
- **Estado**: `[ ]` pendiente
- **Scope**:
  - Adaptador `adapters/windows/registry` + carpetas Startup: listar entradas de inicio con impacto y riesgo.
  - Deshabilitar/habilitar sobre el motor de C-16 (snapshot, undo exacto). US-013.
  - UI: lista con impacto (simple), detalle de entradas (avanzado).
  - Tests: listar, deshabilitar + undo, entrada modificada externamente.
- **Dependencias**: `C-16`
- **Governance**: ALTO (HIGH)
- **Leer antes**:
  - `knowledge-base/06_funcionalidades.md` §US-013
  - `knowledge-base/05_reglas_de_negocio.md` §RN-MEJ-01..04
  - `knowledge-base/12_ui_modos_simple_avanzado.md` §Mapa de pantallas

### [C-18] `one-click-improvements`
- **Estado**: `[ ]` pendiente
- **Scope**:
  - Mejoras de lista cerrada: plan de energía, preset de efectos visuales, limpieza whitelisted (reutiliza preview/Papelera/log de C-09; nunca vacía la Papelera).
  - UI: cartas con riesgo e impacto en palabras, confirmación previa y botón de deshacer visible; historial y snapshots (avanzado). US-015.
  - Tests: aplicar/revertir cada tipo, riesgo/impacto visibles, ninguna mejora fuera del catálogo.
- **Dependencias**: `C-09, C-16`
- **Governance**: ALTO (HIGH)
- **Leer antes**:
  - `knowledge-base/06_funcionalidades.md` §US-015
  - `knowledge-base/05_reglas_de_negocio.md` §RN-MEJ-01..05
  - `knowledge-base/07_flujos_principales.md` §Flujo 3
  - `knowledge-base/09_decisiones_y_supuestos.md` §DD-07

### [C-19] `unused-apps-suggestions`
- **Estado**: `[ ]` pendiente
- **Scope**:
  - Detección vía claves Uninstall del registro + señal de último uso best-effort; solo sugiere (DD-14).
  - "Desinstalar" abre el desinstalador del programa; la app nunca desinstala por su cuenta. US-014.
  - Tests con fake de `Registry`: app sin señal de uso, con uso reciente, desinstalador ausente.
- **Dependencias**: `C-04`
- **Governance**: MEDIO (MEDIUM)
- **Leer antes**:
  - `knowledge-base/06_funcionalidades.md` §US-014
  - `knowledge-base/05_reglas_de_negocio.md` §RN-MEJ-06
  - `knowledge-base/09_decisiones_y_supuestos.md` §DD-14

### [C-20] `health-score`
- **Estado**: `[ ]` pendiente
- **Scope**:
  - `domain/health_score.rs`: puntaje 0-100 con pesos configurables (SU-01), explicación de los factores que lo bajan y métricas no disponibles marcadas.
  - Etiquetas ("Excelente", "Mejorable", "Atención") con icono + texto; se integra al dashboard de C-12 (US-016, completa US-001).
  - Tests de tabla: factores faltantes (sin SMART/temperatura), extremos 0 y 100, explicación coherente.
- **Dependencias**: `C-08, C-12, C-14, C-17`
- **Governance**: MEDIO (MEDIUM)
- **Leer antes**:
  - `knowledge-base/05_reglas_de_negocio.md` §RN-SAL-01..06
  - `knowledge-base/09_decisiones_y_supuestos.md` §SU-01
  - `knowledge-base/12_ui_modos_simple_avanzado.md` §Modo simple

---

## FASE 5 — Asistente continuo (v0.3)

### [C-21] `custom-cleanup-rules`
- **Estado**: `[ ]` pendiente
- **Scope**:
  - Migración 007: `cleanup_rule` (`path_pattern` glob, `min_age_days`, `min_size_bytes`, `enabled`); categoría `custom_rule`.
  - Validación en dominio: rechaza reglas que toquen rutas protegidas (`C:\`, `C:\Users`, `C:\Windows`, etc.) vía `is_protected`.
  - Preview obligatorio y mismo flujo de C-09 (Papelera + log); editor solo en modo avanzado (SU-07). US-012.
  - Tests: reglas rechazadas, filtros de antigüedad/tamaño, preview obligatorio.
- **Dependencias**: `C-03, C-09`
- **Governance**: ALTO (HIGH)
- **Leer antes**:
  - `knowledge-base/06_funcionalidades.md` §US-012
  - `knowledge-base/05_reglas_de_negocio.md` §RN-LIM-08, RN-PROT-06
  - `knowledge-base/11_seguridad_limpieza_y_rutas_protegidas.md` §Casos borde
  - `knowledge-base/09_decisiones_y_supuestos.md` §SU-07

### [C-22] `tray-mode-and-autostart`
- **Estado**: `[ ]` pendiente
- **Scope**:
  - Icono de bandeja; cerrar ventana minimiza a bandeja; muestreo reducido (5-10 s) con ventana oculta.
  - Autostart opt-in vía tarea de Task Scheduler "run with highest privileges" al iniciar sesión (no clave `Run`); desactivar la opción o desinstalar elimina la tarea. US-017.
  - Activa `autostart_tray` en Configuración (C-04). Adaptador `adapters/windows/task_scheduler` con fake.
  - Tests: crear/eliminar tarea, idempotencia, desinstalación limpia.
- **Dependencias**: `C-05`
- **Governance**: ALTO (HIGH)
- **Leer antes**:
  - `knowledge-base/13_monitoreo_hardware_y_privilegios.md` §Autostart de la bandeja
  - `knowledge-base/05_reglas_de_negocio.md` §RN-PRIV-01, RN-PRIV-02
  - `knowledge-base/09_decisiones_y_supuestos.md` §DD-02, DD-03

### [C-23] `scheduled-scans`
- **Estado**: `[ ]` pendiente
- **Scope**:
  - Migración 008: `schedule` (diario/semanal, `next_run`); scheduler dentro del proceso de bandeja disparando `scan(trigger=scheduled)`.
  - Los escaneos programados solo detectan y notifican, nunca limpian (SU-03); ninguno habilitado por defecto.
  - UI: activar/desactivar (simple), frecuencia y hora (avanzado). US-018. Comportamiento con la app cerrada según pregunta abierta en `10_preguntas_abiertas.md`.
  - Tests: cálculo de `next_run`, no limpieza automática, deshabilitado por defecto.
- **Dependencias**: `C-08, C-22`
- **Governance**: MEDIO (MEDIUM)
- **Leer antes**:
  - `knowledge-base/07_flujos_principales.md` §Flujo 4
  - `knowledge-base/06_funcionalidades.md` §US-018
  - `knowledge-base/09_decisiones_y_supuestos.md` §SU-03
  - `knowledge-base/05_reglas_de_negocio.md` §RN-ALE-04

### [C-24] `alerts-and-thresholds`
- **Estado**: `[ ]` pendiente
- **Scope**:
  - Migración 009: `alert_rule` (seed: disco > 90 %, temperatura > 85 °C, RAM > 90 % por >= 60 s; cooldown 6 h) y `alert`.
  - `domain/alerts.rs`: evaluación de umbrales y cooldown anti-spam; regla sin sensor no evalúa ni alerta.
  - Notificación de bandeja; click abre la app en la pantalla relevante. Umbrales configurables en Configuración (US-024 v0.3). US-019.
  - Tests de tabla: umbral superado, cooldown vigente, sensor ausente.
- **Dependencias**: `C-14, C-15, C-22`
- **Governance**: MEDIO (MEDIUM)
- **Leer antes**:
  - `knowledge-base/05_reglas_de_negocio.md` §RN-ALE-01..05
  - `knowledge-base/04_modelo_de_datos.md` §alert_rule, §alert
  - `knowledge-base/09_decisiones_y_supuestos.md` §SU-02
  - `knowledge-base/07_flujos_principales.md` §Flujo 4

### [C-25] `weekly-trends`
- **Estado**: `[ ]` pendiente
- **Scope**:
  - Snapshots periódicos de carpetas (semanal, top-N por nivel, ~90 días) y consulta "qué carpeta crece más" sobre `folder_snapshot(path, scan_id)`.
  - Tendencias con agregados horarios de C-15; UI avanzada de tendencias. US-020.
  - Tests: ranking de crecimiento, retención 90 días, datos insuficientes.
- **Dependencias**: `C-07, C-15, C-23`
- **Governance**: MEDIO (MEDIUM)
- **Leer antes**:
  - `knowledge-base/06_funcionalidades.md` §US-020
  - `knowledge-base/04_modelo_de_datos.md` §folder_snapshot, §metric_hourly
  - `knowledge-base/05_reglas_de_negocio.md` §RN-MON-02, RN-MON-03

### [C-26] `benchmark-before-after`
- **Estado**: `[ ]` pendiente
- **Scope**:
  - Migración 010: `benchmark_run` (`label` before/after/manual, relaciones opcionales a `scan` e `improvement`).
  - Medición de disco lectura/escritura, CPU single loop y cantidad de apps de inicio; comparación antes/después de mejoras. US-021.
  - Tests: comparación con fakes, retención 90 días.
- **Dependencias**: `C-17, C-18`
- **Governance**: MEDIO (MEDIUM)
- **Leer antes**:
  - `knowledge-base/06_funcionalidades.md` §US-021
  - `knowledge-base/04_modelo_de_datos.md` §benchmark_run
  - `knowledge-base/05_reglas_de_negocio.md` §RN-REP-02

### [C-27] `report-export`
- **Estado**: `[ ]` pendiente
- **Scope**:
  - `export_report(format, dest)`: HTML por plantilla local y PDF (decisión abierta: impresión del webview vs crate Rust; resolver al proponer). Rutas ofuscadas por defecto.
  - Recopila score, hallazgos, `cleanup_log`, mejoras, tendencias y benchmark; secciones sin datos se omiten indicándolo; manejo de destino sin permisos o disco lleno. US-022.
  - Tests: render con/sin benchmark, ofuscación de rutas, error de destino.
- **Dependencias**: `C-20, C-25, C-26`
- **Governance**: BAJO (LOW)
- **Leer antes**:
  - `knowledge-base/07_flujos_principales.md` §Flujo 5
  - `knowledge-base/05_reglas_de_negocio.md` §RN-REP-01, RN-REP-03, RN-REP-04
  - `knowledge-base/10_preguntas_abiertas.md` (decisión PDF)

---

## Resumen

| Hito | Changes | ALTO | MEDIO | BAJO |
|------|---------|------|-------|------|
| v0.1 Núcleo de limpieza | C-01 a C-13 (13) | C-03, C-05, C-08, C-09, C-10, C-11 | C-02, C-06, C-07 | C-01, C-04, C-12, C-13 |
| v0.2 Sistema y mejoras | C-14 a C-20 (7) | C-16, C-17, C-18 | C-14, C-15, C-19, C-20 | — |
| v0.3 Asistente continuo | C-21 a C-27 (7) | C-21, C-22 | C-23, C-24, C-25, C-26 | C-27 |

**Total**: 27 changes en 5 fases, 13 gates (GATE 0-12). **Primer change recomendado**: `C-01` -> `/opsx:propose foundation-setup`.

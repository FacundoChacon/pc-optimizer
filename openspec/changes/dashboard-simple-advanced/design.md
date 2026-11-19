# Design

> Gobernanza **LOW** (UI, catálogo y dos consultas de solo lectura). Toca dos puntos de la línea base que siempre se aprueban explícitamente: la superficie IPC y las capabilities (+2 permisos `allow-*`). La acción "Limpiar ahora" **no** agrega ningún camino de borrado: solo abre el preview de C-09. Las decisiones marcadas **[DUEÑO]** requieren aprobación antes del apply; las tareas asumen la opción recomendada y, si el dueño elige otra, se actualizan specs y tasks antes de codear (`/opsx:update`).

## Context

Motivación: ver `proposal.md`. Estado actual relevante (observado en el código, no supuesto):

- **Inicio** (`src/features/dashboard/HomeScreen.tsx`): solo un `Card` con `es.home.emptyTitle`/`emptyBody`; sin datos ni acciones. Es `DEFAULT_SCREEN` en `src/app/navigation.ts` (registro plano de 6 pantallas: Inicio, Espacio en disco, Limpieza, Duplicados, Monitor, Configuración).
- **Shell (C-04)**: `SimpleLayout` (cabecera con navegación horizontal) y `AdvancedLayout` (**sidebar ya existente**); `DensityProvider` por modo; `NavigationProvider.navigate(id)` cambia de pantalla y **mueve el foco a `<main id="main">`** en un efecto del proveedor (corre después de los efectos de la pantalla nueva).
- **Proveedores de sesión** (`AppProviders`): `JunkScanProvider` (`last: JunkScanSummary | null`, `progress`, `start`, `error`), `CleanupProvider` (`useCleanup`: `open()` pide el preview del último escaneo y abre el diálogo; `useEmptyRecycleBin`), `DuplicatesProvider`, `DiskProvider` (`scan.last: FolderScanSummary | null` vía `get_last_folder_scan`, `scan.progress`), `MonitorProvider` (`useMonitor()` abre la suscripción única mientras haya consumidores), `StoreStatusProvider`, `PrivilegeProvider`. `NavigationProvider` es el más interno: Inicio puede usar todos.
- **Limpieza (C-09)**: el diálogo `CleanupPreviewDialog` se renderiza solo dentro de `CleanupScreen`; su estado (`dialogOpen`, `preview`, `selected`) vive en el proveedor. El diálogo usa `<dialog>.showModal()`, enfoca "Cancelar" al abrir y devuelve el foco al elemento que tenía el foco antes de abrir. "Limpiar" solo aparece con `canClean` (último escaneo `done`, con elegibles, no limpiado, sin trabajos). `JunkScanSummary.totalBytes` ya excluye la Papelera; `cleanedAt` deriva del log.
- **Monitor (C-06)**: `MonitorTiles` (tarjetas simples con `MetricLine`/`SpaceLine` internas), `MeterList` (medidores), `LiveCharts` (4 gráficos con `TimeSeriesChart`), `metricDisplay`, `monitorText` (`levelPhrase`, `spaceLevelText`). Nota de C-06 para C-12: "consume `useMonitor` y `Meter`".
- **Disco (C-07)**: `FolderScanSummary` trae `root.path`, `volume {totalBytes, freeBytes}`, `totalBytes`, `inaccessibleDirs`, `cloudFiles`, fechas. Nota de C-07: "el dashboard puede consumir `get_last_folder_scan`".
- **Log (`cleanup_log`, migración 001)**: append-only (triggers), una fila por ítem procesado (`ts`, `category`, `path`, `size_bytes`, `outcome`, `scan_id` nullable, `error_detail` código estable). Solo índice `(ts)`. Las limpiezas de basura escriben el `scan_id` del escaneo (una limpieza por escaneo: `scan_already_cleaned`); las de duplicados, el `scan_id` del escaneo de duplicados con `category = duplicates` (puede haber varias por escaneo); el vaciado de la Papelera, una fila con `category = recycle_bin`, `path = C:\`, `scan_id` nulo. El puerto `CleanupLogStore` solo ofrece `append_cleanup_log`, `recent_cleanup_log(limit)` y `scan_cleaned_at`; **no hay ningún comando IPC que lea el log** (C-09 D-24 lo dejó para C-12).
- **Seguridad**: 29 permisos `allow-*`, 0 `core:*`, 0 plugins (`tests/security_baseline.rs`); `APP_COMMANDS` en `commands/names.rs` genera los permisos en `build.rs`.
- **Tests**: `Button` expone `data-variant`; `src/tests/uiLiterals.test.ts` prohíbe literales de UI en componentes; `mockBackend.ts` simula los comandos; fixtures por feature en `src/tests/support/`.

## Goals / Non-Goals

**Goals:**
- Inicio útil sin conocimientos técnicos (simple) y denso para quien quiere detalle (avanzado), **con el mismo componente por bloque** y densidad por modo (spec `ui-modes`).
- Reutilizar estado y componentes existentes; ninguna consulta nueva para lo que ya está en un proveedor.
- "Limpiar ahora" como atajo al flujo seguro existente (escaneo → preview → confirmación → Papelera), nunca como camino nuevo de ejecución.
- Historial legible por limpieza y auditable por ítem, sin cambiar el esquema ni el carácter append-only del log.

**Non-Goals:**
- Health score, tarjetas de Inicio de Windows y Salud del disco (C-20, C-17, C-14).
- Vaciar la Papelera o resolver duplicados desde Inicio.
- Purga, exportación u ofuscación de rutas para reportes (C-27; KB 04 "purga manual" sigue como suposición).
- Copiar rutas al portapapeles o revelar ítems del historial en el Explorador (requeriría IPC o permisos nuevos).

## Decisions

### D-01 — Inicio en modo simple: tres tarjetas y una acción **[DUEÑO]**

- **A (recomendada)**: encabezado "Inicio" y una frase de contexto; debajo, **"Limpiar ahora"** (única `variant="primary"`) con una línea de ayuda asociada por `aria-describedby` ("Primero vas a ver qué se mueve a la Papelera. Nada se borra sin tu confirmación."); luego tres tarjetas en grilla (1 columna en angosto, 3 en `md`): **Espacio**, **Basura**, **Memoria**, cada una con su cifra principal en texto grande, una frase llana y un enlace secundario **"Ver detalle"** (botón con estilo de enlace, no primario) que navega a Espacio en disco, Limpieza o Monitor. Textos de ejemplo (catálogo):
  - Espacio: "Te quedan 120 GB libres de 476 GB" + nivel "Normal"/"Casi lleno" con icono.
  - Basura: "Podés liberar hasta 1,2 GB" + "Según la búsqueda del 19/11, 10:30"; sin escaneo: "Todavía no buscaste basura"; ya limpiado: "Ya limpiaste lo que encontró la última búsqueda (19/11)"; si la Papelera ocupa algo: "La Papelera ocupa 2 GB. Podés vaciarla desde Limpieza."
  - Memoria: "Usando 62 % (9,8 GB de 16 GB)" + nivel y la frase de `levelPhrase`.
- B: la acción primaria cambia de etiqueta según el estado ("Buscar basura" sin escaneo, "Limpiar ahora" con elegibles). Más explícito pero dos acciones primarias distintas en la misma pantalla según el momento.
- C: sin acción primaria en Inicio (solo enlaces). Contradice US-001.
- Orden de tarjetas fijo (Espacio, Basura, Memoria) como en `CHANGES.md`. Sin gráficos en simple.

### D-02 — Qué hace "Limpiar ahora" **[DUEÑO]**

Siempre lleva a la pantalla **Limpieza** y nunca ejecuta nada por sí misma; lo único que cambia es si antes hace falta buscar basura.

- **A (recomendada)**: función pura `cleanNowPlan(last, now)`:
  - **`preview`** si el último escaneo de basura es `done`, no fue limpiado (`cleanedAt = null`), tiene al menos una categoría limpiable no bloqueada con elementos, y terminó hace **menos de 60 min** → navegar a Limpieza y abrir el preview de C-09 (todas las limpiables marcadas, foco en "Cancelar").
  - **`scan_then_preview`** en cualquier otro caso (sin escaneo, cancelado, ya limpiado, sin elegibles o de hace ≥ 60 min) → navegar a Limpieza, iniciar "Buscar basura" (progreso y cancelar visibles allí) y, si termina `done` con elegibles, **abrir el preview automáticamente**; si no hay elegibles, se ve el resultado de Limpieza y se anuncia "No encontramos archivos para limpiar". Si se cancela o falla, no se abre nada.
  - El pedido se descarta si el usuario sale de Limpieza antes de que termine el escaneo (el escaneo sigue, como siempre) y nunca abre el diálogo en otra pantalla.
  - Por qué 60 min: un escaneo reciente evita esperar de nuevo; uno viejo mostraría tamaños que ya no son reales (y C-09 omitiría muchos ítems por `changed_since_scan`). El umbral es una constante del frontend.
- B: siempre escanear antes del preview. Un solo camino y cifras siempre frescas, pero obliga a esperar aunque el usuario acabe de buscar.
- C: solo navegar a Limpieza, sin abrir nada. Más simple, pero "Limpiar ahora" no hace lo que dice (dos clics más).
- D: preview, progreso y resumen dentro de Inicio. Duplica en Inicio toda la UI de C-09 (progreso, resumen, errores, aviso de ya limpiado) y la acción de vaciar Papelera del resumen. Rechazada.

### D-03 — Mecánica del pedido y del foco

- `CleanupProvider` agrega un pedido de sesión `cleanNow` (`request(plan)`, `pending`, `consume()`, `drop()`); Inicio llama `request` y `navigate("cleanup", { focus: "none" })`. `CleanupScreen`, al montarse, consume el pedido: `preview` → `cleaning.open()`; `scan_then_preview` → `junk.start()` y, al recibir el resumen `done` con elegibles, `cleaning.open()`. Al desmontarse con un pedido pendiente, lo descarta.
- `NavigationProvider.navigate(id, options?)`: `focus: "main"` (por defecto, comportamiento actual) o `"none"` (el foco lo maneja la pantalla destino). Sin `"none"`, el efecto del proveedor enfocaría `<main>` después de que el diálogo enfocó "Cancelar".
- Con `scan_then_preview` el foco va al título del progreso del escaneo (como al pulsar "Buscar basura") y luego al "Cancelar" del diálogo cuando se abre.
- Al cerrar el diálogo abierto desde Inicio, el elemento que tenía el foco ya no existe (el botón de Inicio se desmontó): `CleanupPreviewDialog` recibe un destino de respaldo y el foco vuelve a "Limpiar" de Limpieza (o al encabezado si no hay "Limpiar"), igual que el escenario "Escape" de la spec `cleanup`.
- No cambia ningún comando ni regla de la limpieza: el preview, la confirmación, la re-verificación y la Papelera son los de C-09.

### D-04 — Cuándo "Limpiar ahora" está deshabilitado

- Deshabilitado (botón `disabled` + motivo en texto visible asociado por `aria-describedby`) si: hay un escaneo de basura, una limpieza, un vaciado de Papelera, un análisis de disco, una búsqueda de duplicados o una limpieza de duplicados en curso (lo que exponen los proveedores) → "Esperá a que termine {tarea} para limpiar."; o la base no está `ready` (`StoreStatusProvider`) → "No se puede limpiar mientras la base de datos no esté disponible.".
- En `mismatch` **no** se deshabilita: las categorías de sistema siguen siendo limpiables y el preview de C-09 ya bloquea las por-usuario con su motivo.
- Si aun así el backend rechaza el escaneo por un trabajo no visible (p. ej. `scan_in_progress`), el error se muestra en Limpieza con el mensaje del catálogo (comportamiento existente).

### D-05 — Fuentes de datos de las tarjetas **[DUEÑO]**

| Tarjeta | Fuente principal | Respaldo | Sin datos |
|---|---|---|---|
| Espacio | `useMonitor()`: `volumeFreeBytes`/`volumeTotalBytes` de la última muestra y `summary.volume.level` (casi lleno ≥ 90 %, D-09 de C-06) | `DiskProvider.scan.last.volume` del último análisis `done` de `C:\`, con "según el análisis del {fecha}" y sin nivel | "No disponible" + motivo |
| Basura | `JunkScanProvider.last`: suma de `sizeBytes` de las categorías limpiables `done`/`partial` (igual que `totalBytes`, sin la Papelera), fecha de fin, `cleanedAt` | — | "Todavía no buscaste basura" (no es "no disponible") |
| Memoria | `useMonitor()`: `summary.memory` (actual, nivel) y `memUsedBytes`/`memTotalBytes` | — | "No disponible" / "Midiendo…" |

- **A (recomendada)**: la tabla. El espacio en vivo es más exacto que el del último análisis (que puede tener días); el respaldo cubre el monitor caído. Inicio pasa a ser consumidor de `useMonitor` (la suscripción queda abierta mientras Inicio está visible, que es la pantalla de arranque; el muestreo ya corre siempre por D-01 de C-06 y su costo con la pantalla visible se midió en 0,014 %).
- B: Espacio solo del último análisis de disco. Sin suscripción en Inicio, pero cifras viejas o ausentes hasta analizar `C:` (7 min en debug).
- C: Memoria con el promedio de 60 s en vez del valor actual. Más estable pero no coincide con lo que muestra Monitor en simple.

### D-06 — Estados explícitos por bloque

Un componente puro `cardState(...)` decide qué ve cada tarjeta; cada estado tiene texto del catálogo, nunca un 0 inventado ni un bloque vacío:

| Estado | Cuándo | Texto (simple) |
|---|---|---|
| Cargando | proveedor sin leer (`loaded = false`) o monitor `loading` | "Cargando…" |
| Midiendo | métrica `warming_up` | "Midiendo…" |
| No disponible | monitor `unavailable`, métrica `source_error`/`not_supported`, lectura del último escaneo fallida (`store_unavailable`, etc.) | "No disponible" + explicación breve (motivo del catálogo de C-06 o mensaje de error por `kind`) |
| Sin datos recientes | `stale` del monitor | valores marcados "sin datos recientes" |
| Sin análisis | no hay escaneo de basura | "Todavía no buscaste basura" |
| Con datos | resto | cifra + frase |

En avanzado, el estado no disponible muestra además el código crudo (`not_supported`, `store_unavailable`).

### D-07 — Inicio en modo avanzado **[DUEÑO]**

- **A (recomendada)**, de arriba abajo, en la densidad compacta y con la sidebar existente:
  1. Encabezado + "Limpiar ahora" (misma acción primaria y mismas reglas de D-02/D-04).
  2. **Sistema**: `MeterList` de C-06 (CPU, memoria, tiempo activo, espacio con actual, promedio 60 s y pico) y **un** gráfico en vivo de 5 min con dos series (CPU % y memoria %) sobre `TimeSeriesChart` (cursor por teclado, "Ver tabla", leyenda), con enlace "Ver todos los gráficos" a Monitor.
  3. **Basura**: tabla por categoría del último escaneo (nombre y código, estado con código, elementos, tamaño, omitidos, inaccesibles) con la fecha del escaneo y `cleanedAt`; sin rutas por ítem (están en Limpieza → "Ver rutas").
  4. **Disco**: último análisis (`root.path` completo, fecha, total analizado, espacio no contabilizado, carpetas sin acceso, archivos en la nube), enlace a Espacio en disco.
  5. **Últimas limpiezas**: las 3 más recientes del historial (D-09) en una línea cada una, enlace a Historial.
- B: los 4 gráficos de Monitor en Inicio. Duplica Monitor y cuadruplica el costo de render en la pantalla de arranque.
- C: solo medidores, sin gráfico. Más liviano; contradice "gauges + gráficos" de KB 12.
- No hay health score (C-20); el espacio queda reservado arriba del bloque Sistema sin placeholder visible (sin "próximamente").

### D-08 — Historial: ubicación y forma **[DUEÑO]**

- **A (recomendada)**: pantalla propia **"Historial"** en la navegación de ambos modos, entre Duplicados y Monitor (orden: Inicio, Espacio en disco, Limpieza, Duplicados, Historial, Monitor, Configuración), icono nuevo `history`. Agrupa limpiezas de basura, de duplicados y vaciados de la Papelera, que hoy viven en dos pantallas distintas.
  - **Simple**: lista de limpiezas, la más reciente primero, una tarjeta por limpieza con fecha y una frase ("Limpieza de basura: 340 archivos (1,2 GB) movidos a la Papelera. 5 se omitieron." / "Copias duplicadas: 3 archivos (2 GB) movidos a la Papelera." / "Papelera vaciada: se liberaron 2 GB."); "Ver más" pagina. Sin rutas ni códigos. Sin acción primaria (pantalla de consulta).
  - **Avanzado**: misma lista con conteos por resultado y código; cada limpieza se despliega ("Ver archivos", `aria-expanded`) en una tabla paginada de ítems: fecha y hora, categoría (nombre y código), **ruta completa**, tamaño, resultado (texto y código) y detalle (código estable de KB 11). "Cargar más" de 100 en 100.
  - Estado vacío: "Todavía no limpiaste nada con la app." Base no disponible: mensaje del catálogo por `kind`.
- B: sección "Historial" solo dentro de Limpieza en modo avanzado (literal de KB 12, "Limpieza avanzada: … log"). Deja fuera al modo simple y a los duplicados.
- C: sub-vista de Limpieza (botón "Ver historial") sin entrada de navegación. Exige pantallas no registradas en la navegación (hoy no existen) y otro patrón de foco.

### D-09 — Agrupación en limpiezas e IPC **[DUEÑO]**

- **Limpieza** = conjunto de filas del log con el mismo `scan_id` no nulo; cada fila con `scan_id` nulo es una limpieza propia (vaciado de la Papelera). Tipo derivado en el dominio (`domain/cleanup/history.rs`, función pura): `recycle_bin_empty` (fila sin `scan_id` y categoría `recycle_bin`), `duplicates` (todas las filas `duplicates`), `junk` (todas las filas de categorías de la whitelist), `other` (cualquier otra mezcla; reservado para C-21). Varias limpiezas de duplicados sobre el mismo escaneo se ven como una sola (con la fecha de la última fila): se documenta en el texto ("Limpiezas del análisis de duplicados del {fecha}").
- **A (recomendada)**: dos comandos de solo lectura, con un permiso cada uno (**31** en total):
  - `list_cleanup_runs({ before: { finishedAt, key } | null, limit })` → `{ items: CleanupRun[], nextCursor | null }`. `limit` 1..=50; orden por `finishedAt` (máximo `ts`) desc y desempate por clave. `CleanupRun = { key: { kind: "scan", scanId } | { kind: "entry", entryId }, kind, startedAt, finishedAt, entryCount, outcomes: { movedToTrash, skippedLocked, skippedProtected, skippedCloud, error, recycleBinEmptied }, movedBytes, emptiedBytes, emptyDetail: "partial" | "unverified" | null, categories: string[] }`. **Sin rutas**.
  - `list_cleanup_entries({ key, offset, limit })` → `{ total, pathsHidden, items: [{ id, ts, category, path: string | null, sizeBytes, outcome, errorDetail }] }`. `limit` 1..=200, `offset` ≥ 0; orden `id` asc (estable: el log solo crece al final). Clave inexistente → página vacía con `total = 0`.
  - Argumentos fuera de rango, cursores mal formados o números inseguros → `invalid_argument`; base no disponible → el error de base existente. Sin errores nuevos.
  - La interfaz nunca envía rutas; ningún comando escribe.
- B: un solo comando con filas sueltas paginadas (sin agrupar). Menos backend, pero el modo simple no puede decir "340 archivos en esta limpieza" sin bajar todo el log.
- C: agrupar en el frontend leyendo las últimas N filas. Conteos incorrectos para limpiezas grandes. Rechazada.

### D-10 — Esquema: sin migración ni índice nuevo **[DUEÑO]**

- **A (recomendada)**: sin migración. Las consultas agrupan sobre `cleanup_log` (índice `(ts)` existente); los ítems de una limpieza se filtran por `scan_id` (o por `id`). KB 04 estima volumen bajo (retención indefinida, auditoría). Se agrega un benchmark `#[ignore = "mide rendimiento sobre 100 000 filas en un tempdir"]` que se corre a mano y se anota en Apply notes; contingencia si la página de limpiezas supera 100 ms: volver al dueño con la opción B.
- B: migración 005 con índice `cleanup_log(scan_id, id)`. Consultas por limpieza indexadas (también acelera `scan_cleaned_at`), pero sube la versión de esquema (una versión anterior de la app ya no abre la base) y toca `local-store` para un volumen hoy pequeño.

### D-11 — Rutas del historial en over-the-shoulder **[DUEÑO]**

El log vive en el perfil del token elevado (DD-16); en `mismatch` quien mira es otra cuenta.

- **A (recomendada)**: con contexto `mismatch` (cualquier motivo, incluido "sin determinar"), `list_cleanup_entries` devuelve `path = null` en todas las filas y `pathsHidden = true`; la UI muestra "Ruta oculta: la app corre con otra cuenta" en la columna y un aviso arriba de la tabla. Conteos, tamaños, categorías y códigos se mantienen. La regla vive en el backend (función pura del dominio + test de comando), no en la UI. Fail-closed, coherente con PA-14 y con las carpetas opacas de C-07.
- B: mostrar todas las rutas (el administrador autorizó la elevación). Expone nombres de archivos del perfil del administrador a quien está en la sesión.
- C: ocultar solo las rutas dentro de perfiles de usuario. Más fino, pero requiere clasificar rutas guardadas (que pueden ya no existir) con las reglas de `path-protection`.

### D-12 — Textos

- Catálogo nuevo `es.home.*` (reemplaza `emptyTitle`/`emptyBody`), `es.history.*`, `es.nav.history`. Resultados y códigos de detalle reutilizan los textos de C-09 (`cleanupReasons`) y C-10; categorías con `categoryName` + `duplicates` y `recycle_bin`. Plurales en español (patrón de C-11). Fechas con `formatDateTime`, tamaños con `formatBytes`.
- Lenguaje llano en simple: "movidos a la Papelera" (nunca "borrados"), "se omitieron" con la familia ("en uso o modificados hace poco", "protegidos", "en la nube").

### D-13 — Una acción primaria por pantalla

- Test transversal: para cada pantalla registrada, en modo simple y en avanzado, con el backend simulado en su estado inicial y con resultados, hay **como máximo un** botón `data-variant="primary"` visible, e Inicio tiene exactamente uno. Si una pantalla existente viola la regla, el apply lo reporta (no se corrige sin aprobación, es otra capability).

### D-14 — Estructura de módulos

```
src-tauri/src/
  domain/ports/store.rs          CleanupLogStore: cleanup_runs(before, limit), cleanup_run_entries(key, offset, limit)
  domain/ports/fakes.rs          InMemoryStore: mismas consultas en memoria
  domain/ports/store_contract.rs contratos de agrupación, cursor, paginado (fake y SQLite)
  domain/cleanup/history.rs      RunKey, RunKind (clasificación), cursor, redact_paths(ctx)  + history_tests.rs
  adapters/sqlite/mod.rs         SQL de agrupación y de ítems
  commands/history.rs            list_cleanup_runs, list_cleanup_entries, DTOs, validación  + history_tests.rs
  commands/names.rs, lib.rs, capabilities/main-window.json
src/
  lib/ipc.ts                     tipos, decodificadores, listCleanupRuns, listCleanupEntries
  features/dashboard/            HomeScreen, SpaceCard, JunkCard, MemoryCard, CleanNowAction,
                                 AdvancedSystem, JunkTable, DiskSummaryBlock, RecentCleanups,
                                 cleanNowPlan.ts, cardState.ts, homeText.ts
  features/history/              HistoryScreen, RunList, RunEntries, useCleanupRuns, useRunEntries, historyText.ts
  features/cleanup/              CleanupProvider (cleanNow), CleanupScreen (consumo), CleanupPreviewDialog (foco de respaldo)
  features/monitor/              extraer MetricLine/SpaceLine reutilizables por Inicio (sin cambio de comportamiento)
  app/navigation.ts, app/providers/NavigationProvider.tsx, shared/ui/Icon.tsx, shared/i18n/es.ts
```

### D-15 — Tests

- Rust: contratos del store (agrupación por `scan_id`, filas sin `scan_id` separadas, conteos por resultado, `movedBytes`/`emptiedBytes`, orden y cursor, paginado de ítems, clave inexistente), dominio (clasificación, redacción en `mismatch`/`undetermined`, validación de cursor y límites) y comando (DTO exacto camelCase, `invalid_argument`, base no disponible, `pathsHidden`). Todos sobre `tempdir` o el doble en memoria.
- Vitest: `cleanNowPlan` (tabla de casos), `cardState`, decodificadores, cada tarjeta en sus estados, "Limpiar ahora" de punta a punta con `mockBackend` (preview directo, escaneo y luego preview, sin elegibles, cancelado, salir antes de terminar, foco en "Cancelar" y vuelta a "Limpiar"), deshabilitado con motivo, avanzado (medidores, gráfico de dos series, tabla, disco, últimas limpiezas), Historial (vacío, simple sin rutas, avanzado con rutas y paginado, `pathsHidden`), alternancia de modo en Inicio conservando la pantalla, navegación con 7 entradas, test transversal de D-13, guardia de literales.

### D-16 — Revisión UX (IN-01)

- Revisión con la skill `ux-ui-revisor` (solo lectura) de Inicio e Historial en ambos modos: jerarquía, una sola CTA, lenguaje llano, estados no disponible, foco y teclado (incluido el salto Inicio → diálogo), contraste, ancho mínimo 960 px. Hallazgos reales se corrigen con TDD; el resultado se anota en KB 10 IN-01 como revisión parcial (Inicio/Historial), sin cerrar IN-01 (la revisión completa es tras v0.1).

### D-17 — Smoke guiado (manual, con el dueño) **[DUEÑO]**

Se escribe `smoke-guide.md` en el apply (patrón de C-06). Reglas:

1. Un paso por vez; el dueño confirma antes del siguiente.
2. **Aviso y cuenta atrás real de 10 s** (`for ($i=10; $i -gt 0; $i--) { Write-Host $i; Start-Sleep 1 }`) antes de cada lanzamiento de la UI o de UAC.
3. Sin capturas: verificación por DOM vía CDP solo si el dueño lo delega (texto, roles, atributos, `data-variant`, foco activo) o a ojo por el dueño.
4. Base temporal con `PCOPT_DATA_DIR=%TEMP%\pcopt-smoke-c12\data` (build de desarrollo: el release ignora `PCOPT_DATA_DIR`); terminal NO elevada.
5. Archivos de prueba solo en `%TEMP%\pcopt-smoke-c12\` (p. ej. 5 archivos `.tmp` de 1 MiB con `LastWriteTime` de hace 3 días para que sean candidatos de `temp_user`).
6. **Confirmar una limpieza real solo con autorización explícita en ese paso**: mueve a la Papelera los temporales reales del usuario que el preview liste (reversible desde la Papelera); recomendado dejar marcada solo "Temporales del usuario". Sin autorización, el paso se cierra con "Cancelar" y el historial con datos se cubre con la opción B (un `#[ignore]` que siembra filas sintéticas en la base temporal).
7. Si algo no coincide, se detiene y se anota.

- Pasos: arranque (Inicio simple con Espacio y Memoria con valores, Basura "Todavía no buscaste", una sola acción primaria); Historial vacío; "Limpiar ahora" sin escaneo (navega, busca, abre el preview con foco en "Cancelar"; Esc → foco en "Limpiar", nada cambió); "Limpiar ahora" de nuevo antes de 60 min (preview directo); confirmación con autorización (resumen de C-09; Historial muestra la limpieza en simple sin rutas y en avanzado con rutas de los archivos de prueba); Inicio avanzado (medidores, gráfico de dos series, tabla de basura, disco sin análisis o con un análisis de una carpeta de prueba, últimas limpiezas); deshabilitado con motivo durante una búsqueda de duplicados; alternar modo en Inicio sin perder la pantalla.
- Cierre: el borrado de `%TEMP%\pcopt-smoke-c12\` lo hace el dueño con el comando exacto que entrega el agente (`Remove-Item -LiteralPath … -Recurse -Force`); los archivos de prueba movidos quedan en la Papelera y el dueño decide si restaurarlos o vaciarla.
- B: sin limpieza real; historial solo con filas sembradas. No prueba la cadena completa "Limpiar ahora" → Papelera → Historial.

### D-18 — Puntos de extensión (sin implementarlos)

- C-20: la tarjeta del score entra arriba de las tres tarjetas en simple y arriba de Sistema en avanzado; `cardState` admite una tarjeta más.
- C-27: `list_cleanup_runs`/`list_cleanup_entries` sirven al export; la ofuscación de rutas de reportes es otra regla (no la de D-11).
- C-21: el tipo `other` del historial cubre limpiezas por reglas personalizadas.

## Risks / Trade-offs

- [El diálogo abierto al navegar pierde el foco por el efecto de `NavigationProvider`] → opción `focus: "none"` y test de foco en "Cancelar" al llegar desde Inicio.
- [El pedido "scan → preview" abre un diálogo inesperado] → solo si el usuario sigue en Limpieza; se descarta al salir, al cancelar o con error; el diálogo es la confirmación y no ejecuta nada.
- [Cifras viejas en Basura] → fecha visible del escaneo; "Limpiar ahora" re-escanea si tiene ≥ 60 min.
- [Consultas de agrupación lentas con logs grandes] → benchmark `#[ignore]` anotado, contingencia D-10 B.
- [Historial con rutas de otra cuenta en over-the-shoulder] → ocultamiento en el backend (D-11), fail-closed con `undetermined`.
- [Varias limpiezas de duplicados del mismo escaneo agrupadas en una] → documentado en el texto; un `run_id` necesitaría migración (fuera de alcance).
- [Suscripción al monitor desde la pantalla de arranque] → costo medido en C-06 (0,014 % con pantalla visible); sin re-render con la ventana oculta.
- [Test transversal de una CTA falla en una pantalla existente] → se reporta como hallazgo; no se corrige fuera del alcance sin aprobación.

## Migration Plan

- Sin migración de esquema (D-10 A). El log sigue append-only; no hay escrituras nuevas.
- Capabilities: +2 permisos `allow-list-cleanup-runs`, `allow-list-cleanup-entries` (31); sin `core:*`, sin plugins, sin cambios de CSP ni de `tauri.conf.json`.
- `package-lock.json` y `Cargo.lock` sin cambios.
- Rollback: revertir la rama; no deja datos nuevos.
- Docs en el apply (tarea 1.2 y cierre): `CHANGES.md` (C-12 en curso, decisiones, desvíos, notas para C-20/C-21/C-27), KB 02 (comandos), KB 06 US-001/US-002 (criterios reales), KB 10 IN-01 (revisión parcial), KB 11 §Registro (visible en Historial, rutas ocultas en mismatch), KB 12 (Inicio simple/avanzado, Historial en el mapa de pantallas).

## Decisiones del dueño (bloquean el apply)

| ID | Decisión | Recomendación |
|---|---|---|
| D-01 | Inicio simple | Tres tarjetas (Espacio, Basura, Memoria) con "Ver detalle" y una sola CTA "Limpiar ahora" con su línea de ayuda |
| D-02 | "Limpiar ahora" | Va a Limpieza: preview directo si el último escaneo limpiable tiene < 60 min; si no, busca basura y abre el preview al terminar; nunca ejecuta |
| D-05 | Fuentes de las tarjetas | Espacio y Memoria del monitor en vivo (respaldo: último análisis de disco); Basura del último escaneo |
| D-07 | Inicio avanzado | Medidores + 1 gráfico CPU/memoria + tabla de basura + resumen de disco con ruta + 3 últimas limpiezas |
| D-08 | Historial | Pantalla "Historial" en la navegación (entre Duplicados y Monitor); simple por limpieza sin rutas; avanzado con ítems y rutas completas |
| D-09 | Agrupación e IPC | Limpieza = mismo `scan_id` (o fila sin `scan_id`); `list_cleanup_runs` + `list_cleanup_entries`; 31 permisos |
| D-10 | Esquema | Sin migración ni índice; benchmark `#[ignore]` y contingencia |
| D-11 | Rutas en mismatch | Ocultas en el backend (`path = null`, `pathsHidden`) |
| D-17 | Smoke | Guiado con base temporal; limpieza real solo con autorización y solo "Temporales del usuario"; si no, filas sembradas |

## Desvíos respecto de CHANGES.md / KB

- "Dashboard modo avanzado: sidebar": la sidebar ya existe desde C-04; C-12 no la cambia.
- "Gauges": medidores lineales de C-06 (D-13 de C-06), no gauges circulares.
- KB 12 "Health score 0-100 grande" en Inicio simple: llega en C-20.
- KB 12 "Limpieza | avanzado | log": el log pasa a la pantalla propia Historial en ambos modos (D-08), con rutas solo en avanzado.
- KB 12 "Tarjetas Inicio, Salud del disco": no en v0.1 (C-17, C-14).
- Dos comandos IPC nuevos (`CHANGES.md` no los nombra; C-09 D-24 anticipaba `get_cleanup_log`).

## Preguntas abiertas de KB 10 que toca este change

- **IN-01** (denso vs. común): se mantiene la resolución por modos; C-12 hace la revisión de usabilidad de Inicio e Historial (D-16) y la anota como parcial.
- Ninguna PA de prioridad alta bloquea C-12 (PA-01 resuelta en C-06; PA-03 es de C-19).

## Open Questions

- Ninguna que cambie specs o tareas. El umbral de 60 min de D-02 y el tamaño de página de 100 ítems son constantes ajustables tras el smoke sin tocar specs.

## Apply notes

(Se completa durante el apply: línea base, salidas RED por tarea, benchmark de D-10, revisión UX, conteos finales y resultados del smoke.)

### Decisiones del dueño (2026-11-19)

El dueño aprobó TODAS las decisiones [DUEÑO] con la opción A recomendada: D-01, D-02, D-05, D-07, D-08, D-09, D-10, D-11, D-17. Registradas en `CHANGES.md` (C-12) y KB 02, 06, 11, 12 (tarea 1.2).

### Línea base (tarea 1.1, rama `feat/dashboard-simple-advanced` @ 20b1837)

- `npm test -- --maxWorkers=2`: 680 passed (39 archivos). `npm run typecheck` y `npm run build`: verdes.
- `cargo test --locked`: lib 1169 passed + 23 ignorados; `architecture` 24; `manifest` 5; `no_network_crates` 9; `security_baseline` 22. `cargo fmt --check` y `cargo clippy --all-targets --all-features --locked -D warnings`: verdes.
- Red de los grupos 2-3: `domain::ports` 93 passed; `adapters::sqlite` 107 passed.
- Nota: `cargo build` deja las `permissions/autogenerated/*.toml` modificadas solo por fin de línea; se restauran con `git checkout --` antes de commitear.

### Grupos 2-3 (Rust): persistencia agrupada, dominio y comandos

Comandos: `cargo test --manifest-path src-tauri/Cargo.toml --locked <filtro>`.

- **2.1 RED** (`contract_cleanup_runs`): `error[E0432]: unresolved import crate::domain::ports::store::CleanupRunKey` y `error[E0599]: no method named cleanup_runs found for type parameter S` (`store_contract.rs`). GREEN: tipos `CleanupRunKey`/`RunCursor`/`OutcomeCounts`/`CleanupRunRow` en `domain/ports/store.rs`, `InMemoryStore::cleanup_runs` y SQL de agrupacion en `adapters/sqlite/history.rs`. TRIANGULATE: vaciados sin escaneo separados (`emptied_bytes` 1000/2000), log vacio, categorias sin repetir y ordenadas, `partial` del vaciado conservado, omitidos por familia y error. REFACTOR: la consulta pasa a dos etapas (agrupar solo `min/max(ts)`, paginar, y recien entonces agregar los conteos de la pagina) para bajar el costo (ver benchmark).
- **2.2 (sin RED natural)**: el cursor ya forma parte de la firma de 2.1, por lo que los casos de orden y cursor pasaron al primer intento. Se verifico que el test detecta el defecto con una mutacion: cambiar `<` por `<=` en el cursor SQL hace fallar `sqlite_store_should_satisfy_contract_cleanup_runs` con `left: [Scan(ScanId(4)), Scan(ScanId(3))] right: [Scan(ScanId(3)), Scan(ScanId(2))]`; revertida. Casos: 5 limpiezas en paginas de 2 (2, 2, 1, sin repetidas), misma fecha maxima (orden estable y paginando de a una no se pierde ninguna), `limit` 0 vacio, filas nuevas de un escaneo ya listado lo suben arriba (documentado en `contract_cleanup_runs_new_rows_move_a_run_to_the_top`).
- **2.3 RED** (`contract_cleanup_run_entries`): `error[E0599]: no method named cleanup_run_entries found for type parameter S`. GREEN: `cleanup_run_entries(key, offset, limit) -> (total, filas)` (fake y SQLite, filtro parametrizado). TRIANGULATE: clave `Entry` devuelve solo esa fila y un `Entry` que apunta a una fila de escaneo no cuenta; clave inexistente -> `(0, [])`; filas de otro escaneo nunca aparecen. `contract_log` y `schema_tests` siguen verdes sin cambios (append-only intacto).
- **2.4 Benchmark de D-10** (`cleanup_runs_benchmark_should_page_100_000_rows_quickly`, `#[ignore]`, 100 000 filas = 200 limpiezas de 500 filas, base en `tempdir`, comando `cargo test --locked [--release] cleanup_runs_benchmark -- --ignored --nocapture`):

  | Consulta | Perfil de test (debug) | Release |
  |---|---|---|
  | Primera pagina de limpiezas (50) | 108 ms | 64 ms |
  | Segunda pagina (con cursor) | 108 ms | 63 ms |
  | Pagina de 200 items de una limpieza (500 filas) | 20 ms | 13 ms |

  La primera version de la consulta (una sola agregacion con `group_concat(DISTINCT)`) daba 160 ms en debug / 79 ms en release; la version en dos etapas baja a 108 / 64. **El limite de 100 ms de D-10 se cumple en release pero se supera por 8 ms en el perfil de test (debug), donde SQLite se compila sin optimizar**; con 100 000 filas (volumen muy superior al esperado). El test solo afirma el limite en builds sin `debug_assertions`. Queda anotado para el dueno: opcion A de D-10 vigente salvo que prefiera la B (migracion con indice `(scan_id, id)`).
- **3.1 RED** (`domain::cleanup::history`): `error[E0433]: cannot find type RunKind in this scope` / `CleanupRunKey`. GREEN `RunKind::classify`. TRIANGULATE: solo `duplicates`; `Entry` con `recycle_bin`; whitelist + categoria desconocida -> `Other`; `Scan` con `recycle_bin` -> `Junk`; `duplicates` mezclada con basura y lista vacia -> `Other`; codigos estables.
- **3.2 RED**: `error[E0425]: cannot find function redact_paths in this scope`. GREEN: `redact_paths(ctx, entries) -> (paths_hidden, Vec<HistoryEntry>)`. TRIANGULATE: `mismatch` sin determinar oculta; `match` mantiene las rutas; pagina vacia en `mismatch` informa `paths_hidden`. Conteos, tamanos y codigos intactos.
- **3.3 RED** (`commands::history`): `error[E0425]: cannot find function list_runs in this scope` (y tipos `PrivilegeContext`/`AppError` sin importar). GREEN: DTO camelCase exacto (`key` etiquetada por `kind`, `nextCursor` nulo). Se pide una limpieza de mas para saber si hay otra pagina, de modo que el cursor es nulo exactamente en la ultima. TRIANGULATE: `limit` 0/51/negativo/`i64::MAX` y cursor mal formado (fecha 0, negativa o 2^53, clave de tipo desconocido, sin id) -> `invalid_argument` sin leer el log (se prueba con un store que siempre falla); `offset` negativo o 2^53 y clave desconocida -> `invalid_argument`; pagina con rutas en `match`; `mismatch` (ambos motivos) -> `path: null` y `pathsHidden: true`; base no disponible -> `store_unavailable`; con un `ActiveJob` ocupado por una limpieza responde y el conteo del log no cambia.
- **3.4 RED**: `security_baseline` -> `assertion left == right failed: allow-list-cleanup-runs  left: 0  right: 1`; test IPC de `lib.rs` -> `Command list_cleanup_runs not found`. GREEN: `#[tauri::command]` async con `run_blocking`, `APP_COMMANDS`, `invoke_handler`, `allow-list-cleanup-runs` y `allow-list-cleanup-entries` (31 permisos `allow-*`, 0 `core:*`, 0 plugins); el test que contaba 29 pasa a 31. `src-tauri/tauri.conf.json` y `Cargo.lock` sin cambios; clippy `-D warnings` verde. Se agregan los `permissions/autogenerated/list_cleanup_{runs,entries}.toml` que genera `build.rs`.
- Decision de implementacion: `RunKeyArg { kind, scanId?, entryId? }` (en lugar de un enum etiquetado) para que una clave de tipo desconocido llegue como `invalid_argument` y no como error de deserializacion de Tauri; la respuesta usa `RunKeyDto` etiquetado por `kind` con la misma forma, de modo que la clave y el cursor de una respuesta se reenvian tal cual.
- Los dobles de `CleanupLogStore` de `recycle_bin_tests.rs` y `execute_tests.rs` ganaron los dos metodos nuevos (cambio mecanico de trait).

### Grupo 4 (Frontend): IPC, catalogo y textos

Comandos: `npx vitest run <ruta> --maxWorkers=2`.

- **4.1 RED** (`src/tests/history/ipc.test.ts`): `TypeError: listCleanupRuns is not a function` al decodificar una pagina valida con un vaciado `emptyDetail: "partial"` y un cursor. GREEN: tipos `CleanupRunKey`, `CleanupRun`, `CleanupRunPage`, `CleanupEntry`, `CleanupEntryPage`, decodificadores estrictos y `listCleanupRuns(before, limit)` / `listCleanupEntries(key, offset, limit)` en `src/lib/ipc.ts`. TRIANGULATE (12 tests): `kind` de limpieza desconocido, `kind` de clave desconocido (en la limpieza y en el cursor), numero inseguro/negativo/fraccionario, campo faltante, `emptyDetail` desconocido, respuestas que no son una pagina, resultado de item desconocido, ruta que no es texto, `pathsHidden` ausente y total inseguro -> `invalid_response`; `path: null` aceptado; campos de mas ignorados; los argumentos viajan tal cual (`before: null`, clave, desplazamiento y limite).
- **4.1 mock (RED)** (`src/tests/support/mockBackendHistory.test.ts`): `Unknown Error: unknown command list_cleanup_runs` (5 tests fallidos). GREEN: el estado `history: HistoryFixture[]` y los dos comandos en `mockBackend.ts` (orden total por fecha, tipo de clave e id; cursor exclusivo; rutas nulas y `pathsHidden` con `privilege.status === "mismatch"`). Fixtures en `src/tests/support/historyFixtures.ts` (`junkRun`, `duplicatesRun`, `emptyBinRun`, `historyEntry(ies)`, `sampleHistory`). TRIANGULATE: pagina con cursor sin repetir (2, 1), historial vacio, 250 items paginados (200 + 50), `mismatch`, clave inexistente.
- **4.2 RED** (`src/tests/i18n/historyText.test.ts`): `Failed to resolve import "../../features/history/historyText"`. GREEN: `runPhrase` (plural/singular, omitidos por familia), `runTitle`, `outcomeText`, `entryCategoryText`, `entryDetailText` en `src/features/history/historyText.ts` y el catalogo `es.history.*` + `es.nav.history`. TRIANGULATE (12 tests): singular con 1 movido y 1 omitido; sin omitidos; archivos en la nube sin familias en cero; cero movidos; duplicados; tipo `other` con frase generica (nunca el codigo); vaciado `partial`/completo/`unverified`/fallido; resultado, categoria y detalle desconocidos con respaldo (incluida una clave del prototipo como `toString`).
- **4.2 RED** (`src/tests/i18n/homeText.test.ts`): `Failed to resolve import "../../features/dashboard/homeText"`. GREEN: `busyReasonText`, `binNoteText`, `codeText` en `homeText.ts` y el catalogo `es.home.*` (tarjetas, motivos de deshabilitado, bloques avanzados). Icono `history` en `Icon.tsx`. La guardia `src/tests/uiLiterals.test.ts` sigue verde.
- Desvio menor: `es.home.emptyTitle`/`emptyBody` se conservan (con un comentario) hasta la tarea 6.3, porque el Inicio provisional y `App.test.tsx` todavia los usan; se retiran con el `HomeScreen` real.

### Conteos al cerrar la Parte 1 (grupos 1-4) frente a la linea base

| Verificacion | Linea base | Parte 1 |
|---|---|---|
| `cargo test` lib | 1169 passed, 23 ignorados | 1196 passed, 24 ignorados (+27 tests, +1 benchmark) |
| `architecture` / `manifest` / `no_network_crates` / `security_baseline` | 24 / 5 / 9 / 22 | 24 / 5 / 9 / 22 (31 permisos `allow-*`) |
| `domain::ports` / `adapters::sqlite` | 93 / 107 | 95 / 109 |
| Vitest | 680 (39 archivos) | 713 (43 archivos) |
| `typecheck`, `build`, `cargo fmt --check`, `clippy -D warnings` | verdes | verdes |
| `package-lock.json`, `Cargo.lock`, `tauri.conf.json` | - | sin cambios |

### Seguimiento de D-10 (2026-11-19)

Con el benchmark de 2.4 (64 ms en release / 108 ms en debug por página de limpiezas sobre 100 000 filas) el dueño decidió **mantener D-10 A (sin migración ni índice)**.

### Grupos 5-9 (Frontend): navegación, pedido de preview, Inicio, Historial y revisión UX

Comandos: `npx vitest run <ruta> --maxWorkers=2`. Los RED de "pantalla nueva" fallan por el símbolo o el texto inexistente.

- **5.1 RED** (`src/tests/history/navigation.test.tsx`): `AssertionError: expected [ 'home', 'disk', 'cleanup', ...(3) ] to deeply equal [ 'home', 'disk', 'cleanup', ...(4) ]` (6 entradas en lugar de 7). GREEN: entrada `history` en `navigation.ts` con un `HistoryScreen` mínimo (reemplazado en 8.1). Los conteos de `providers`, `shell`, `disk/navigation`, `monitor/navigation` y `duplicatesScreen` se actualizaron mecánicamente a 7 entradas.
- **5.2 RED** (`src/tests/app/navigationFocus.test.tsx`): `expect(element).not.toHaveFocus()` con `focus: "none"` (el proveedor igual enfocaba `<main>`). GREEN: `navigate(id, { focus })`. TRIANGULATE: sin opciones y `focus: "main"` siguen enfocando `<main>`.
- **5.3 RED** (`src/tests/dashboard/cleanNowPlan.test.ts`): `Failed to resolve import "../../features/dashboard/cleanNowPlan"`. GREEN: `cleanNowPlan` y `hasEligibleItems` (CleanupScreen reutiliza `hasEligibleItems`). TRIANGULATE: sin escaneo, cancelado, `cleanedAt`, bloqueadas/no disponibles/0 ítems/solo Papelera, 59 min 59 s -> preview, 60 min y 2 h -> scan_then_preview, `startedAt` cuando `finishedAt` es nulo.
- **5.4 RED** (`src/tests/dashboard/cleanNowRequest.test.tsx`): `TypeError: useCleanNow is not a function` (8 tests). GREEN: `useCleanNowRequest.ts` (pedido de sesión `request/consume/drop`) y `useCleanNow` en `CleanupProvider`; `CleanupScreen` consume el pedido al montar (`preview` -> `cleaning.open()`; `scan_then_preview` -> `start()` y, al terminar con un escaneo nuevo `done` y elegibles, abre el preview; sin elegibles anuncia `es.cleanup.cleanNowNothing`); al desmontar descarta el pedido y la espera; `CleanupPreviewDialog.fallbackFocus` devuelve el foco a "Limpiar" o al `h1` (ahora con `tabIndex=-1`) si el elemento previo ya no existe. TRIANGULATE: Esc -> foco en "Limpiar" y 0 `confirm_cleanup`; sin botón Limpiar -> foco en el encabezado; sin elegibles, cancelada y con error -> sin diálogo; salir antes de terminar -> sin diálogo al volver. Los tests de `src/tests/cleanup` pasan sin cambios.
- **6.1 RED** (`cardState.test.ts`): `Failed to resolve import "../../features/dashboard/cardState"`. GREEN: `spaceCardState`, `memoryCardState`, `junkCardState` puros. TRIANGULATE (17 tests): respaldo de disco con fecha y sin nivel, sin ambos -> no disponible con código, análisis cancelado o sin volumen no sirve, cargando, `stale`, `warming_up`, `source_error`, sin escaneo, `store_unavailable`, ya limpiado, Papelera aparte, `old_logs` no cuenta como limpiable.
- **6.2**: `Missing`, `MetricLine` y `SpaceLine` pasan de `MonitorTiles.tsx` a `MetricLines.tsx` sin cambio de comportamiento; `src/tests/monitor` 47 tests antes y después. Inicio reutiliza `LevelBadge`, `levelPhrase`, `levelText`, `spaceLevelText` y `MeterList`.
- **6.3 RED** (`homeSimple.test.tsx`, escrito antes del `HomeScreen`): 15 de 15 fallan (`Unable to find role="region" name "Espacio"`), incluida la parte "Inicio exactamente un primario" de 9.1 (el Inicio vacío no tenía ninguno). GREEN: `HomeScreen`, `HomeCard`, `SpaceCard`, `JunkCard`, `MemoryCard`, `CleanNowAction`. Se retiran `es.home.emptyTitle/emptyBody` y se actualiza `App.test.tsx`. Lección: la fixture del monitor necesita bytes enteros (un decimal hace que el decodificador devuelva `invalid_response`).
- **6.4 RED** (`homeBusy.test.tsx`): `Failed to resolve import "../../features/dashboard/homeBusy"`. GREEN: `busyTask` puro y `CleanNowAction` deshabilitado con motivo (`aria-describedby` = ayuda + motivo). TRIANGULATE: tabla de los seis trabajos y su orden, búsqueda de duplicados, búsqueda de basura (se rehabilita al terminar), base no disponible, `mismatch` habilitado. Limpieza, vaciado, análisis de disco y limpieza de duplicados se cubren con la tabla de `busyTask` (los flags vienen de los proveedores).
- **7.1/7.2/7.3 RED** (`homeAdvanced.test.tsx`, 16 de 19 fallan antes del código): `Unable to find role="list" name "Procesador y memoria..."`, `Unable to find an element with the text ...`. GREEN: `AdvancedSystem` (`MeterList` + `TimeSeriesChart` de 2 series con los tonos `primary`/`secondary` ya validados en C-06 con `--color-series-2`, leyenda, eje de 5 min, sin `style`; skill `dataviz` aplicada reutilizando el componente validado), `JunkTable`, `DiskSummaryBlock`, `RecentCleanups` (`useCleanupRuns(3)`). En el test: una categoría `partial` exige `blockReason` y `scope: per_user`, y "Basura" es el nombre de región tanto de la tarjeta simple como de la tabla avanzada, así que la alternancia se verifica con "Sistema".
- **8.1/8.2 RED** (`historyScreen.test.tsx`, 11 de 11 fallan con el `HistoryScreen` mínimo): `Unable to find an element with the text: Todavía no limpiaste nada...`, `Unable to find role="button" and name "Ver archivos"`. GREEN: `useCleanupRuns` (páginas de 20), `useRunEntries` (100 por página), `RunList`, `RunEntries`, `HistoryScreen`. Se corrige la fixture `historyEntry` (la ruta tenía escapes `\U`, `\a` interpretados) y su aserto en `history/ipc.test.ts`.
- **9.1** (`src/tests/app/primaryAction.test.tsx`, 8 tests): todas las pantallas, simple y avanzado, backend vacío y con resultados, a lo sumo un primario visible; Inicio exactamente uno ("Limpiar ahora"). Sin RED propio por ser test de regresión (la parte de Inicio ya había fallado en 6.3). Ninguna pantalla existente viola la regla: no hubo que detenerse.

### Revisión UX (tarea 9.2, skill `ux-ui-revisor`, solo lectura)

Limitación: la auditoría se hizo sobre el código, el catálogo y los tests (no se lanzó la app real; el smoke guiado del grupo 10 cubre lo visual). Resumen: Inicio e Historial tienen jerarquía clara, una sola CTA en Inicio y ninguna en Historial, estados no disponibles explícitos y teclado completo; sin hallazgos críticos.

Corregidos (RED anotado):
1. **Alta - foco perdido al pulsar "Limpiar ahora" con búsqueda previa** (`CleanupScreen`): el botón de Inicio se desmonta y la navegación no mueve el foco, así que el teclado quedaba en `<body>` durante toda la búsqueda (WCAG 2.4.3). RED: `expect(heading "Limpieza").toHaveFocus()` fallaba en `cleanNowRequest.test.tsx`. Fix: tras iniciar la búsqueda el foco va al `h1` de Limpieza.
2. **Media - varios "Ver archivos" idénticos en el Historial avanzado** (`RunList`): el nombre accesible no distingue la limpieza (WCAG 2.4.6). RED: `toHaveAccessibleDescription(fecha + frase)` fallaba en `historyScreen.test.tsx`. Fix: `aria-describedby` apunta a la fecha y la frase de cada limpieza.

Diferidos:
3. Media: en avanzado, "Sin datos recientes" solo aparece en el bloque Sistema; `MeterList` no lo marca (compartido con Monitor de C-06; conviene tratarlo en un change de monitor).
4. Baja: la tabla de basura de Inicio avanzado tiene 6 columnas con códigos; a 960 px depende de `overflow-x-auto`. Verificar a ojo en el smoke.
5. Baja: "Ver detalle" es un botón secundario (no un enlace); coherente con el resto de la app, pero compite visualmente con "Limpiar ahora" en anchos angostos. Revisar en el smoke.
6. Baja: la tarjeta Espacio con respaldo del análisis de disco no muestra nivel "casi lleno" (D-05, a propósito).

Descartados por ser gusto: tamaño del texto de las cifras y orden de las tarjetas (fijado por D-01).

### Conteos al cerrar la Parte 2 (grupos 5-9)

- Vitest: 819 tests en 53 archivos (Parte 1: 713 en 43); `typecheck` y `build` verdes; `package-lock.json` sin cambios. Sin cambios en Rust en esta parte. Un test de `settings.test.tsx` (flujo de recuperación) falló una vez con todos los hilos y pasa solo y con `--maxWorkers=2` (flaky conocido por carga).

### Fixes de la revisión independiente (3 hallazgos Low, aprobados por el dueño; D-10 sin cambios)

Comando: `npx vitest run src/tests/dashboard/cleanNowRequest.test.tsx src/tests/history/staleResponses.test.tsx --maxWorkers=2`.

1. **StrictMode perdía el preview de `scan_then_preview`** (`CleanupScreen`, efecto "Pedido Limpiar ahora"). Causa: el efecto de montaje corre dos veces en desarrollo; la limpieza ponía `awaitingScan.current = null` y la segunda pasada ya no recuperaba el pedido (`consume()` devolvía `null`). RED (`cleanNowRequest.test.tsx`, envuelto en `<StrictMode>`): `TestingLibraryElementError: Unable to find role="alertdialog" and name "Revisá antes de limpiar"` (1 failed de 13). Fix: la limpieza solo hace `cleanNow.drop()`; la espera vive en un ref de la instancia y se pierde sola al desmontar (salir de Limpieza sigue descartándola). TRIANGULATE: plan `preview` con StrictMode (dialogo abierto), y salir de Limpieza con StrictMode antes de terminar (sin dialogo al volver, "Limpiar" disponible); nunca se invoca `confirm_cleanup`.
2. **`awaitingScan` podía quedar colgado** si el inicio fallaba sin que se viera la busqueda en curso. Fix: efecto que descarta la espera cuando `useJunkScan` informa un `error`. RED: **no se pudo obtener** (el test "inicio falla de inmediato y una busqueda manual posterior con elegibles no abre el preview" pasa ya con el codigo anterior, porque en el mock React llega a renderizar el estado en curso antes del rechazo y la transicion true->false descartaba la espera). Queda como test de caracterizacion y la regla por `error` como defensa para el camino sin transicion (rechazo antes del primer render); se reporta al orquestador como hallazgo sin RED.
3. **`active` compartido en `useRunEntries`/`useCleanupRuns`**: una respuesta de una clave/tamano anterior podia aplicarse despues de re-ejecutar el efecto. RED (`staleResponses.test.tsx`): `AssertionError: expected [ 1, 2, 3 ] to deeply equal [ 100, 101 ]` (entradas) y `expected [ { kind: 'scan', scanId: 1 }, …(2) ] to deeply equal [ { kind: 'scan', scanId: 7 } ]` (limpiezas). Fix: token de generacion `genRef` (el efecto y su limpieza lo incrementan; cada fetch captura `gen` y solo aplica/limpia `inFlight` si `gen === genRef.current`). TRIANGULATE: primera pagina y "Cargar mas" sin cambio de clave siguen concatenando. Nota: el segundo test de entradas fallo inicialmente por una clave nueva en cada render (bucle del efecto), problema del test, no del hook; se uso una clave estable.
- Verificación: `src/tests/{dashboard,history,cleanup}` 208 passed; suite completa `--maxWorkers=2` 827 passed (54 archivos).

### Conteos finales (10.1) frente a la línea base de 1.1

| Comando | Línea base (1.1) | Final |
|---|---|---|
| `npx vitest run --maxWorkers=2` | 680 passed (39 archivos) | **827 passed** en 54 archivos (+147); `npm test` con todos los hilos conserva el flaky de carga conocido de `settings.test.tsx` |
| `npm run typecheck` y `npm run build` | verdes | verdes (JS 406,8 kB, gzip 115,9 kB) |
| `cargo test --locked` lib | 1169 passed, 23 ignorados | **1198 passed**, 25 ignorados (+29 tests; +2 ignorados: benchmark de D-10 y `seed_smoke_database`) |
| `architecture` / `manifest` / `no_network_crates` / `security_baseline` | 24 / 5 / 9 / 22 | 24 / 5 / 9 / 22 |
| `cargo fmt --check` y clippy `--all-targets --all-features --locked -D warnings` | verdes | verdes |
| Permisos `allow-*` autogenerados | 31 | 31 |
| `package-lock.json`, `Cargo.lock` | - | sin cambios frente a `main` |
| `openspec validate dashboard-simple-advanced --strict` | - | válido |

Cada tarea de comportamiento de los grupos 1 a 9 tiene su salida RED anotada (Partes 1 y 2 y fixes de la revisión independiente; 9.1 es regresión sin RED propio, ver arriba). Excepción declarada: el hallazgo 2 de la revisión independiente no tiene RED (el test pasaba ya con el código anterior). La siembra del smoke (`smoke_seed`, solo `#[cfg(test)]`) se escribió junto a su guardia de ruta (2 tests no ignorados) y no toca código de producción.

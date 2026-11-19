# Proposal

## Why

US-001 y US-002 (v0.1, gobernanza LOW): la pantalla **Inicio** sigue siendo el estado vacío de C-04 ("Todavía no hay nada para mostrar") aunque ya existen el monitor en vivo (C-06), el análisis de disco (C-07), la detección de basura (C-08) y la limpieza a la Papelera (C-09). Un usuario común abre la app y no ve el estado de su equipo ni qué hacer; un usuario avanzado no tiene un tablero denso. Además, el registro `cleanup_log` se escribe desde C-09/C-10/C-11 pero **no se puede ver**: C-09 dejó el historial pendiente para C-12 (D-24). C-12 cierra la UI de v0.1 antes del empaquetado (C-13) y es requisito de C-20 (health score).

## What Changes

- **Inicio en modo simple**: tres tarjetas grandes en lenguaje llano desde el catálogo — **Espacio** (libre de total del disco del sistema, "casi lleno"), **Basura** (cuánto se puede limpiar según el último escaneo y cuándo se hizo) y **Memoria** (uso y nivel en palabras) — y **una sola acción primaria: "Limpiar ahora"**. Cada tarjeta tiene un enlace secundario "Ver detalle" a su pantalla.
- **"Limpiar ahora" nunca borra directo**: lleva a Limpieza y abre el **preview de C-09** (que es la confirmación; todo va a la Papelera). Si no hay un escaneo de basura reciente y limpiable, primero corre uno y abre el preview al terminar. Deshabilitado, con el motivo en texto, mientras hay otro trabajo en curso o la base no está disponible.
- **Inicio en modo avanzado** (misma pantalla, densidad compacta, sidebar existente de C-04): medidores de C-06 (CPU, memoria, tiempo activo, espacio con actual / promedio 60 s / pico), un gráfico en vivo de 5 min de CPU y memoria, tabla de basura por categoría con códigos y conteos, resumen del último análisis de disco con su raíz (ruta completa) y las últimas limpiezas. La misma acción primaria.
- **Estados "no disponible" explícitos** en cada tarjeta y bloque (monitor caído, métrica sin fuente, base no disponible, "Midiendo…"), nunca un 0 inventado; "todavía no analizaste" se distingue de "no disponible".
- **Historial de limpieza** (pantalla nueva "Historial" en la navegación): lista de limpiezas pasadas agrupadas (basura, duplicados, vaciado de la Papelera) con fecha, movidos, omitidos, errores y tamaño en lenguaje llano; en modo avanzado cada limpieza se despliega en sus ítems con **ruta completa**, tamaño, resultado y código de detalle. En over-the-shoulder (`mismatch`) las rutas se ocultan.
- **Dos comandos IPC de solo lectura** (`list_cleanup_runs`, `list_cleanup_entries`) con un permiso `allow-*` cada uno (31 en total), sin plugins, sin `core:*`, sin cambios de CSP, sin migraciones y sin dependencias nuevas.
- Reutiliza `useMonitor`, `Meter`/`MeterList`, `TimeSeriesChart`, `JunkScanProvider`, `CleanupProvider`, `DiskProvider` (`get_last_folder_scan`) y el flujo de preview de C-09.
- Decisiones que requieren al dueño **[DUEÑO]** en `design.md` (comportamiento de "Limpiar ahora", fuentes de las tarjetas, contenido avanzado, ubicación del historial, agrupación e IPC, rutas en mismatch, índice/migración, smoke).

Fuera de alcance: health score y tarjetas Inicio de Windows / Salud del disco (C-20, C-17, C-14); vaciar la Papelera desde Inicio (sigue solo en Limpieza con doble confirmación); purga o exportación del historial (C-27); procesos; historial de métricas (C-15); cambios en la ejecución de la limpieza.

## Capabilities

### New Capabilities
- `dashboard`: pantalla Inicio en ambos modos (tarjetas Espacio/Basura/Memoria, acción primaria "Limpiar ahora" encaminada al preview, bloques avanzados con métricas crudas, rutas y detalle, estados explícitos de no disponible).
- `cleanup-history`: consulta y presentación del registro de limpieza (agrupación en limpiezas, ítems paginados, rutas solo en avanzado y ocultas en mismatch, comandos IPC de solo lectura, pantalla Historial).

### Modified Capabilities
- `app-shell`: lista exacta de comandos habilitados (+2) y navegación con la pantalla Historial.
- `local-store`: lectura agrupada y paginada de `cleanup_log` (sin cambios de esquema; el log sigue append-only).

## Impact

- **Rust**: `domain/ports/store.rs` (`CleanupLogStore`: limpiezas agrupadas e ítems de una limpieza) con su doble en `fakes.rs` y contrato en `store_contract.rs`; `adapters/sqlite/mod.rs` (consultas); nuevo `domain/cleanup/history.rs` (clasificación de la limpieza, cursor, ocultamiento de rutas); nuevo `commands/history.rs` (2 comandos, DTOs, tests); `commands/names.rs`, `lib.rs`, `capabilities/main-window.json` (31 permisos).
- **Frontend**: `src/features/dashboard/` (reemplaza el estado vacío de `HomeScreen`), nuevo `src/features/history/`, `src/lib/ipc.ts` (tipos y decodificadores), `src/app/navigation.ts` y `NavigationProvider` (navegar sin mover el foco cuando se abre el preview), `CleanupProvider`/`CleanupScreen` (pedido de preview desde Inicio), `src/shared/i18n/es.ts`, `src/tests/support/mockBackend.ts` y tests.
- **Dependencias**: ninguna nueva; `package-lock.json` y `Cargo.lock` sin cambios. **Esquema**: sin migración (ver D-10).
- **Docs** (en el apply): `CHANGES.md` (C-12 y nota para C-20/C-27), KB 02 (comandos), KB 06 US-001/US-002, KB 10 (IN-01: revisión UX hecha sobre Inicio), KB 11 §Registro (visibilidad en la UI y mismatch), KB 12 (Inicio e Historial).

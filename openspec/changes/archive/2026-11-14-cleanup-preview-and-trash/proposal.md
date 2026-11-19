# Proposal

## Why

C-08 detecta y mide la basura, pero la app todavía no libera nada: US-009 (limpieza con preview y log) es el corazón de v0.1 y el siguiente paso del camino crítico (`C-08 → C-09 → C-10/C-11 → C-13`). Es el **primer change que mueve archivos del usuario** (a la Papelera): por eso concentra el contrato anti-TOCTOU que C-03 dejó preparado (`prepare_delete`), la re-evaluación del contexto de privilegios de C-05 al confirmar y el hallazgo del smoke de C-08 (las fechas del listado de carpeta llegan tarde en archivos abiertos: hay que re-verificar contra el archivo abierto). C-10, C-11, C-18 y C-21 reutilizan este flujo.

## What Changes

- **Dominio de limpieza** (`domain/cleanup/`, puro): `preview_cleanup` arma, solo con datos persistidos (sin tocar el disco), el resumen de los hallazgos elegibles del último escaneo `done` para las categorías elegidas (cantidad y tamaño por categoría y totales) y lo guarda en memoria con un `preview_id` de un solo uso que vence a los 10 minutos; `confirm_cleanup(preview_id)` ejecuta **exactamente** ese conjunto.
- **Re-verificación por ítem justo antes de mover**: re-evaluación del alcance por-usuario (`scope_access`) y del contexto de protección restringido (`restrict_protection_context`) resuelto de nuevo al confirmar; `prepare_delete` de C-03 (ancestros fijados, verificación completa, identidad `volume_serial`+`file_id` y tipo contra lo persistido, symlink/junction/hard link/nube); tamaño y última escritura leídos **del archivo abierto** comparados con lo persistido; guarda de "escrito en las últimas 24 h" (en uso probable, incluida una descarga de Windows Update en curso).
- **Papelera**: adaptador sobre el crate `trash` ya fijado (un ítem por llamada para tener el resultado de cada uno), que solo acepta un objetivo verificado (`DeleteTarget`) y nunca borra de forma permanente; outcomes `moved_to_trash`, `skipped_locked`, `skipped_protected`, `skipped_cloud`, `error` con un código estable de detalle; un error no detiene el resto, salvo Papelera no disponible o log inaccesible, que abortan la ejecución sin seguir moviendo.
- **Registro**: una entrada append-only en `cleanup_log` por ítem procesado, escrita apenas se conoce su resultado; resumen final (movido a la Papelera, omitidos por motivo, errores, no procesados). Sin migración nueva (los outcomes y `error_detail` ya existen en 001).
- **Progreso y cancelación** por `Channel` de la invocación; cancelar detiene antes del próximo ítem (lo ya movido queda en la Papelera y registrado). Un escaneo y una limpieza nunca corren a la vez.
- **IPC**: `preview_cleanup`, `confirm_cleanup`, `cancel_cleanup`; `get_last_junk_scan` informa si ese escaneo ya se limpió (un escaneo limpiado exige volver a escanear). El frontend solo envía categorías (enum del backend) y el `preview_id`, nunca rutas.
- **UI**: botón "Limpiar" en la pantalla Limpieza; el preview es un diálogo modal con categorías seleccionables, cantidad y tamaño, y foco inicial en "Cancelar"; cerrarlo no ejecuta nada; progreso y resumen accesibles; modo avanzado con detalle por motivo y categoría.
- Decisiones que requieren aprobación del dueño **[DUEÑO]** en `design.md` (backend de Papelera, guarda de 24 h, mapeo de outcomes sin migración, modelo de preview, selección por categoría, escaneo "consumido", UI del preview, verificación empírica de "nunca permanente").

Fuera de alcance: vaciar la Papelera (C-10: única acción permanente); duplicados (C-11) y reglas personalizadas (C-21), que reutilizarán este flujo; selección ítem por ítem; borrar carpetas que queden vacías; matar procesos o reintentar al reiniciar (RN-LIM-05); detener servicios de Windows Update; visor del historial de `cleanup_log` (D-24); cambios en la whitelist o en las excepciones de `path-protection`.

## Capabilities

### New Capabilities
- `cleanup`: limpieza de hallazgos verificados: preview en memoria de un solo uso, confirmación explícita, re-verificación por ítem (privilegios, protección, identidad, cambios, uso reciente), movimiento a la Papelera sin borrado permanente, outcomes y registro append-only por ítem, progreso/cancelación, exclusión mutua con el escaneo, comandos IPC y diálogo de preview/resumen en ambos modos.

### Modified Capabilities
- `junk-detection`: la pantalla Limpieza pasa a ofrecer "Limpiar"; un escaneo no puede iniciarse durante una limpieza; el resumen del último escaneo informa si ya se limpió.
- `path-protection`: la re-verificación anti-TOCTOU entrega también la última escritura leída del archivo abierto (además de tamaño, tipo e identidad).
- `domain-ports`: el puerto de Papelera solo acepta un objetivo re-verificado y tipifica "en uso" y "Papelera no disponible"; el doble permite simular reemplazos durante el movimiento.
- `local-store`: consulta de candidatos de limpieza (elegibles por categoría del último escaneo) y fecha de limpieza de un escaneo derivada del log.
- `app-shell`: lista exacta de comandos IPC habilitados (3 nuevos).

## Impact

- **Rust**: nuevo `src-tauri/src/domain/cleanup/` (+ `*_tests.rs`); `domain/protection/{decision,service}.rs` (`CheckedPath.modified`); `domain/ports/{trash,store,fakes,store_contract}.rs`; `adapters/sqlite/` (consultas nuevas, sin migración); nuevo `adapters/windows/trash.rs` (crate `trash`, hilo dedicado, mapeo de errores); `commands/cleanup.rs`, `commands/scan.rs` (exclusión mutua, `cleanedAt`), `commands/names.rs`, `lib.rs`, `error.rs` (kinds nuevos); `tests/architecture.rs` (ninguna ruta de producción con borrado permanente).
- **Capabilities**: `capabilities/main-window.json` + `allow-preview-cleanup`, `allow-confirm-cleanup`, `allow-cancel-cleanup`. Sin `core:*`, sin plugins, sin cambios de CSP.
- **Frontend**: `src/lib/ipc.ts` (tipos, decodificadores, `Channel`), `src/features/cleanup/` (diálogo de preview, progreso, resumen, estado), `src/shared/i18n/{es,errors}.ts`, `src/tests/support/mockBackend.ts` y tests.
- **Dependencias**: ninguna nueva. `trash = "=5.3.0"` ya está fijado (deja de estar sin uso). Lockfiles sin cambios.
- **Docs** (en el apply): `CHANGES.md` (C-09, notas para C-10/C-11/C-21), KB 11 (re-verificación por handle, guarda de 24 h, códigos de detalle), KB 04 (`error_detail` con código estable), KB 07 Flujo 2 (aborto por Papelera no disponible).

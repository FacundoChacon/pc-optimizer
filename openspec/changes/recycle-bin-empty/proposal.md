# Proposal

## Why

C-09 mueve la basura a la Papelera, pero mover **no libera espacio** (DD-05): el resumen de la limpieza termina diciendo "El espacio se libera cuando vacíes la Papelera", y hoy la app no ofrece hacerlo. US-010 cierra ese hueco con la **única acción permanente declarada** del producto (RN-LIM-06, IN-02 y PA-02 resueltas por el dueño: sí, con doble confirmación propia, nunca automática). Es co-final de v0.1 (`C-09 → C-10 / C-11 → C-13`) y, por ser irreversible, exige que la doble confirmación, el bloqueo en mismatch y la exclusividad la garantice el **backend**, no solo la interfaz.

## What Changes

- **Flujo de vaciado en tres pasos con estado en el backend** (dominio puro, type-state): `prepare_empty_recycle_bin` mide en vivo la Papelera del usuario en el volumen del sistema y emite una solicitud de un solo uso ligada a esa cantidad y tamaño, con vencimiento corto; `acknowledge_empty_recycle_bin` registra la **primera** confirmación y entrega un token de segundo paso; `empty_recycle_bin` (la **segunda** confirmación) solo ejecuta con la solicitud reconocida y su token, vuelve a medir y se niega si la Papelera cambió. Una llamada suelta, fuera de orden, repetida, vencida o con otro token falla sin tocar nada.
- **Solo el volumen del sistema** (PA-12 y nota de C-08): la misma raíz que mide `RecycleBinInfo` (la unidad de `windows_dir`), nunca "todas las Papeleras" (la API las vaciaría todas con una raíz nula, que el tipo del puerto hace imposible).
- **Bloqueo en mismatch** (over-the-shoulder o usuario indeterminado): la Papelera es por-usuario; preparar, reconocer y ejecutar fallan con un error propio y la interfaz explica por qué, sin ofrecer la acción.
- **Exclusividad**: el vaciado ocupa el mismo lugar de trabajo que el escaneo y la limpieza (`ActiveJob`); ninguno de los tres corre a la vez (vaciar mientras la limpieza mueve archivos a la Papelera los destruiría).
- **Adaptador de Windows** `SHEmptyRecycleBinW(raíz, SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND)` en un hilo propio con COM inicializado, aislado de los hilos del pool; nuevo puerto de dominio separado de `Trash` y de `RecycleBinInfo` (que siguen sin operaciones permanentes).
- **Resultado y registro**: el resultado se decide por la medición antes y después (vaciada, parcial, fallida), y se escribe **una** entrada append-only en `cleanup_log` con outcome `recycle_bin_emptied` (o `error`), categoría `recycle_bin`, tamaño liberado y código de detalle estable, **sin `scan_id`** (no marca el escaneo de basura como limpiado). Sin migración: el outcome ya existe en el `CHECK` de 001.
- **Única excepción auditada**: el test de arquitectura de C-09 pasa a permitir `SHEmptyRecycleBin` en **un solo archivo** de producción y verifica que el puerto de vaciado solo se use desde el comando IPC y su flujo; cualquier otro camino (escaneos programados, mejoras, bandeja) no compila o rompe el test.
- **UI**: acción "Vaciar Papelera…" en la tarjeta de la Papelera (y en el resumen de la limpieza, junto a la nota de espacio); dos diálogos **distintos**: paso 1 con cantidad y tamaño a liberar; paso 2 con aviso de irreversibilidad, casilla "Entiendo que no se puede deshacer" y botón destructivo; foco inicial en "Cancelar" en ambos; resultado accesible. Textos en el catálogo `es`.
- **IPC**: 3 comandos nuevos y sus 3 permisos `allow-*` explícitos; sin `core:*`, plugins ni cambios de CSP.
- Decisiones que requieren aprobación del dueño **[DUEÑO]** en `design.md` (protocolo de doble confirmación, vencimiento, igualdad estricta del tamaño, contenido del log, punto de entrada en la UI, segundo paso con casilla, flags del shell, plan de smoke).

Fuera de alcance: vaciar Papeleras de otros volúmenes o de otros usuarios; listar, restaurar o borrar elementos sueltos de la Papelera; vaciado programado o automático (prohibido); historial navegable de `cleanup_log` (D-24 de C-09); cambios a la whitelist o a `path-protection`.

## Capabilities

### New Capabilities
- `recycle-bin-empty`: vaciado de la Papelera del usuario en el volumen del sistema como única acción permanente: solicitud medida de un solo uso, doble confirmación aplicada por el backend, re-medición y rechazo si cambió, bloqueo en mismatch, exclusividad con escaneo y limpieza, resultado por medición y registro append-only, comandos IPC y diálogos de dos pasos.

### Modified Capabilities
- `cleanup`: "ninguna ruta de código borra de forma permanente" pasa a tener una única excepción declarada y auditada (el vaciado), y la exclusión mutua abarca escaneo, limpieza y vaciado.
- `domain-ports`: nuevo puerto de vaciado de la Papelera (separado de mover y de medir) que solo acepta la raíz del volumen del sistema, con doble configurable.
- `app-shell`: lista exacta de comandos IPC habilitados (3 nuevos).
- `junk-detection`: un escaneo no puede iniciarse mientras se vacía la Papelera (`recycle_bin_empty_in_progress`).

## Impact

- **Rust**: nuevo `src-tauri/src/domain/recycle_bin/` (solicitud con type-state, ejecución y clasificación del resultado, `*_tests.rs`); `domain/ports/recycle_bin_empty.rs` + `fakes.rs`; nuevo `adapters/windows/recycle_bin_empty.rs` (único archivo con `SHEmptyRecycleBinW`; `unsafe` mínimo con `// SAFETY:`); nuevo `commands/recycle_bin.rs` (+ tests); `commands/scan.rs` (`ActiveJob` con un tercer tipo de trabajo); `commands/names.rs`, `lib.rs`, `error.rs` (kinds nuevos); `tests/architecture.rs` (excepción de un solo archivo y uso restringido del puerto).
- **Capabilities**: `capabilities/main-window.json` + `allow-prepare-empty-recycle-bin`, `allow-acknowledge-empty-recycle-bin`, `allow-empty-recycle-bin`.
- **Frontend**: `src/lib/ipc.ts` (tipos y decodificadores), `src/features/cleanup/` (flujo de vaciado de dos diálogos, botón en la tarjeta y en el resumen), `src/shared/i18n/{es,errors}.ts`, `src/tests/support/mockBackend.ts` y tests.
- **Dependencias**: ninguna nueva; `SHEmptyRecycleBinW` y `SHERB_*` ya están en la feature `Win32_UI_Shell` habilitada de `windows` 0.62.2 (`CoInitializeEx` en `Win32_System_Com`, también habilitada). Lockfiles sin cambios. Sin migración (esquema sigue en v2).
- **Docs** (en el apply): `CHANGES.md` (estado de C-10), KB 11 (fila `recycle_bin`: protocolo y registro), KB 05 RN-LIM-06 (sin "pregunta abierta"), KB 07 (flujo de vaciado), KB 06 US-010.

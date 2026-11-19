# Proposal

## Why

US-011 (v0.1, gobernanza HIGH): el usuario quiere encontrar los archivos repetidos en sus carpetas y liberar espacio **sin perder datos**. Hoy la app solo limpia basura de una whitelist (C-08/C-09); los duplicados están en carpetas personales (Documentos, Descargas, Imágenes…), donde un error significa perder el único ejemplar de una foto o un documento. Es co-final de v0.1 (`C-09 → C-10 / C-11 → C-13`) y exige que "nunca se borra la última copia" y "nunca se elige por el usuario" los garantice el **backend**, no solo la interfaz (RN-DUP-01..06, DD-13, SU-06).

## What Changes

- **Escaneo de duplicados de solo lectura** (dominio puro sobre puertos): recorre las carpetas de contenido del usuario actual (Escritorio, Documentos, Descargas, Imágenes, Música, Videos) sin seguir puntos de reanálisis, verifica cada archivo con la protección de rutas (C-03) y descarta protegidos, contenido en la nube (sin abrirlo, para no descargarlo), archivos con varios hard links y archivos menores que el tamaño mínimo (1 MB por defecto, SU-06; opciones cerradas en avanzado).
- **Detección en tres etapas** (RN-DUP-01/02): agrupar por tamaño → hash parcial blake3 (primeros y últimos 64 KiB) → blake3 completo; solo es duplicado lo que coincide en hash completo. Lectura secuencial, cancelable entre bloques, que verifica la identidad del archivo abierto y descarta los que cambian mientras se leen.
- **Nuevo puerto de lectura de contenido** (separado de `FileSystem`): abre sin seguir reparse, sin recuperar contenido de la nube y compartiendo lectura/escritura/borrado (no bloquea al usuario); con doble en memoria.
- **Persistencia** (migración nueva, la **003** por la regla "el número lo toma quien mergea primero"; C-07 pasa a 004): `duplicate_group` y `duplicate_file` (con `kept` que solo escribe la decisión confirmada del usuario) más el resumen del escaneo y el estado por carpeta; solo se conservan los grupos del último escaneo de duplicados terminado.
- **Decisión siempre humana** (RN-DUP-03/04): ninguna copia viene preseleccionada en ningún modo; el backend rechaza toda decisión que no conserve al menos una copia por grupo o que no elimine ninguna; un grupo ya resuelto no admite otra decisión hasta volver a buscar.
- **Borrado por el flujo de C-09** (RN-DUP-05): las copias elegidas forman un preview de limpieza propio (ítems con categoría `duplicates`) que reutiliza el `PreviewSlot` (un solo uso, vence a los 10 min) y `run_cleanup` (re-verificación por handle, guardas, Papelera, log append-only). `run_cleanup` gana una **verificación adicional por ítem** con la fijación viva: cada copia se vuelve a hashear y la copia conservada del grupo se verifica antes de mover cualquier otra; si algo cambió, no se mueve nada de ese grupo.
- **Over-the-shoulder**: el escaneo se bloquea en mismatch (las carpetas serían de otra cuenta) y la barrera por-usuario se re-evalúa al confirmar.
- **Exclusividad**: el escaneo de duplicados y su ejecución ocupan el mismo lugar de trabajo (`ActiveJob`) que el escaneo de basura, la limpieza y el vaciado de la Papelera.
- **UI**: pantalla nueva **Duplicados** en la navegación. Simple: buscar, grupos con elección guiada ("¿Cuál querés conservar?", sin preselección, con carpeta y fecha de cada copia), revisar y mover a la Papelera. Avanzado: selección de carpetas y tamaño mínimo, rutas completas, hash, filtros por carpeta y tamaño, y conservar más de una copia.
- **IPC**: 5 comandos nuevos con sus 5 permisos `allow-*` explícitos; reutiliza `cancel_scan` y `cancel_cleanup`; sin `core:*`, plugins ni cambios de CSP. Ningún comando recibe rutas.
- Decisiones que requieren aprobación del dueño **[DUEÑO]** en `design.md` (alcance de carpetas, exclusiones, tamaño mínimo, esquema y numeración de la migración, modelo de decisión y elección guiada, integración con `run_cleanup`, re-verificación del contenido, guarda de 24 h, resolución de grupos, superficie IPC, UI, smoke).

Fuera de alcance: carpetas arbitrarias elegidas por el usuario (requiere selector de carpetas: plugin o COM propio; change aparte), otros volúmenes (PA-12), duplicados dentro de AppData/Program Files o rutas protegidas, reglas masivas de conservación ("conservar siempre la de Documentos"), vista previa de imágenes o abrir el archivo/ubicación (exigiría `fs`/`shell` en el webview), reemplazar copias por hard links, comparación byte a byte además de blake3.

## Capabilities

### New Capabilities
- `duplicates`: búsqueda de archivos duplicados en las carpetas de contenido del usuario (alcance, exclusiones, etapas de hash, lectura segura del contenido, bloqueo en mismatch, progreso y cancelación), grupos persistidos del último escaneo, decisión explícita del usuario con al menos una copia conservada, preview y ejecución por el flujo de limpieza con re-verificación del contenido, resolución de grupos, comandos IPC y pantalla Duplicados en ambos modos.

### Modified Capabilities
- `cleanup`: la ejecución acepta ítems de otras fuentes (copias duplicadas) con la misma re-verificación, y una verificación adicional por ítem con la fijación viva que puede omitir el ítem o, ante cancelación, dejarlo sin procesar.
- `domain-ports`: nuevo contrato de lectura de contenido (sin seguir reparse, sin recuperar de la nube, identidad por handle) y contrato de persistencia de duplicados, con dobles y batería de contrato.
- `local-store`: migración 3 (esquema de duplicados), grupos solo del último escaneo de duplicados, resolución atómica de grupos, escaneos interrumpidos que también descartan sus grupos.
- `app-shell`: lista exacta de comandos IPC habilitados (5 nuevos) y pantalla Duplicados en la navegación.

## Impact

- **Rust**: nuevo `src-tauri/src/domain/duplicates/` (modelo, raíces, recorrido, etapas de hash, decisiones, preview, guarda de contenido, `*_tests.rs`); `domain/ports/content.rs` (+ doble en `fakes.rs`) y contrato de persistencia de duplicados en `domain/ports/store.rs`; `domain/cleanup/` (categoría generalizada `CleanupCategory` y gancho `ItemGuard` en `run_cleanup`); `domain/protection/context.rs` (carpetas de contenido con su clave); nuevo `adapters/windows/content.rs` (lectura por handle; `unsafe` mínimo con `// SAFETY:`); `adapters/sqlite/` (migración `003_duplicates.sql`, `duplicates.rs`, contrato); nuevo `commands/duplicates.rs` (+ tests); `commands/scan.rs` (sin tipos de trabajo nuevos), `commands/names.rs`, `lib.rs`, `error.rs` (kinds nuevos); `tests/architecture.rs` (`kept` solo lo escribe la resolución confirmada).
- **Capabilities**: `capabilities/main-window.json` + `allow-start-duplicate-scan`, `allow-get-last-duplicate-scan`, `allow-list-duplicate-groups`, `allow-preview-duplicate-cleanup`, `allow-confirm-duplicate-cleanup`.
- **Frontend**: `src/lib/ipc.ts` (tipos y decodificadores), nuevo `src/features/duplicates/`, `src/app/navigation.ts`, `src/shared/i18n/{es,errors}.ts`, `src/tests/support/mockBackend.ts` y tests.
- **Dependencias**: ninguna nueva. `blake3 = "=1.8.7"` ya está fijado y pasa a tener consumidor; las constantes de apertura (`FILE_FLAG_OPEN_REPARSE_POINT`, `FILE_FLAG_OPEN_NO_RECALL`, `FILE_FLAG_SEQUENTIAL_SCAN`) están en la feature `Win32_Storage_FileSystem` ya habilitada. Lockfiles sin cambios.
- **Esquema**: versión 2 → 3.
- **Docs** (en el apply): `CHANGES.md` (C-11 en curso/decisiones; C-07 pasa a migración 004), KB 04 (tablas de duplicados), KB 05 RN-DUP-06 (carpetas y exclusiones), KB 02 (nombres reales de los comandos), KB 07 (flujo de duplicados), KB 06 US-011, KB 09 SU-06 (opciones cerradas), KB 10 (nueva pregunta abierta: carpetas arbitrarias / selector de carpetas).

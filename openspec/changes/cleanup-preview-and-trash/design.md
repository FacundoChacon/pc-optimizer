# Design

> Gobernanza **HIGH**: es el primer change que mueve archivos del usuario. Las decisiones marcadas **[DUEÑO]** requieren aprobación explícita antes del apply. Las tareas asumen la opción recomendada; si el dueño elige otra, se actualizan specs y tasks antes de codear (`/opsx:update`).

## Context

Motivación: ver `proposal.md`. Estado actual relevante (observado en el código, no supuesto):

- **Protección (C-03)**: `PathProtection::prepare_delete(&CheckedPath) -> Result<DeleteTarget, ProtectionDecision>` fija los ancestros (`FileSystem::pin_ancestors`, handles sin `FILE_SHARE_DELETE`), repite la resolución completa sin memo y compara `identity` y `kind` con lo escaneado (`identity_changed`). `DeleteTarget` expone la ruta real verificada y debe vivir hasta que el movimiento retorne. `ProtectionDecision::skip_outcome()` traduce `Protected → skipped_protected` y `SkippedCloud → skipped_cloud`. El doc de `service.rs` fija el contrato para C-09 (5 pasos). `CheckedPath` trae `path`, `kind`, `size_bytes`, `identity`, `rule`, **sin fecha**.
- **Adaptador de archivos**: `WindowsFileSystem::metadata` abre la entrada (`FILE_READ_ATTRIBUTES`, `FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS`, compartición total) y consulta **por handle** (`FileAttributeTagInfo`, `FileBasicInfo`, `FileStandardInfo`, `FileIdInfo`): tamaño (`EndOfFile`), última escritura, hard links, identidad y nube. `read_dir` usa en cambio los datos de la enumeración (sin identidad).
- **Hallazgos (C-08)**: el escáner guarda `size_bytes` = tamaño **por handle** (`checked.size_bytes`) pero `modified_at` = fecha **del listado** (`entry.meta.modified`). El smoke de C-08 (§9.2) mostró 13 de 16 978 hallazgos con fecha más nueva al releer por handle: caché de Brave/Edge **abiertos** (NTFS actualiza tarde la entrada de directorio de archivos abiertos para escritura). `junk_finding` tiene `volume_serial` (i64 por bits) y `file_id` (hex 32) obligatorios en elegibles; solo el último escaneo `done` conserva hallazgos.
- **Privilegios (C-05)**: `PrivilegeContext` se calcula una vez por proceso (estado gestionado); `scope_access(ctx, UserScope)` y `restrict_protection_context(raw, ctx)` (en mismatch quita el usuario actual → todo el perfil es `other_user_profile`). Los hallazgos persisten en la base del perfil del token, así que una sesión over-the-shoulder con la misma cuenta admin **ve hallazgos de una sesión anterior en `match`**: la barrera al previsualizar y al confirmar es real, no teórica.
- **Puerto `Trash`**: `move_to_trash(&self, &Path) -> Result<(), PortError>`; `FakeTrash` existe (registra y quita del `FakeFileSystem`). Sin adaptador real todavía.
- **Crate `trash = "=5.3.0"`** (fuente leída en `~/.cargo/registry`): en Windows crea un `IFileOperation` por llamada con `FOF_NO_UI | FOF_ALLOWUNDO | FOF_WANTNUKEWARNING`, un `IShellItem` por ruta (`SHCreateItemFromParsingName`, quitando `\\?\`) y un *sink* `IFileOperationProgressSink` que junta los fallos por ítem; devuelve `Err(Error::Os { code: HRESULT, description })` con el **primer** fallo y "(and N more item(s))", o `Error::Unknown("Some operations were aborted")`. Antes canonicaliza el **padre** con `std::fs::canonicalize` (el último componente no se resuelve). Inicializa COM por hilo con un `thread_local` (STA por la feature por defecto `coinit_apartmentthreaded`) y **hace `panic!`** si `CoInitializeEx` falla. Mapea `COPYENGINE_E_RECYCLE_FORCE_NUKE` ("the Recycle Bin is not available on this drive"), `..._RECYCLE_BIN_NOT_FOUND`, `..._RECYCLE_SIZE_TOO_BIG`, `..._SHARING_VIOLATION_SRC`, etc.
- **SQLite**: `cleanup_log` (001) ya admite `moved_to_trash | skipped_locked | skipped_protected | skipped_cloud | error | recycle_bin_emptied`, `scan_id` sin FK y `error_detail`; triggers abortan UPDATE/DELETE; índice por `ts`. WAL + `synchronous = NORMAL`. Migraciones 001-002 existentes; `CHANGES.md` reserva 003 (C-07), 004 (C-11), 005-010 (otros).
- **IPC/estado**: `ActiveScan` (un escaneo a la vez, cancelación por `AtomicBool`), progreso por `Channel` (exento del ACL), `run_blocking`, `APP_COMMANDS` ↔ capability sin `core:*`.
- **Frontend**: pantalla Limpieza de C-08 (tarjetas, "Ver rutas" en avanzado, `JunkScanProvider` con el estado de la sesión), `ConfirmDialog` (`<dialog>` nativo, `alertdialog`, foco en "Cancelar"), catálogo `es.ts`, `errors.ts` por `kind`, `mockBackend.ts` con `Channel` simulado.

## Goals / Non-Goals

**Goals:**
- Lo que se mueve es exactamente lo que el usuario vio y confirmó, re-verificado ítem por ítem contra el disco **en el momento** de moverlo (RN-LIM-04, RN-PROT-03).
- Imposible por construcción mover algo que no pasó por `prepare_delete` (type-state: el puerto solo acepta `DeleteTarget`).
- Ningún camino de código de producción borra de forma permanente (test de arquitectura).
- Resultado y registro por ítem, sin migración nueva.
- Dominio 100 % testeable con dobles; el adaptador real se prueba solo sobre `tempdir` y con `#[ignore]`.

**Non-Goals:**
- Liberar espacio real (eso ocurre al vaciar la Papelera, C-10); el resumen dice "movido a la Papelera".
- Detectar qué proceso usa un archivo, cerrar navegadores o detener servicios.
- Selección ítem por ítem, borrar carpetas vacías, historial navegable del log (D-24, D-25).
- Rendimiento óptimo: se mide en el smoke y hay una contingencia acotada (D-09).

## Decisions

### D-01 — Modelo del preview: en memoria, uno activo, de un solo uso, vence a los 10 min **[DUEÑO]**

- **A (recomendada)**: el preview vive en un estado gestionado `PreviewSlot` (`Mutex<Option<CleanupPreview>>`). Contiene `preview_id` (contador `u64` monótono del proceso, empezando en 1), `scan_id`, `created_at`, las categorías incluidas y una **copia** de las filas elegibles (ruta real, categoría, tamaño, fecha, identidad) tal como se mostraron. Crear un preview **reemplaza** al anterior (su id deja de valer). `confirm_cleanup` lo **consume** al empezar (aunque luego falle) → un `preview_id` nunca se ejecuta dos veces. Vence a los **10 minutos** (`PREVIEW_TTL_MS`): confirmar después → `preview_expired` y la UI pide uno nuevo. Al reiniciar la app no hay preview (cerrar nunca ejecuta nada).
- B: tabla `cleanup_preview` persistida. Exige migración (choque de numeración con C-07, ver D-07), sobrevive reinicios sin necesidad y agrega estado a limpiar. Rechazada.
- C: sin `preview_id` (confirmar por categorías). Rompe RN-LIM-04 ("la confirmación aplica solo al preview exacto mostrado"): un escaneo nuevo entre medio cambiaría el conjunto. Rechazada.
- La copia de filas garantiza "lo que ves es lo que se ejecuta"; además, al confirmar se exige que el escaneo del preview siga siendo el último `done` y no esté limpiado (D-20), si no → `preview_stale`.
- Alternativas de vencimiento: sin vencimiento (un preview de ayer confirmado hoy contradice el espíritu de RN-LIM-04), 2 min (molesto si el usuario lee con calma). 10 min es configurable solo como constante.

### D-02 — Qué puede elegir el usuario: categorías **[DUEÑO]**

- **A (recomendada)**: selección **por categoría** en ambos modos: el preview incluye todas las categorías limpiables con elegibles, todas marcadas; el usuario puede desmarcar. Cada cambio pide un preview nuevo (nuevo id). El frontend envía `categories: string[]` (enum del backend validado; nunca rutas ni IDs inventados).
- B: todo o nada. Más simple, pero el usuario no puede excluir, p. ej., la caché del navegador que está usando. Rechazada.
- C: ítem por ítem (avanzado). Miles de IDs por IPC, UI compleja (tabla con selección), y C-11 necesitará su propio modelo de selección de copias. Diferida: el dominio recibe una lista de filas, así que C-11/C-21 pueden construir el preview con IDs de hallazgos sin cambiar la ejecución.
- Limpiables = todas menos `recycle_bin` (solo medida; vaciarla es C-10) y `old_logs` (sin ubicaciones). Pedirlas, repetir una categoría o enviar una desconocida o la lista vacía → `invalid_argument`.

### D-03 — Contenido del preview (sin tocar el disco)

`build_preview` es una función pura sobre las filas persistidas + `PrivilegeContext` + `now`:

1. El escaneo debe ser el último `done` (`scan_not_found` si no) y no estar limpiado (`scan_already_cleaned`, D-20).
2. Por categoría pedida: si su alcance es `PerUser` y `scope_access` da `Blocked(reason)` → la categoría figura `blocked` con motivo y **no aporta ítems** (barrera 1). `crash_dumps` (mixta) se incluye; sus ítems del perfil caerán en la barrera 2 al confirmar (D-05).
3. Solo filas elegibles (`protected_flag = 0`).
4. Guarda de escritura reciente (D-04) con la fecha persistida: si `modified_at` es nula o `now - modified_at < 24 h`, la fila se excluye y se cuenta como `recent` (se informa, no se mueve).
5. Totales por categoría y generales: cantidad y bytes (tamaño persistido, que es el leído por handle al escanear).

No hay I/O: el preview es rápido y determinista. Toda verificación contra el disco ocurre al confirmar (RN-PROT-03).

### D-04 — Guarda de escritura reciente: 24 h, al previsualizar y al confirmar **[DUEÑO]**

Un archivo con última escritura en las últimas 24 h probablemente está en uso (descarga de Windows Update en curso, caché del navegador abierto, instalador). Como regla restrictiva (achica, nunca amplía; RN-GLB-01):

- **A (recomendada)**: **ningún archivo escrito en las últimas 24 h se mueve**, en todas las categorías. Al previsualizar se evalúa con la fecha persistida; al confirmar, con la fecha **leída del archivo abierto** (D-27). Sin fecha → no se mueve (fail-closed). Para `temp_*` coincide con el filtro de C-08 (que se re-evalúa así al confirmar, porque un temporal escrito después del escaneo vuelve a ser "reciente"). Costo: la caché del último día del navegador queda.
- B: guarda de 1 h. Libera más, protege menos (una descarga de WU pausada más de 1 h sería movible). 
- C: sin guarda; solo bloqueos. Chromium abre su caché con `FILE_SHARE_DELETE`: el renombre a la Papelera **tiene éxito con el navegador abierto** y escribe en un archivo ya movido (corrupción de caché recuperable, pero evitable). Rechazada.
- Constante del dominio (`RECENT_WRITE_GUARD_MS`), no un ajuste del usuario.

### D-05 — Secuencia de re-verificación al confirmar (por ítem, en este orden)

Antes del bucle (una vez por ejecución):
- Tomar el lugar de trabajo (D-19); consumir el preview (D-01); verificar escaneo vigente y no limpiado (`preview_stale`); verificar vencimiento (`preview_expired`).
- **Re-evaluar privilegios**: leer el `PrivilegeContext` gestionado y **resolver de nuevo** el contexto de protección crudo (`resolve_protection_context(ProcessToken, app_dirs)`), restringirlo (`restrict_protection_context`) y validarlo (`from_raw`). Nunca se reutiliza el del escaneo ni el del preview.

Por cada ítem (copia del preview):
1. **Cancelación**: si se pidió, se corta acá (D-18).
2. **Barrera 1 (alcance)**: categoría `PerUser` y `scope_access` bloqueado → `skipped_protected` / `blocked_<motivo>` sin tocar el disco.
3. **Reconstrucción**: `CheckedPath` desde la fila (ruta persistida normalizada con `normalize_lexical`, `kind = File`, identidad persistida). Ruta que no normaliza → `skipped_protected` con el código de rechazo.
4. **`prepare_delete`** con el servicio de protección del contexto recién resuelto (barrera 2): fija ancestros, re-resuelve sin memo (reglas, reparse en el camino, objetivo reparse/symlink/junction, hard links > 1, nube, ruta léxica = real) y compara identidad + tipo con lo persistido. `Err(decision)` → outcome según D-06.
5. **Cambio desde el escaneo** (con lo leído por handle en `target.checked()`): `size_bytes` distinto del persistido, o `modified` distinto del persistido → `skipped_locked` / `changed_since_scan`. (La fecha persistida viene del listado: si difiere de la del handle es justamente el caso "archivo abierto para escritura" del smoke de C-08 → se omite, que es lo correcto.)
6. **Guarda reciente** con la fecha del handle (D-04) → `skipped_locked` / `recently_modified`.
7. **Mover**: `trash.move_to_trash(&target)` (el puerto solo acepta `DeleteTarget`, D-11). Resultado → D-06.
8. **Post-verificación** (barata, defensa en profundidad): si la Papelera respondió éxito, `metadata(target.path())`: `NotFound` o identidad distinta → movido; la misma identidad sigue ahí → `error` / `not_moved`.
9. Soltar `target` (libera la fijación) **después** del paso 8.
10. Registrar la entrada del ítem en `cleanup_log` (D-16) y avisar progreso (D-18).

Tamaño registrado de un movido: el leído por handle en el paso 4 (lo realmente enviado a la Papelera), no el del escaneo.

### D-06 — Outcomes y códigos de detalle, sin outcomes nuevos **[DUEÑO]**

El roadmap fija 5 outcomes; varios casos nuevos (cambiado, reciente, desaparecido, Papelera no disponible) no tienen uno propio. Se usa `error_detail` con un **código estable** (ASCII `snake_case`, nunca texto localizado ni rutas) para todo lo que no es `moved_to_trash`:

| Situación | outcome | `error_detail` |
|---|---|---|
| Movido | `moved_to_trash` | `NULL` |
| Barrera 1 (mismatch) | `skipped_protected` | `blocked_different_user` / `blocked_undetermined` |
| `prepare_delete` → `Protected(reason)` | `skipped_protected` | `reason.code()` de C-03 (`identity_changed`, `reparse_point`, `multiple_hard_links`, `other_user_profile`, `unresolvable_not_found`, `unresolvable_pin_failed`, …) |
| `prepare_delete` → `Protected(Unresolvable(InUse))` | **`skipped_locked`** | `unresolvable_in_use` |
| `prepare_delete` → `SkippedCloud` | `skipped_cloud` | `skipped_cloud` |
| Tamaño o fecha distintos | `skipped_locked` | `changed_since_scan` |
| Escrito hace < 24 h o sin fecha | `skipped_locked` | `recently_modified` |
| Papelera: en uso / violación de compartición | `skipped_locked` | `in_use` |
| Papelera: no disponible en el volumen (deshabilitada, inexistente) | `error` | `recycle_bin_unavailable` (aborta, D-17) |
| Papelera: ítem demasiado grande para la Papelera | `error` | `recycle_bin_too_small` |
| Papelera: ruta demasiado larga para el shell | `error` | `path_too_long` |
| Papelera: acceso denegado / requiere elevación | `error` | `permission_denied` |
| Papelera: desapareció entre verificar y mover | `error` | `not_found` |
| Papelera respondió éxito pero el archivo sigue | `error` | `not_moved` |
| Backend de Papelera inutilizable (COM, pánico) | `error` | `trash_unavailable` (aborta, D-17) |
| Otro fallo | `error` | `io` o `io_0x<HRESULT>` |

- **A (recomendada)**: esta tabla, sin migración. `skipped_locked` agrupa "en uso o modificado" (lenguaje de UI: "en uso o modificados hace poco"), coherente con RN-LIM-05. Un archivo que ya no existe queda `skipped_protected` / `unresolvable_not_found` (la traducción de C-03), y la UI lo muestra como "ya no existían".
- B: outcomes nuevos (`skipped_changed`, `skipped_missing`, `cancelled`). Exige reconstruir `cleanup_log` (tabla `STRICT` con `CHECK`: no hay `ALTER`), recrear los triggers append-only y renumerar migraciones (C-07 → 004, C-11 → 005, …). Riesgo alto en la tabla de auditoría por poca ganancia. Rechazada para v0.1.
- Los códigos del detalle se definen en el dominio como constantes y la UI los traduce por familia (como `skipReasons` de C-08) con texto genérico de respaldo.

### D-07 — Migración: ninguna **[DUEÑO]**

No hace falta: outcomes y `error_detail` existen (001); "escaneo limpiado" se deriva del log (D-20); los previews viven en memoria (D-01). Así no se toca la numeración reservada (003 sigue siendo de C-07). Si el dueño elige D-06 B o D-01 B, la migración sería la **003** (regla de C-08 D-01: el número lo toma quien mergea primero) y `CHANGES.md` se renumera en el mismo change.

### D-08 — Backend de Papelera: crate `trash` (ya fijado) vs. adaptador propio de `IFileOperation` **[DUEÑO]**

- **A (recomendada)**: crate **`trash` 5.3.0**, ya en `Cargo.toml`/`Cargo.lock` y nombrado en RN-LIM-01 y KB 02. Hace lo que hace falta (`IFileOperation` + `FOF_ALLOWUNDO` + `FOF_WANTNUKEWARNING`, sin UI, resultado por ítem con `HRESULT`), su código es legible y sin red. Riesgos conocidos y su mitigación:
  - Agrupa errores de lote → **una llamada por ítem** (D-09).
  - `panic!` si `CoInitializeEx` falla → hilo dedicado con apartamento conocido + `catch_unwind` (D-10).
  - Canonicaliza el padre con `std` → los ancestros ya están fijados y verificados sin reparse por `prepare_delete`; el último componente no se sigue.
  - Quita `\\?\` y usa el *parsing name* del shell → rutas > `MAX_PATH` pueden fallar (`COPYENGINE_E_PATH_TOO_DEEP_SRC`) → `error` / `path_too_long` (se reporta, no se fuerza).
  - No expone si el ítem se recicló o se destruyó (no hay acceso al `psiNewlyCreated` de `PostDeleteItem`) → D-12.
- B: adaptador propio con `IFileOperation` (`windows` 0.62, features ya habilitadas `Win32_UI_Shell` + `Win32_System_Com`, más el macro `implement`). Permite lotes con resultado por ítem y **detectar** un borrado permanente (`psiNewlyCreated == NULL` en `PostDeleteItem`), pero suma ~300 líneas de COM `unsafe` propio a revisar (interfaces, *sink*, apartamento), contradice RN-LIM-01/KB 02 ("crate `trash`") y sigue sin poder **prevenir** la destrucción (solo verla después).
- C: `SHFileOperationW` directo. API heredada, sin resultado por ítem. Rechazada.
- Decisión de contingencia: si la verificación de D-12 falla o el rendimiento de D-09 es inaceptable, se vuelve al dueño con B antes de seguir.

### D-09 — Granularidad: una llamada por ítem (contingencia acotada) **[DUEÑO]**

- **A (recomendada)**: `trash::delete(path)` por ítem, mientras su `DeleteTarget` está vivo. Resultado exacto por ítem y fijación mínima. Costo: un `IFileOperation` por archivo (estimado 5–30 ms → 17 k archivos ≈ 1,5–8 min). Se **mide** en el smoke (tarea 9.2 d) con 500 archivos de prueba.
- Contingencia (solo con aprobación, si el smoke supera ~20 ms/ítem): lotes de hasta 64 objetivos verificados y fijados a la vez, `delete_all`, y conciliación por post-verificación de cada ítem (ausente → movido; presente → reintento individual para obtener el motivo). Mantiene "cada ítem verificado y fijado mientras se mueve" pero complica el código crítico; por eso no es el punto de partida.

### D-10 — Hilo dedicado para COM

El adaptador `WindowsTrash` arranca, por ejecución de limpieza, **un hilo propio** que hace todas las llamadas a `trash` y recibe pedidos por canal (`std::sync::mpsc`, respuesta síncrona por ítem). Así:
- el apartamento STA que `trash` inicializa por hilo no contamina los hilos del pool de bloqueo de Tauri (C-14 usará WMI/COM en MTA y fallaría con `RPC_E_CHANGED_MODE` en un hilo ya STA);
- el `thread_local` de COM se destruye (y `CoUninitialize` corre) al terminar el hilo;
- cada llamada va envuelta en `std::panic::catch_unwind`: un pánico de `trash` se traduce a `TrashError::BackendUnavailable` (D-17), nunca tumba la app.
El hilo solo recibe la ruta (`PathBuf`) del `DeleteTarget`; el `DeleteTarget` (con la fijación) sigue vivo en el hilo del dominio hasta recibir la respuesta.

### D-11 — Puerto `Trash`: solo objetivos verificados y error propio

- `fn move_to_trash(&self, target: &DeleteTarget) -> Result<(), TrashError>`. `DeleteTarget` solo lo construye `prepare_delete` (constructor privado al módulo `protection`), así que **no compila** mover algo sin re-verificar. Es un cambio del contrato de `domain-ports` (delta) y de `FakeTrash`/tests de C-03.
- `TrashError { InUse, BinUnavailable, TooLarge, PathTooLong, PermissionDenied, NotFound, BackendUnavailable, Other(String) }` (en vez de reutilizar `PortError::Unsupported` para tres casos con consecuencias distintas: abortar vs. seguir). `Other` lleva un código ASCII (`io_0x80270021`), nunca la descripción del sistema.
- Mapeo puro y testeable (`adapters/windows/trash_error.rs`) de `trash::Error`: `Os { code }` por `HRESULT` (`COPYENGINE_E_SHARING_VIOLATION_SRC`, `HRESULT_FROM_WIN32(32|33)` → `InUse`; `RECYCLE_FORCE_NUKE`, `RECYCLE_BIN_NOT_FOUND` → `BinUnavailable`; `RECYCLE_SIZE_TOO_BIG` → `TooLarge`; `PATH_TOO_DEEP_SRC`, `RECYCLE_PATH_TOO_LONG` → `PathTooLong`; `ACCESS_DENIED_SRC`, `E_ACCESSDENIED`, `REQUIRES_ELEVATION` → `PermissionDenied`; `PATH_NOT_FOUND_SRC`, `HRESULT_FROM_WIN32(2|3)` → `NotFound`), `CanonicalizePath` → `NotFound`, `TargetedRoot` → `Other("targeted_root")`, `Unknown` → `Other("aborted")`. Las constantes se confirman en el apply con `winmd-api-search` / fuente de `windows` 0.62.2.
- `FakeTrash` gana: fallo configurable por ruta con `TrashError`, y un *hook* "antes de mover" para simular un reemplazo en la ventana residual (el doble mueve lo que haya en la ruta, igual que el shell: el enlace, no su destino).

### D-12 — "Nunca permanente": verificación empírica obligatoria **[DUEÑO]**

El riesgo más grave de usar el shell es que, si el volumen no tiene Papelera (deshabilitada por el usuario o por política, `NukeOnDelete`, `NoRecycleFiles`) o el ítem no entra, `IFileOperation` con `FOF_NOCONFIRMATION` **destruya** el archivo. `trash` agrega `FOF_WANTNUKEWARNING`, que según la documentación "anula parcialmente `FOF_NOCONFIRMATION`" y, por el mapeo de `COPYENGINE_E_RECYCLE_FORCE_NUKE`, la intención es fallar por ítem. **No está verificado en esta máquina.** Además, PA-12 ya restringe la limpieza al volumen del sistema (`unsupported_volume` en C-03), así que "volumen sin Papelera" se reduce a "Papelera del sistema deshabilitada o ítem demasiado grande".

- **A (recomendada)**: compuerta en el apply (tarea 6.4) **antes de conectar la UI**: el dueño desactiva temporalmente la Papelera de `C:` para su usuario ("No mover archivos a la Papelera" en sus propiedades) y se corre un test `#[ignore]` que mueve **un archivo creado por el test en un `tempdir`**. Resultado esperado: `Err(BinUnavailable)` y el archivo sigue en su lugar, sin diálogos. Si en cambio el archivo desaparece o aparece un diálogo → **DETENER** y volver al dueño (opciones: preflight de la política en el registro — `HKCU/HKLM …\Policies\Explorer\NoRecycleFiles` y `…\Explorer\BitBucket\Volume\{GUID}\NukeOnDelete` — o el adaptador propio de D-08 B). Se restaura la configuración al terminar.
- Defensa adicional siempre activa: ante el primer `BinUnavailable` se **aborta** la ejecución (D-17), así un escenario no previsto afecta a lo sumo a un ítem.
- B: preflight del registro desde el inicio (sin esperar la verificación). Agrega un adaptador de registro (solo lectura) y lee claves no documentadas formalmente. **ADOPTADA tras el fallo de la compuerta A (ver el resultado abajo).**
- **Resultado de la compuerta (2026-11-14): A FALLÓ con `trash` solo → el dueño adoptó B (preflight del registro), y D-08 A (crate `trash`) se mantiene.** Con `C:` en `NukeOnDelete = 1`, `trash::delete` devolvió `Ok(())` y el archivo de prueba quedó destruido, ausente de la Papelera y sin diálogo: el crate NO falla por ítem, así que el aborto al primer `BinUnavailable` (D-17) nunca se dispara por sí solo. Diseño adoptado:
  - **Puerto `RecycleBinPolicy`** (dominio, con doble `FakeRecycleBinPolicy`): `availability(path) -> Result<BinAvailability, PortError>`. La decisión es una **función pura** `evaluate_policy(RawBinPolicy)`: deshabilitada si `NoRecycleFiles != 0` en `HKLM` o `HKCU` (`Software\Microsoft\Windows\CurrentVersion\Policies\Explorer`), o `NukeOnDelete != 0` global (`HKCU\...\Explorer\BitBucket`) o del volumen (`...\BitBucket\Volume\{GUID}\NukeOnDelete`); si no, habilitada con capacidad `MaxCapacity` (DWORD en **MB**; ausente = desconocida, no se omite por tamaño). `check_move` la cruza con el tamaño del archivo (`> capacidad` → `TooLarge`).
  - **Adaptador de Windows** `adapters/windows/recycle_bin_policy.rs` (solo lectura; `RegGetValueW`, `GetVolumePathNameW`, `GetVolumeNameForVolumeMountPointW`; feature `Win32_System_Registry`, sin cambio en `Cargo.lock`). **Fail-closed**: clave o valor inexistente = default de Windows (habilitada); cualquier otro error de lectura (acceso denegado, tipo inesperado, volumen sin GUID) = `Err` = no disponible.
  - **Control dentro del adaptador de Papelera, no salteable**: `WindowsTrash::move_to_trash` consulta la política del volumen del objetivo justo antes de cada `trash::delete`: deshabilitada o ilegible → `Err(BinUnavailable)`; tamaño verificado `>` capacidad → `Err(TooLarge)`; en ambos casos SIN llamar al shell.
  - **Preflight de UX en el dominio**: `Trash::preflight(&Path)` se ejecuta una vez, con la ruta del primer ítem, antes de mover nada; si falla, `run_cleanup` aborta con `recycle_bin_unavailable` (o `trash_unavailable` si el mecanismo es inutilizable) sin mover ni registrar nada. El preview debe exponer la disponibilidad para advertir en la UI (campo `recycleBinAvailable` del DTO; tareas 7.3a y 8.4a).
  - `TooLarge` por ítem sigue siendo `error` / `recycle_bin_too_small` y la ejecución continúa (sin cambios).
  - Riesgos aceptados: ver Risks (claves de registro no documentadas formalmente; TOCTOU residual si el usuario deshabilita la Papelera a mitad de la ejecución).
- "Ítem demasiado grande" (> capacidad máxima de la Papelera) también se verifica (`TooLarge`) en la misma compuerta si el dueño lo permite reduciendo la capacidad a 1 MB con un archivo de prueba de 2 MB; si no, queda como riesgo documentado.

### D-13 — Archivos bloqueados o en uso

No se prueban antes (C-08 D-12). Se detectan en tres puntos, todos `skipped_locked`: `metadata` con `InUse` dentro de `prepare_delete` (`unresolvable_in_use`), guardas de cambio/reciente (D-05 pasos 5–6) y violación de compartición al mover (`in_use`). Nunca se reintenta, se mata un proceso ni se programa para el reinicio (RN-LIM-05).

### D-14 — Placeholders de nube

Los detecta `prepare_delete` (C-03): atributos `RECALL_ON_DATA_ACCESS | RECALL_ON_OPEN | OFFLINE` o tag de reparse `IO_REPARSE_TAG_CLOUD*` del objetivo o de un ancestro, leídos con `FILE_READ_ATTRIBUTES` sin seguir reparse (no hidrata). C-08 ya los omite al escanear; al confirmar aparecen solo si cambiaron desde entonces (p. ej. la carpeta pasó a OneDrive) → `skipped_cloud`.

### D-15 — Windows Update descargando en `SoftwareDistribution\Download`

- **A (recomendada)**: sin lógica específica de WU: la guarda de 24 h (D-04) deja fuera lo que WU escribió recientemente (descarga en curso o pausada hace poco) y lo que el servicio tiene abierto sin compartir falla con `in_use`. Lo que WU dejó hace más de 24 h y no tiene abierto es caché que WU vuelve a descargar si la necesita.
- B: consultar el estado de `wuauserv`/`BITS`/`TrustedInstaller` por el Service Control Manager y omitir toda la categoría si alguno corre. `wuauserv` arranca por disparador y suele estar en ejecución sin hacer nada → bloquearía la categoría casi siempre; además suma un puerto y una feature nueva de `windows`. Rechazada.
- Nunca se detienen servicios (sería un tweak del sistema fuera de alcance).

### D-16 — Registro en `cleanup_log`

- **Cuándo**: una entrada por ítem procesado, **después** de conocer su resultado (paso 10 de D-05), en su propia transacción (`append_cleanup_log(&[entry])`; con WAL + `synchronous = NORMAL` no hay `fsync` por commit). No se escribe "intención" previa: la tabla es append-only y no admite un estado pendiente que luego se actualice.
- **Campos**: `ts` = reloj al terminar el ítem; `category` = categoría del hallazgo; `path` = ruta real verificada (`target.path()`) si se llegó a `prepare_delete` con éxito, si no la ruta persistida; `size_bytes` = tamaño por handle si se verificó, si no el persistido; `outcome` y `error_detail` según D-06; `scan_id` = escaneo del preview.
- Ítems no procesados por cancelación o aborto **no** se registran (no hubo acción); el resumen los informa como "no procesados".
- Ventana residual: si la app muere entre mover y registrar, queda a lo sumo **un** movimiento sin entrada (está en la Papelera, recuperable). Documentado en Riesgos.

### D-17 — Abortos de la ejecución

Se detiene el bucle (sin procesar más ítems) y la respuesta trae `status: "aborted"` con `abortReason`:
- `log_unavailable`: falla `append_cleanup_log` → no se sigue moviendo sin auditoría.
- `recycle_bin_unavailable`: el preflight de D-12 B falla antes de mover nada (no se registra ningún ítem) o, en la ventana de carrera, el primer `BinUnavailable` por ítem (el ítem queda registrado como `error`). Ya no se confía en que el crate `trash` produzca este error por sí solo (la compuerta mostró que destruye sin avisar): lo produce el control de política del adaptador.
- `trash_unavailable`: `BackendUnavailable` (COM o pánico de `trash`).
Lo ya movido queda en la Papelera y registrado.

### D-18 — Progreso y cancelación **[DUEÑO]**

- `confirm_cleanup(previewId, onProgress: Channel<CleanupEvent>)` (mismo patrón que C-08 D-13, sin permisos `core:*`). Eventos: `started { previewId, total }`, `progress { processed, total, movedCount, movedBytes }` cada **32** ítems y al final del bucle (por conteo: dominio sin reloj para el ritmo, tests deterministas). La respuesta del comando es el resumen.
- `cancel_cleanup(previewId)`: marca la bandera si esa ejecución está en curso, si no `preview_not_found`. El dominio la consulta **antes de cada ítem**; el ítem en curso termina (mover es atómico para el shell) y se registra. Resultado `status: "cancelled"` con `notProcessed`.
- Alternativa: sin cancelación. Rechazada: con D-09 A una ejecución grande puede durar minutos.

### D-19 — Exclusión mutua escaneo/limpieza

`ActiveScan` se generaliza a `ActiveJob` (`Mutex<Option<{ job: Scan(ScanId?) | Cleanup(preview_id), cancel }>>`): un único lugar para escaneo **o** limpieza.
- `start_scan` con una limpieza en curso → `cleanup_in_progress`; `confirm_cleanup` o `preview_cleanup` con un escaneo en curso → `scan_in_progress`; un segundo `confirm_cleanup` → `cleanup_in_progress`.
- `preview_cleanup` no toma el lugar (es instantáneo) pero lo consulta. `cancel_scan`/`cancel_cleanup` solo actúan sobre su tipo.
- Motivo: un escaneo que termina durante la limpieza purga los hallazgos del escaneo anterior (C-08 D-04) y cambiaría el "último `done`" a mitad.

### D-20 — Qué pasa con el escaneo y `junk_finding` tras limpiar **[DUEÑO]**

- **A (recomendada)**: `junk_finding` **no se toca** (es la foto del escaneo). Un escaneo se considera **limpiado** si existe al menos una entrada de `cleanup_log` con su `scan_id` y `ts >= finished_at` del escaneo (consulta acotada por el índice `ts`; sin migración). Un escaneo limpiado ya no admite previews (`scan_already_cleaned`) y la UI pide "Buscar de nuevo". `get_last_junk_scan` agrega `cleanedAt` (máximo `ts` de esas entradas, o `null`). Una cancelación antes del primer ítem no deja entradas → el escaneo sigue sin limpiar y se puede previsualizar otra vez.
- B: borrar los hallazgos movidos (y dejar los omitidos). Los totales por categoría (`junk_category_result`) quedarían desfasados y habría que recalcular resúmenes. Rechazada.
- C: re-escanear automáticamente al terminar. Tarda (3–11 s medidos en C-08) y mezcla dos operaciones; queda como botón explícito "Buscar de nuevo" en el resumen.

### D-21 — Superficie IPC **[DUEÑO]**

| Comando | Args | Respuesta |
|---|---|---|
| `preview_cleanup` | `{ scanId, categories: string[] }` | `CleanupPreviewDto` |
| `confirm_cleanup` | `{ previewId, onProgress: Channel }` | `CleanupSummaryDto` |
| `cancel_cleanup` | `{ previewId }` | `null` |
| `get_last_junk_scan` (existente) | — | `JunkScanSummaryDto` + `cleanedAt: number \| null` |

```
CleanupPreviewDto = { previewId, scanId, createdAt, expiresAt, totalItems, totalBytes,
  categories: [{ category, status: "included" | "blocked", blockReason?: "different_user" | "undetermined",
                 itemCount, sizeBytes, recentCount, recentBytes }] }
CleanupSummaryDto = { previewId, scanId, status: "completed" | "cancelled" | "aborted",
  abortReason: null | "log_unavailable" | "recycle_bin_unavailable" | "trash_unavailable",
  startedAt, finishedAt, totalItems, processed, notProcessed, movedCount, movedBytes,
  skippedLocked, skippedProtected, skippedCloud, errors,
  reasons: [{ outcome, detail, count }],                        // ordenado por count desc
  categories: [{ category, movedCount, movedBytes, skippedCount, errorCount }] }
```

camelCase, sin rutas, SIDs ni nombres de cuenta (las rutas siguen solo en `list_junk_findings` y en el log). `previewId`/`scanId` como enteros JS seguros (validados `<= 2^53 - 1`). Se agregan exactamente 3 permisos `allow-*`.

### D-22 — Errores nuevos

`AppError` suma `PreviewNotFound` (`preview_not_found`), `PreviewExpired` (`preview_expired`), `PreviewStale` (`preview_stale`), `ScanAlreadyCleaned` (`scan_already_cleaned`), `CleanupInProgress` (`cleanup_in_progress`) y `NothingToClean` (`nothing_to_clean`: confirmar un preview con 0 ítems). Reutiliza `scan_in_progress`, `scan_not_found`, `invalid_argument` y `store_*`. Cada `kind` con texto en `es.errors`. Un fallo **por ítem** nunca es `AppError` (es un outcome).

### D-23 — Resumen mostrado

- Simple: "Se movieron N archivos (X) a la Papelera. El espacio se libera cuando vacíes la Papelera." + una línea por familia no vacía en lenguaje llano ("N en uso o modificados hace poco: se omitieron", "N protegidos o cambiados: no se tocaron", "N en la nube: no se tocaron", "N no se pudieron mover"), aviso si fue cancelada ("Cancelaste la limpieza: N archivos no se procesaron") o abortada (mensaje por `abortReason`), y botón "Buscar de nuevo".
- Avanzado: además, tabla por categoría (movidos/bytes/omitidos/errores) y por motivo (código crudo al lado del texto, como C-08).
- Inconsistencia con el roadmap: `CHANGES.md` dice "resumen (liberado, omitidos)"; mover a la Papelera **no libera** espacio (DD-05). La UI dice "movido a la Papelera" (ver "Desvíos").

### D-24 — Historial de `cleanup_log` fuera de C-09 **[DUEÑO]**

KB 02 lista `get_cleanup_log` y KB 12 "log" en Limpieza avanzada, pero el alcance de C-09 en `CHANGES.md` no lo incluye. **Recomendado**: no agregarlo en C-09 (menos superficie IPC en un change HIGH); el resumen de cada ejecución cubre US-009. Se anota en `CHANGES.md` para C-12 o un change propio. Alternativa: `get_cleanup_log { limit }` de solo lectura + tabla en avanzado (≈ +1 comando, +1 componente).

### D-25 — Carpetas vacías: no se borran

Pregunta abierta de C-08. Los hallazgos son archivos; borrar carpetas exigiría una decisión de protección por carpeta, verificar que esté vacía en el momento y moverla con el shell (que movería también lo que aparezca adentro). Se dejan; se anota para un change posterior si el dueño lo pide.

### D-26 — Flujo de UI: el preview es el diálogo de confirmación **[DUEÑO]**

- **A (recomendada)**: botón primario **"Limpiar"** en la tarjeta de resultados (habilitado si hay elegibles, el escaneo no está limpiado y no hay escaneo en curso; "Buscar basura" pasa a secundario mientras haya algo para limpiar). Al pulsarlo se pide el preview y se abre `CleanupPreviewDialog` (`<dialog>` nativo, `role="alertdialog"`, patrón de `ConfirmDialog`): título "Revisá antes de limpiar"; lista de categorías con casilla (todas marcadas), nombre llano, cantidad y tamaño; las bloqueadas, deshabilitadas con su motivo; total grande; notas fijas ("Todo va a la Papelera: podés recuperarlo desde ahí", "No se tocan archivos en uso ni modificados en las últimas 24 horas (N archivos, X)"); botones **"Cancelar"** (foco inicial) y **"Mover N archivos a la Papelera"** (`danger`). Desmarcar/marcar pide un preview nuevo y deshabilita el botón de confirmar hasta recibirlo (región `aria-live`). Esc, "Cancelar" o cerrar → no se ejecuta nada (el preview vence solo). Un solo paso de confirmación: la doble confirmación queda reservada a la acción permanente (C-10).
- Durante la ejecución: el diálogo se cierra y la pantalla muestra un panel de progreso (`aria-live="polite"`: "Moviendo a la Papelera: N de M") con "Cancelar"; al terminar, el foco pasa al título del resumen (WCAG 2.4.3).
- `preview_expired` al confirmar → se pide un preview nuevo automáticamente y se muestra "El resumen se actualizó; revisalo antes de confirmar" (nunca se confirma solo).
- B: panel en línea + `ConfirmDialog` aparte (dos pasos). Más clics sin más seguridad; contradice "una acción primaria por pantalla". Rechazada.
- Modo avanzado: mismas piezas + en el diálogo, columnas de cantidad/bytes recientes por categoría; en el resumen, las tablas de D-23. Las rutas siguen en "Ver rutas" (C-08).
- Textos nuevos (catálogo `es.cleanup.*`, sin literales en componentes): `clean`, `previewTitle`, `previewLoading`, `previewTotal`, `previewRecycleNote`, `previewRecentNote`, `previewBlocked`, `confirmMove` ("Mover {count} archivos a la Papelera"), `cancel`, `previewUpdated`, `cleaningProgress` ("Moviendo a la Papelera: {done} de {total}"), `cancelCleaning`, `summaryTitle`, `summaryMoved`, `summarySpaceNote`, `summaryLocked`, `summaryProtected`, `summaryCloud`, `summaryErrors`, `summaryCancelled`, `summaryAborted.{log_unavailable,recycle_bin_unavailable,trash_unavailable}`, `scanAgain`, `alreadyCleaned` ("Ya limpiaste este análisis el {date}. Buscá basura de nuevo para ver el estado actual."), `cleanupReasons.*` (familias de D-06) y los 6 `errors.*` de D-22.

### D-27 — `CheckedPath.modified` (delta mínima de `path-protection`)

Para D-05 pasos 5–6 hace falta la fecha leída del archivo abierto en la misma re-verificación. Se agrega `modified: Option<EpochMillis>` a `CheckedPath`, poblado desde `FileSystem::metadata` (que ya la lee por handle con `FileBasicInfo`). Alternativa: una segunda llamada a `metadata` tras `prepare_delete` → otro handle, otra ventana y otra comparación de identidad; rechazada. C-08 no cambia de comportamiento (sigue guardando la fecha del listado; unificarlo queda para otro change).

### D-28 — Estructura de módulos

```
src-tauri/src/domain/cleanup/
  mod.rs        // re-exports + contrato para C-10/C-11/C-21
  model.rs      // CleanupItem, CleanupPreview, PreviewCategory, CleanupRecord, CleanupSummary, AbortReason
  detail.rs     // códigos de error_detail (constantes) + mapeo D-06
  preview.rs    // build_preview (puro, D-03/D-04)
  execute.rs    // run_cleanup sobre FileSystem + Trash + CleanupLogStore + Clock + cancelación + observer (D-05)
  *_tests.rs
src-tauri/src/domain/ports/trash.rs        // move_to_trash(&DeleteTarget) -> Result<(), TrashError>
src-tauri/src/domain/ports/store.rs        // JunkFindingStore::cleanup_candidates, CleanupLogStore::scan_cleaned_at
src-tauri/src/domain/protection/{decision,service}.rs  // CheckedPath.modified
src-tauri/src/adapters/windows/trash.rs    // WindowsTrash: hilo dedicado + catch_unwind
src-tauri/src/adapters/windows/trash_error.rs  // mapeo puro trash::Error -> TrashError
src-tauri/src/adapters/sqlite/{junk,store...}.rs  // consultas nuevas
src-tauri/src/commands/cleanup.rs          // 3 comandos + DTOs + PreviewSlot
src-tauri/src/commands/scan.rs             // ActiveJob, cleanedAt
src/features/cleanup/{CleanupPreviewDialog,CleanupProgress,CleanupSummary}.tsx, useCleanup.ts
```

Type-state: `CleanupPreview` (solo lo crea `build_preview`) → `run_cleanup(preview, …)` lo consume por valor; `DeleteTarget` (solo `prepare_delete`) → `Trash::move_to_trash`.

### D-29 — Tests

- Dominio (mayoría, dobles): preview (último `done`, limpiado, categorías inválidas/repetidas/vacías, `recycle_bin`/`old_logs` rechazadas, mismatch bloquea por-usuario y deja `crash_dumps`, guarda 24 h con fecha nula/borde exacto, totales); ejecución con **una tabla por fila de D-06** (cada outcome y detalle), TOCTOU (reemplazo por otro archivo, por symlink, por junction en el padre, hard link nuevo, nube nueva, desaparecido), re-evaluación de privilegios (preview en `match`, confirm con contexto `mismatch` → nada del perfil se fija ni se mueve: `calls()` del `FakeFileSystem` sin rutas del perfil y `FakeTrash::moved()` vacío), cambio de tamaño/fecha, guarda reciente por handle, abortos (log, Papelera, backend), cancelación a mitad, progreso cada 32, un error no detiene el resto, post-verificación `not_moved`, orden de fijación/liberación (la fijación vive hasta después de mover).
- Persistencia: batería de contrato (doble + SQLite temporal) para `cleanup_candidates` y `scan_cleaned_at`; el log sigue rechazando UPDATE/DELETE.
- Adaptador: mapeo de errores (tabla de `HRESULT`, siempre corre); `WindowsTrash` real **solo sobre `tempdir`** y `#[ignore = "mueve archivos de prueba a la Papelera real; correr a mano con --ignored"]`: archivo movido, archivo abierto sin compartir → `InUse`, junction como último componente → se mueve la junction y el destino queda intacto, symlink de archivo (privilegio → `#[ignore]` propio), ancestros fijados no impiden mover, ruta > 260; compuerta D-12 con su propio `#[ignore]`.
- Arquitectura: el código de producción no contiene `remove_file`, `remove_dir`, `DeleteFileW`, `SHEmptyRecycleBin`, `SHFileOperation` ni `FOF_` (solo el crate `trash` mueve).
- Comandos: DTOs exactos, errores por mock runtime (`preview_not_found`, `preview_expired`, `preview_stale`, `scan_already_cleaned`, `cleanup_in_progress`, `scan_in_progress`, `nothing_to_clean`), exclusión mutua, baseline de capabilities.
- Frontend: decodificadores (formas válidas/ inválidas), diálogo (foco en Cancelar, Esc no ejecuta, desmarcar pide preview nuevo y deshabilita confirmar, bloqueadas deshabilitadas), progreso y cancelación, resumen simple/avanzado, `preview_expired` refresca, `alreadyCleaned`, errores por `kind`, `uiLiterals`.

### D-30 — Dependencias y features

Ninguna dependencia nueva. `trash` ya está fijado (pasa a tener consumidor; el comentario de `[lints.cargo]` se actualiza). Si el apply necesita una feature más del crate `windows` (constantes `COPYENGINE_E_*` en `Win32_UI_Shell`, ya habilitada), se agrega solo si `Cargo.lock` no cambia; si cambia, DETENER y consultar.

## Risks / Trade-offs

- [El shell destruye en vez de reciclar si la Papelera está deshabilitada o el ítem no entra] → `FOF_WANTNUKEWARNING` del crate, limpieza solo en el volumen del sistema (PA-12), compuerta empírica D-12 antes de la UI y aborto al primer `BinUnavailable`.
- [Claves de registro no documentadas formalmente por Microsoft (`NukeOnDelete` y `MaxCapacity` por volumen, `NukeOnDelete` global)] → solo lectura, fail-closed ante cualquier error distinto de "no existe", decisión pura con tests por tabla y un test ignorado de solo lectura que imprime la política real; si Windows cambiara su significado, el efecto es negarse a mover, nunca destruir.
- [TOCTOU residual: el usuario deshabilita la Papelera o reduce su capacidad DESPUÉS de la comprobación por ítem y antes de que el shell actúe] → la política se lee justo antes de cada `trash::delete` (ventana de milisegundos); aceptado por el dueño: a lo sumo afecta a un ítem elegible de la whitelist (temporales y cachés).
- [Rendimiento: un `IFileOperation` por archivo] → progreso + cancelación; medición en el smoke; contingencia por lotes con aprobación (D-09).
- [Pánico de `trash` al inicializar COM] → hilo dedicado + `catch_unwind` → `trash_unavailable` y aborto (D-10, D-17).
- [Rutas largas (> 260) que el shell no acepta] → `error` / `path_too_long`, reportado; la caché con rutas largas queda.
- [Un movimiento sin entrada de log si la app muere justo entre mover y registrar] → ventana de un ítem; el archivo está en la Papelera (recuperable); documentado.
- [Base que crece con el log: ~17 k filas por limpieza grande] → aceptable en v0.1 (retención indefinida, KB 04; purga manual futura, PA-13).
- [Guarda de 24 h + cambio desde el escaneo omiten bastante caché de navegadores abiertos] → coherente con "ante duda, no tocar"; el resumen lo explica.
- [La fijación de ancestros (sin `FILE_SHARE_DELETE`) podría impedir al shell mover el archivo] → el spec de `domain-ports` ya exige que el objetivo siga siendo movible con ancestros fijados; test real `#[ignore]` lo confirma con `trash`.
- [`IFileOperation` sobre un STA sin bomba de mensajes] → patrón por defecto del crate (`coinit_apartmentthreaded`); se confirma en los tests reales y el smoke.
- [Over-the-shoulder: lo de máquina va a la Papelera de la cuenta elevada] → esperado (la Papelera es por-usuario); el aviso de C-05 sigue visible; categorías por-usuario bloqueadas.
- [Dos instancias de la app limpiando a la vez] → raro en v0.1 (sin bandeja); `prepare_delete` + identidad lo vuelve inocuo (la segunda ve `unresolvable_not_found`).
- [Pérdida del preview al reiniciar] → intencional (cerrar no ejecuta nada).

## Migration Plan

- Sin migración de esquema (D-07). Bases v2 siguen v2.
- Capabilities: +3 permisos `allow-*` propios; sin cambios de CSP.
- Docs a actualizar en el apply (tarea 1.3): `CHANGES.md` (estado de C-09, nota de D-24 y D-25, nota para C-10 de que la Papelera es la del token elevado y para C-11/C-21 de cómo construir un preview con filas propias), KB 11 (re-verificación por handle, guarda de 24 h, códigos de detalle, aborto por Papelera no disponible), KB 04 (`error_detail` = código estable), KB 07 Flujo 2 (abortos y cancelación).
- Rollback: revertir el change deja hallazgos y log intactos; las entradas de log escritas siguen siendo válidas para versiones anteriores (mismos outcomes).

## Decisiones del dueño (bloquean el apply)

**Aprobadas por el dueño el 2026-11-14**: todas según la recomendación (D-01, D-02, D-04 = 24 h en todas las categorías, D-06, D-07, D-08, D-09, D-12, D-18, D-20, D-21, D-24, D-26). Para la compuerta D-12 el dueño autorizó que **la ejecute el agente**: guardar el valor de `HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\BitBucket\Volume\{GUID de C:}\NukeOnDelete` (y `MaxCapacity` si se prueba `TooLarge`), ponerlo en 1 solo durante el test y restaurarlo SIEMPRE (`finally`), avisando al dueño antes y después.

| ID | Decisión | Recomendación |
|---|---|---|
| D-01 | Modelo del preview | En memoria, uno activo, un solo uso, vence a los 10 min |
| D-02 | Selección | Por categoría (todas marcadas, desmarcables) |
| D-04 | Guarda de escritura reciente | 24 h en todas las categorías, al previsualizar y al confirmar (por handle) |
| D-06 | Outcomes | Los 5 existentes + `error_detail` con código estable (tabla D-06) |
| D-07 | Migración | Ninguna (003 sigue reservada a C-07) |
| D-08 | Backend de Papelera | Crate `trash` 5.3.0 (ya fijado), con hilo dedicado |
| D-09 | Granularidad | Una llamada por ítem; lotes solo si el smoke supera ~20 ms/ítem y con nueva aprobación |
| D-12 | Nunca permanente | **B adoptada (2026-11-14, tras el fallo de la compuerta A)**: preflight de política del registro dentro del adaptador de Papelera + preflight de UX en el dominio; D-08 A (crate `trash`) se mantiene |
| D-18 | Progreso/cancelación | `Channel`, aviso cada 32 ítems, cancelación entre ítems |
| D-20 | Escaneo tras limpiar | `junk_finding` intacto; escaneo "limpiado" derivado del log; exige re-escanear |
| D-21 | Superficie IPC | 3 comandos nuevos + `cleanedAt` en `get_last_junk_scan` |
| D-24 | Historial del log | Fuera de C-09 (nota en `CHANGES.md`) |
| D-26 | UI | Diálogo de preview = confirmación (un paso), foco en "Cancelar" |

## Desvíos respecto de CHANGES.md / KB

- `CHANGES.md` C-09: "resumen (liberado, omitidos)" → mover a la Papelera no libera espacio (DD-05); se dice "movido a la Papelera" (D-23).
- `CHANGES.md` C-09: "volumen sin Papelera se omite y reporta" y KB 11 "Volumen sin Papelera (red/USB)" → con PA-12 no hay otros volúmenes limpiables; el caso real es "Papelera del sistema deshabilitada", que se reporta como `error`/`recycle_bin_unavailable` y **aborta** (D-12, D-17) en vez de omitirse ítem por ítem.
- `CHANGES.md` C-09 lista outcomes sin un caso para "cambiado desde el escaneo", "reciente" o "ya no existe" → se usan los existentes + `error_detail` (D-06).
- KB 02 (`get_cleanup_log`) y KB 12 ("log" en Limpieza avanzada) → diferidos (D-24).
- KB 11 §Registro enumera los campos sin `error_detail`; KB 04 sí lo tiene → se documenta como código estable.
- KB 07 Flujo 2 no contempla cancelación ni aborto → se agregan en el apply.
- `junk-detection` exigía "MUST NOT ofrecer ninguna acción que limpie" en esta versión → delta MODIFIED (era transitorio hasta C-09).

## Open Questions

- Umbral de aviso de progreso (32 ítems): ajustable en el apply según la fluidez observada; no cambia specs.
- Texto exacto de los mensajes del catálogo: se ajusta con la revisión UX (tarea 8.6) sin cambiar specs.

## Apply notes

_(Se completa durante el apply; lote 1 = grupos 1 a 5. Quedan para los lotes siguientes: compuerta D-12, medición de D-09, revisión de `unsafe`, conteos finales y resultados del smoke.)_

### Lote 1 (grupos 1 a 5): dominio de limpieza

**Línea base (1.1)**, desde terminal NO elevada, rama `feat/cleanup-preview-and-trash`:

| Comando | Resultado |
|---|---|
| `npm test` | 13 archivos, **227 tests** passing |
| `npm run build` | verde (un primer intento falló solo por correr en paralelo con otro `npm run build`; repetido en solitario: verde) |
| `cargo test --locked` | lib **525 passed, 8 ignored**; integración: `architecture` 5, `manifest` 5, `no_network_crates` 9, `security_baseline` 21 (40 en total); doc-tests 0 |
| `cargo fmt --check` / `cargo clippy --all-targets --all-features --locked -D warnings` | verdes |

**Constantes confirmadas (1.2)** en el fuente de `windows-0.62.2` (`Win32/UI/Shell/mod.rs`, módulo `windows::Win32::UI::Shell`; feature `Win32_UI_Shell` ya habilitada) y `trash-5.3.0` (`src/windows.rs`), sin invocar la skill `winmd-api-search` (el fuente de las crates es la referencia exacta de las versiones fijadas):

| Constante | HRESULT |
|---|---|
| `COPYENGINE_E_SHARING_VIOLATION_SRC` | `0x80270027` |
| `COPYENGINE_E_RECYCLE_FORCE_NUKE` | `0x80270036` |
| `COPYENGINE_E_RECYCLE_BIN_NOT_FOUND` | `0x8027003A` |
| `COPYENGINE_E_RECYCLE_SIZE_TOO_BIG` | `0x80270037` |
| `COPYENGINE_E_RECYCLE_PATH_TOO_LONG` | `0x80270038` |
| `COPYENGINE_E_PATH_TOO_DEEP_SRC` | `0x8027001D` |
| `COPYENGINE_E_ACCESS_DENIED_SRC` | `0x80270021` |
| `COPYENGINE_E_REQUIRES_ELEVATION` | `0x80270002` |
| `COPYENGINE_E_PATH_NOT_FOUND_SRC` | `0x80270023` |
| `E_ACCESSDENIED` (`Win32/Foundation`) | `0x80070005` |
| `HRESULT_FROM_WIN32(32 / 33 / 2 / 3)` | `0x80070020 / 0x80070021 / 0x80070002 / 0x80070003` |

- `trash` 5.3.0 usa `FOF_NO_UI | FOF_ALLOWUNDO | FOF_WANTNUKEWARNING` (`FOF_NO_UI` = 1556, `FOF_ALLOWUNDO` = 64, `FOF_WANTNUKEWARNING` = 16384); `Error::Os { code }` lleva el `HRESULT` del PRIMER ítem fallido (`failures.first()`); la feature por defecto es `coinit_apartmentthreaded`. Ninguna constante exige una feature nueva de `windows`: `Cargo.lock` y `package-lock.json` sin cambios.

**Decisiones de implementación del lote 1** (todas dentro del diseño; ninguna cambia specs):
- `outcome_for_decision` devuelve `Option<(CleanupOutcome, &'static str)>` (`None` para `Allowed`) en vez de la tupla de la tarea 4.1: un `Allowed` no tiene resultado de omisión. `outcome_for_trash` conserva `Other(code)` solo si es ASCII `snake_case` estable (`[a-z0-9_]+`); cualquier otra cosa (rutas, texto del sistema) se reduce a `io`.
- `build_preview` recibe una estructura `PreviewInput` (en lugar de 8 argumentos posicionales) con el escaneo pedido y el último `done` por separado, de modo que `ScanNotFound` se decide en el dominio; `validate_categories` es pública para que el comando valide antes de consultar la base. `CleanupPreview` tiene campos `pub(super)` (solo `domain::cleanup` lo construye) y accesores públicos; `CleanupPreview::for_tests` (solo `cfg(test)`) arma previews con filas que `build_preview` excluiría.
- `run_cleanup` recibe además el contexto de protección ya restringido; con contexto `Err` todos los ítems quedan `skipped_protected/context_unavailable` SIN tocar el disco (ni siquiera se fijan ancestros). La barrera 1 (por-usuario bloqueado) va antes y tampoco toca el disco.
- Un ítem cuyo registro de log falla se cuenta igualmente en el resumen (se movió de verdad) y la ejecución aborta con `log_unavailable`; los no procesados no se registran. Si coinciden fallo de log y aborto de Papelera, gana `log_unavailable`.
- El aviso de progreso final se omite si coincide con el último aviso por conteo (100 ítems: 32, 64, 96, 100; 64 ítems: 32, 64) y no se emite con 0 procesados.
- Cancelación (5.6): la tarea decía "bandera puesta por el observer tras el ítem 40", pero el observer solo recibe avisos cada 32 ítems; se probaron (a) observer que levanta la bandera al aviso 32 (32 entradas, 68 no procesados) y (b) un *hook* de la Papelera que la levanta mientras se mueve el ítem 41 (41 entradas: el ítem en curso termina y se registra, scenario "a lo sumo 41"). Bandera levantada durante el último ítem: `completed`.
- `FakeFileSystem` gana `with_stale_listing` (el listado informa tamaño y fecha viejos; `metadata` los reales) para el scenario "Tamaño y fecha del elemento abierto"; `FakeTrash` gana `before_move`, `succeed_without_moving` y errores `TrashError` por ruta, y solo acepta `&DeleteTarget`.
- La verificación `rg -n "DeleteTarget \{" src-tauri/src` solo encuentra el literal de construcción en `protection/service.rs` (más la firma `-> DeleteTarget {` de un helper de test, que no es un constructor).

**Desvíos de proceso TDD (registro honesto)**:
- 2.2 (`metadata` por handle de un archivo abierto): el adaptador ya leía por handle, así que los 2 tests pasaron en el primer intento (test de caracterización; no hubo RED posible sin romper producción a propósito).
- 4.1 `blocked_detail(MismatchReason)`: el test se escribió ANTES que la función pero se agregó en el mismo paso que ella, sin ejecutar el RED por separado (el resto de 4.1 sí tuvo RED ejecutado).
- 5.1 `run_cleanup`: el camino feliz se implementó con marcadores de error genéricos (`io`) para `prepare_delete`/Papelera, reemplazados en 5.2 y 5.5 con RED ejecutado en cada grupo.
- 5.7: los tests de agregación por motivo/categoría se agregaron en el REFACTOR sobre código ya existente (`Tally`, creado en 5.1) y pasaron al primer intento (caracterización).
- El test real con `trash` y ancestros fijados (que el shell pueda mover con la fijación viva) queda en 6.3; en este lote solo se verifica con dobles que la fijación está viva durante el movimiento.

**Conteos tras el lote 1**: Vitest 227 (sin cambios, no se tocó el frontend); `cargo test` lib **604 passed, 8 ignored** (+79 frente a 525); integración sin cambios (5 + 5 + 9 + 21); fmt y clippy `-D warnings` verdes; `package-lock.json` y `src-tauri/Cargo.lock` sin cambios; `openspec validate --strict` válido.

### Lote 2 (grupo 6): adaptador de Papelera

**Decisiones de implementacion**:
- `adapters/windows/trash_error.rs`: `from_hresult(i32)` (tabla de 1.2 mas `HRESULT_FROM_WIN32(32|33|2|3)`) y `map_trash_error(&trash::Error)`. Las variantes de `trash::Error` no previstas se reducen a `Other("unsupported_error")`. `CouldNotAccess` (no existe o sin permiso; la tarea no lo especificaba) se mapea a `Other("could_not_access")` y NO a `NotFound`, para no tratar como benigno un acceso denegado. La descripcion del sistema nunca se propaga.
- `adapters/windows/trash.rs::WindowsTrash`: hilo `pcopt-trash` por instancia, canal de pedidos + un canal de respuesta por pedido; `Drop` cierra el canal y une el hilo. Backend inyectable con `with_backend` (`pub(crate)`) para los tests de panico y errores; `new()` usa `trash::delete` (nuestro codigo nunca llama `delete_all`; `trash::delete` es internamente `delete_all` de un solo elemento, asi que "el primer error" es el unico).
- Para reusar `make_junction` en `trash_tests.rs`: `fs::tests` pasa a `pub(super)` y `make_junction` a `pub(in crate::adapters::windows)` (solo cfg(test)).
- 6.5: no existia un helper para quitar bloques `#[cfg(test)]`; se agrego en `tests/architecture.rs` (`strip_cfg_test_items`, sobre el texto ya sin comentarios). Archivos excluidos: `*_tests.rs`, `*_contract.rs`, `fakes.rs`, `test_util.rs`, `test_support.rs`. El test escanea toda `src/`; la unica coincidencia previa era `fs_tests.rs` (excluido).

**Desvios de proceso TDD**: 6.1, 6.2 y 6.5 se escribieron implementacion y tests en el mismo paso; el RED se obtuvo saboteando a proposito la implementacion (mapeo anulado / panico mapeado a `Ok` / filtro de tokens anulado), viendo fallar los tests y restaurando. Los tests reales de 6.3 pasaron a la primera (caracterizacion del comportamiento del shell).

**6.3, corrida real** (terminal NO elevada, `--ignored`, un test por vez y solo los de nombre `real_trash_*` salvo el de D-12; solo archivos `pcopt-test-*` dentro de un `TempDir`): los 5 tests de Papelera real pasaron: archivo movido y ausente de su ruta; ancestros fijados (el ancestro no se puede renombrar mientras la fijacion vive y aun asi el shell mueve el archivo); archivo abierto sin `FILE_SHARE_DELETE` -> `InUse` y sigue en su lugar; junction puesta en la ventana residual -> se mueve la junction, el destino y su contenido quedan intactos; ruta > 260 caracteres -> **`Ok(())`** (se movio; `PathTooLong` no se observo). El test de symlink (`#[ignore]` propio) se intento y fallo por `ERROR_PRIVILEGE_NOT_HELD` (1314): esta terminal no tiene el privilegio; queda sin verificar. Archivos que quedaron en la Papelera real: `pcopt-test-moved.txt`, `pcopt-test-pinned.txt`, `pcopt-test-long.txt` y la junction `pcopt-test-swapped.txt` (solo el enlace). `pcopt-test-inuse.txt` NO se movio (sigue en su tempdir, que se borro al terminar).

**6.4 (solo codigo)**: escrito `real_trash_should_refuse_with_bin_unavailable_and_keep_the_file_when_the_recycle_bin_is_disabled` con `#[ignore = "requiere la Papelera de C: deshabilitada por el dueño"]`; compila, NO se corrio y la tarea sigue sin marcar (la ejecuta el orquestador con el dueño).

**6.6 revision de `unsafe`**: cantidad de `unsafe {` por archivo identica a HEAD (fs 2, fs_tests 1, known_folders 4, privileges 22, recycle_bin 1): 0 bloques nuevos; `trash.rs` y `trash_error.rs` no tienen `unsafe`. Comentario de `[lints.cargo]` actualizado (trash ya tiene consumidor). clippy `-D warnings` verde, `Cargo.lock` y `package-lock.json` sin cambios.

**Conteos tras el lote 2**: lib 612 passed, 15 ignored (antes 604 / 8: +8 activos, +7 ignorados); `architecture` 9 (antes 5, +4); resto igual; fmt, clippy y `openspec validate --strict` verdes.

### Lote 3 (grupo 6, D-12): compuerta FALLÓ con `trash` solo → preflight del registro (B)

**Compuerta D-12: FALLÓ con `trash` solo** (2026-11-14, tarea 6.4 ejecutada por el orquestador con autorización del dueño): con `HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\BitBucket\Volume\{55d47c52-c40e-45dd-8616-c6f1342ecb74}\NukeOnDelete = 1` (Papelera de `C:` deshabilitada), `trash::delete` (trash 5.3.0; `IFileOperation` con `FOF_NO_UI | FOF_ALLOWUNDO | FOF_WANTNUKEWARNING`) devolvió `Ok(())` y el archivo de prueba fue **borrado de forma permanente** (confirmado ausente de la Papelera), sin diálogo. Se detuvo el apply; el dueño decidió D-12 B (gobernanza HIGH) y mantener D-08 A. El aborto de D-17 al primer `BinUnavailable` no se dispara porque el crate no devuelve error.

**Política real leída de `C:`** (test ignorado de solo lectura `real_policy_should_print_the_values_read_for_the_system_volume`; no se escribe nada): `NoRecycleFiles` en HKLM y en HKCU = no existe; `BitBucket\NukeOnDelete` global = **no existe** en esta máquina (el adaptador lo lee igual y trata `!= 0` como deshabilitada; no se pudo confirmar que Windows lo honre); `Volume\{GUID}\NukeOnDelete` = 0; `MaxCapacity` = 24907 → `Enabled { max_capacity_bytes: Some(26116882432) }`. **Unidad de `MaxCapacity`: MB**, inferida: `C:` mide 456868 MB y 24907 MB es el 5,45 % del disco (en KB serían 24 MB, absurdo para una Papelera); no se pudo leer la ventana de propiedades de la Papelera desde el agente, por eso se pide al dueño confirmar el valor en pantalla. Si la clave del volumen no existiera, la capacidad queda desconocida (no se omite por tamaño) y la disponibilidad depende solo de los demás valores; esa situación no se observó y no se escribió el registro para provocarla.

**Código del lote 3**: puerto `domain/ports/recycle_bin_policy.rs` (decisión pura + `check_move` / `check_for_move` + doble), `Trash::preflight` (puerto) usado una vez por `run_cleanup`, adaptador `adapters/windows/recycle_bin_policy.rs`, y control en `WindowsTrash::move_to_trash` antes de cada `trash::delete` (constructor inyectable `with_backend_and_policy`). `unsafe`: 3 bloques nuevos, todos en `recycle_bin_policy.rs` (`RegGetValueW`, `GetVolumePathNameW`, `GetVolumeNameForVolumeMountPointW`), cada uno con `// SAFETY:`; feature `Win32_System_Registry` habilitada sin cambios en `Cargo.lock`. El test de la compuerta (`real_trash_should_refuse_with_bin_unavailable_and_keep_the_file_when_the_recycle_bin_is_disabled`) mantiene su contrato y NO se corrió en este lote; la tarea 6.4 queda para que el orquestador la repita con el registro alternado.

**Conteos tras el lote 3**: lib 632 passed, 16 ignored (antes 612 / 15: +20 activos, +1 ignorado de solo lectura); `architecture` 9, resto igual; Vitest 227 sin cambios; fmt, clippy `-D warnings`, `Cargo.lock` y `package-lock.json` sin cambios, `openspec validate --strict` válido.

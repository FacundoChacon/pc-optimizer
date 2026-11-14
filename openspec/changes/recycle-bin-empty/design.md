# Design

> Gobernanza **HIGH**: es la **única** acción permanente del producto (RN-LIM-06). Las decisiones marcadas **[DUEÑO]** requieren aprobación explícita antes del apply. Las tareas asumen la opción recomendada; si el dueño elige otra, se actualizan specs y tasks antes de codear (`/opsx:update`).

## Context

Motivación: ver `proposal.md`. Estado actual relevante (observado en el código, no supuesto):

- **Medición (C-08)**: puerto `RecycleBinInfo::query(&Path) -> Result<RecycleBinUsage { items, bytes }, PortError>` (`domain/ports/recycle_bin.rs`, cuyo doc ya dice "el vaciado es C-10 y vive en otro contrato") y adaptador `WindowsRecycleBin` (`SHQueryRecycleBinW`, un bloque `unsafe`, conversión pura `usage_from_raw`). El escáner mide `"{drive}:\\"` con `drive = raw.system_drive()` (unidad de `windows_dir` del contexto crudo de protección, nunca una letra fija) y, en `mismatch`, deja `recycle_bin` en `blocked` sin consultar. `FakeRecycleBin` registra las raíces consultadas.
- **Limpieza (C-09)**: `ActiveJob` (`commands/scan.rs`) con `JobKind::{Scan, Cleanup}`, `begin_*`, `ensure_idle`, `JobGuard`; `CleanupPreviews` en memoria con ids monótonos y vencimiento; la confirmación toma `ActiveJob` **antes** de consumir el preview. `WindowsTrash` usa un hilo propio para COM (D-10 de C-09). Errores `AppError` serializados `{ kind, message }`.
- **Log**: `cleanup_log` (001) ya admite el outcome `recycle_bin_emptied` en su `CHECK`; `category` y `path` no vacíos; `scan_id` nullable; triggers append-only. `CleanupOutcome::RecycleBinEmptied` ya existe en `domain/model.rs` (sin usos). `scan_cleaned_at(scan_id, since)` deriva "escaneo limpiado" de entradas **con ese `scan_id`** (C-09 D-20): una entrada con `scan_id = NULL` no lo afecta.
- **Arquitectura**: `tests/architecture.rs::production_code_should_not_contain_permanent_deletion_calls` prohíbe en producción `remove_file`, `remove_dir`, `DeleteFileW`, `SHEmptyRecycleBin`, `SHFileOperation`, `FOF_`, `delete_all`, y su comentario anticipa "la única excepción declarada (vaciar la Papelera) llegará con su propio change y su propia excepción en este test".
- **Win32** (confirmado en el fuente de `windows` 0.62.2, `Win32/UI/Shell/mod.rs`, feature `Win32_UI_Shell` ya habilitada): `SHEmptyRecycleBinW(hwnd: Option<HWND>, pszrootpath: P1: Param<PCWSTR>, dwflags: u32) -> windows_core::Result<()>`; `SHERB_NOCONFIRMATION = 1`, `SHERB_NOPROGRESSUI = 2`, `SHERB_NOSOUND = 4`. Con raíz nula vacía **todas** las Papeleras. Comportamiento conocido: devuelve `E_UNEXPECTED` (`0x8000FFFF`) si la Papelera ya está vacía. `CoInitializeEx` está en `Win32_System_Com`, también habilitada: no cambia `Cargo.lock`.
- **UI**: `CategoryCard` ya distingue `recycle_bin` (tamaño + "Elementos: N"); `CleanupSummary` muestra `summarySpaceNote` cuando hubo movidos; `ConfirmDialog` (`<dialog>` nativo, foco en Cancelar) no admite casilla; `PrivilegeProvider` expone el contexto; catálogo `es.ts` + `errors.ts` por `kind`; `mockBackend.ts` con `failures`/`failureQueue`.

## Goals / Non-Goals

**Goals:**
- Doble confirmación **aplicada por el backend**: no existe secuencia de llamadas IPC que vacíe la Papelera sin preparar, reconocer y ejecutar en orden con el token del segundo paso.
- La confirmación vale para lo que el usuario vio (cantidad y tamaño medidos), con vencimiento corto.
- Imposible por construcción vaciar otra raíz que la del volumen del sistema, o "todas".
- Una sola llamada a `SHEmptyRecycleBinW` en el código de producción, alcanzable solo desde los comandos de vaciado (test de arquitectura).
- Dominio 100 % testeable con dobles; el adaptador real solo se ejecuta en el smoke con el dueño.

**Non-Goals:**
- Garantizar que el shell vacíe "exactamente" los elementos medidos: la API vacía lo que haya en el instante de la llamada (ver Riesgos).
- Progreso por elemento o cancelación del vaciado (la API es una sola llamada bloqueante).
- Vaciar en `mismatch` la Papelera "correcta" (la del usuario de la sesión): exigiría impersonación; se bloquea.

## Decisions

### D-01 — Protocolo de doble confirmación: tres comandos con estado en el backend **[DUEÑO]**

- **A (recomendada)**: `prepare_empty_recycle_bin()` → solicitud medida; `acknowledge_empty_recycle_bin(requestId)` → primera confirmación, devuelve `confirmToken`; `empty_recycle_bin(requestId, confirmToken)` → segunda confirmación y ejecución. El backend guarda la etapa (`Prepared` → `Acknowledged`) y rechaza toda llamada fuera de orden. Cada paso de la UI corresponde a un comando distinto, así un error de la interfaz (doble clic, estado desincronizado) no puede saltar un paso.
- B: dos comandos (`prepare` + `empty(requestId)`) con el segundo paso solo en la UI. El backend no puede distinguir una o dos confirmaciones: contradice "doble confirmación" como regla de negocio (RN-LIM-06). Rechazada.
- C: un comando con frase de confirmación tipeada. No mide antes de confirmar (no muestra tamaño) y mezcla UX con contrato. Rechazada.
- Modelo de amenaza explícito: el token **no** es un secreto frente a un webview comprometido (podría llamar los tres comandos); protege contra errores de la propia interfaz y contra llamadas sueltas. La protección frente a un webview comprometido sigue siendo la CSP y las capabilities mínimas (C-01/C-02).

### D-02 — Solicitud en memoria, una vigente, de un solo uso, vence a los 2 minutos **[DUEÑO]**

- Estado gestionado `EmptyRequests` (`Mutex<Option<Stage>>`, `Stage = Prepared(PreparedEmpty) | Acknowledged(AcknowledgedEmpty)`), análogo a `CleanupPreviews`. `requestId`: contador `u64` monótono del proceso desde 1. `confirmToken`: segundo contador `u64` independiente (nunca igual al id de la misma solicitud); su función es probar que el segundo paso ocurrió después del primero, no el secreto (D-01). Sin dependencias nuevas de aleatoriedad.
- Type-state en el dominio (`domain/recycle_bin/request.rs`): `PreparedEmpty` solo lo crea `prepare_empty` (con mismatch verificado, raíz y medición > 0); `PreparedEmpty::acknowledge(self, now, token) -> Result<AcknowledgedEmpty, EmptyRequestError>`; `run_empty(AcknowledgedEmpty, …)` lo consume por valor. Constructores privados al módulo.
- Preparar reemplaza a la solicitud anterior (en cualquier etapa). Reconocer exige `Prepared` vigente con ese id; si no → `empty_request_not_found` (incluye reconocer dos veces). Vaciar: tras los chequeos de mismatch y `ActiveJob` (que **no** consumen), se **toma** la solicitud del slot (queda vacío pase lo que pase): id distinto o slot vacío → `empty_request_not_found`; `Prepared` → `empty_not_acknowledged`; token distinto → `empty_request_not_found`; vencida → `empty_request_expired`.
- Vencimiento **2 minutos** desde la preparación (`EMPTY_REQUEST_TTL_MS`), evaluado al reconocer y al vaciar. Alternativas: 10 min como el preview de C-09 (demasiado para una acción irreversible; la medición envejece) o 30 s (puede vencer mientras el usuario lee el segundo diálogo). Al reiniciar la app no hay solicitud.

### D-03 — Solo el volumen del sistema, con un tipo que impide la raíz nula

- PA-12 (resuelta) y la nota de C-08: misma raíz que mide el escáner. Los comandos resuelven `raw.system_drive()` con `resolve_protection_context(UserFolderSource::ProcessToken, …)` como `run_junk_scan`; sin unidad → `recycle_bin_unavailable`.
- Nuevo tipo de dominio `VolumeRoot` (`domain/recycle_bin/root.rs`): `VolumeRoot::from_drive_letter(char) -> Option<Self>` (solo `A`–`Z`, normaliza a mayúscula) y `as_path() -> PathBuf` (`"C:\\"`). El puerto de vaciado solo acepta `&VolumeRoot`: pedir "todas las Papeleras" (raíz nula) o una carpeta no compila. El adaptador convierte a UTF-16 terminado en nulo y rechaza un NUL interno, como `WindowsRecycleBin`.
- Alternativa: vaciar todos los volúmenes (raíz nula). Contradice PA-12 (otros volúmenes no se limpian) y la medición mostrada (solo `C:`). Rechazada.

### D-04 — Re-medición con igualdad estricta antes de vaciar **[DUEÑO]**

- **A (recomendada)**: justo antes de llamar al shell se mide de nuevo; si `items` **o** `bytes` difieren de la solicitud → `recycle_bin_changed`, sin vaciar (la solicitud ya se consumió: la UI vuelve al paso 1 con una medición nueva). Análogo a RN-LIM-04 ("la confirmación aplica solo al preview exacto mostrado").
- B: aceptar si disminuyó (el usuario restauró algo). Más cómodo, pero un "menos elementos y más bytes" o un reemplazo con igual cantidad pasaría; y no hay forma de saber qué cambió. Rechazada.
- C: sin re-medición. Un archivo que otra app (o una limpieza que terminó) mandó a la Papelera entre el paso 1 y el 2 se destruiría sin haberse mostrado. Rechazada.
- Una medición que falla al re-medir → `recycle_bin_unavailable` sin vaciar.

### D-05 — Bloqueo en mismatch en los tres pasos

`scope_access(ctx, UserScope::PerUser)` se consulta al **inicio** de cada comando, antes de medir o tocar el slot: `Blocked(reason)` → `AppError::RecycleBinBlocked(reason)` (kind `recycle_bin_blocked`, `message` con `different_user` / `undetermined`). El `PrivilegeContext` es estable por proceso, así que en la práctica falla ya en el paso 1; repetirlo en los pasos 2 y 3 es defensa en profundidad barata (y lo exige el roadmap: "bloqueada en mismatch"). En la UI: la tarjeta de la Papelera ya está `blocked` en mismatch (C-08), el botón no se ofrece y el resumen de la limpieza muestra en su lugar el texto `es.recycleBin.blockedMismatch` ("La Papelera de esta sesión es la de la cuenta que abrió la app como administrador, no la tuya. Para vaciarla, abrí la app con tu propia cuenta.").

### D-06 — Exclusividad: tercer tipo de trabajo en `ActiveJob`

- `JobKind::EmptyRecycleBin` + `ActiveJob::begin_empty()`; `busy()` → `AppError::RecycleBinEmptyInProgress` (kind `recycle_bin_empty_in_progress`). Así `start_scan`, `preview_cleanup` y `confirm_cleanup` lo rechazan sin cambios en sus cuerpos, y un segundo vaciado también.
- `prepare_empty_recycle_bin` y `acknowledge_empty_recycle_bin` consultan `ensure_idle()` (no toman el lugar: son instantáneos); `empty_recycle_bin` toma el lugar con `begin_empty()` **antes** de consumir la solicitud (si hay otro trabajo, la solicitud se conserva, como C-09).
- Motivo crítico: vaciar mientras la limpieza mueve archivos a la Papelera los **destruiría** (pasarían de "recuperables" a "borrados" sin preview). El vaciado no es cancelable (`cancel_scan`/`cancel_cleanup` no lo afectan).

### D-07 — Adaptador `WindowsRecycleBinEmptier`: hilo propio con COM y flags del shell **[DUEÑO]**

- Archivo nuevo `adapters/windows/recycle_bin_empty.rs`, **único** con `SHEmptyRecycleBinW`. Por llamada: arranca un hilo `pcopt-empty-bin`, que hace `CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE)`, llama `SHEmptyRecycleBinW(None, raíz, flags)` y `CoUninitialize` si la inicialización tuvo éxito; el hilo del dominio espera con `join`. Todo dentro de `catch_unwind`: un pánico o un fallo de `CoInitializeEx` → `EmptyError::BackendUnavailable`. Motivo del hilo: no dejar un apartamento STA en el pool de bloqueo de Tauri (mismo razonamiento que C-09 D-10, C-14 usará MTA).
- `unsafe`: 3 bloques mínimos (`CoInitializeEx`, `SHEmptyRecycleBinW`, `CoUninitialize`), cada uno con `// SAFETY:`.
- Mapeo puro y testeable `empty_error_from_hresult(i32)`: `E_ACCESSDENIED` (`0x80070005`) → `PermissionDenied`; resto → `Other("io_0x<HRESULT>")`. La descripción del sistema nunca se propaga.
- Flags **A (recomendada)**: `SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND`: nuestras dos confirmaciones reemplazan la del shell, sin ventanas del sistema sobre la app elevada. B: sin `SHERB_NOPROGRESSUI` (ventana de progreso del shell, con su propio "Cancelar" que no podemos observar ni registrar). Rechazada. `hwnd = None`.
- Nunca se llama con 0 elementos (la preparación lo impide), lo que evita el `E_UNEXPECTED` de Papelera vacía; igual, el resultado se decide por medición (D-08).

### D-08 — Resultado decidido por medición (función pura)

`classify_empty(before: RecycleBinUsage, call: Result<(), EmptyError>, after: Result<RecycleBinUsage, PortError>) -> EmptyResult`:

| Medición posterior | Llamada | Estado | Outcome del log | `error_detail` | Liberado |
|---|---|---|---|---|---|
| `items == 0` | cualquiera | `emptied` | `recycle_bin_emptied` | `NULL` | `before.bytes` |
| bajó `items` o `bytes`, quedan elementos | cualquiera | `partial` | `recycle_bin_emptied` | `partial` | `before.bytes − after.bytes` (saturado) |
| sin cambios (o creció) | `Ok` | `failed` | `error` | `not_emptied` | 0 |
| sin cambios (o creció) | `Err(e)` | `failed` | `error` | código de `e` | 0 |
| falla | `Ok` | `unverified` | `recycle_bin_emptied` | `unverified` | `before.bytes` |
| falla | `Err(e)` | `failed` | `error` | código de `e` | 0 |

Códigos de `e`: `permission_denied`, `empty_unavailable` (mecanismo inutilizable), `io_0x<HRESULT>`. Constantes en el dominio, la UI los traduce por familia con texto de respaldo (como `cleanupReasons` de C-09).

### D-09 — Registro en `cleanup_log`: una fila, sin migración **[DUEÑO]**

- **A (recomendada)**: una entrada por vaciado pedido al shell, escrita **después** de clasificar: `ts` = fin; `category = "recycle_bin"`; `path` = raíz del volumen (`C:\`); `size_bytes` = liberado; `outcome` y `error_detail` según D-08; `scan_id = NULL` (así `scan_cleaned_at` no marca el escaneo de basura como limpiado y C-09 D-20 sigue igual). La cantidad de elementos antes/después va solo en la respuesta (no hay columna). Sin migración (el esquema sigue en v2; 003 sigue reservada a C-07).
- B: migración 003 con columnas `item_count`… Exige reconstruir una tabla `STRICT` con `CHECK` y triggers append-only y renumerar las migraciones reservadas, por un dato que la respuesta ya muestra. Rechazada.
- Los rechazos previos (mismatch, ocupado, solicitud inválida, vencida, cambiada, medición fallida) **no** se registran: no hubo acción (mismo criterio que C-09 D-16).
- Base no disponible: el comando obtiene el `Store` antes de todo; si falla, error de persistencia y no se vacía (sin auditoría no hay acción permanente).
- Si el append falla **después** de vaciar: el vaciado ya ocurrió; la respuesta devuelve el resultado real con `logged: false` y la UI avisa "No se pudo guardar el registro de este vaciado". Alternativa (devolver `AppError`) ocultaría al usuario lo que pasó. Ventana residual: si la app muere entre el vaciado y el append, queda un vaciado sin fila (documentado en Riesgos).

### D-10 — Superficie IPC **[DUEÑO]**

| Comando | Args | Respuesta |
|---|---|---|
| `prepare_empty_recycle_bin` | — | `EmptyRequestDto` |
| `acknowledge_empty_recycle_bin` | `{ requestId }` | `EmptyAckDto` |
| `empty_recycle_bin` | `{ requestId, confirmToken }` | `EmptyResultDto` |

```
EmptyRequestDto = { requestId, itemCount, sizeBytes, createdAt, expiresAt }
EmptyAckDto     = { requestId, confirmToken, itemCount, sizeBytes, expiresAt }
EmptyResultDto  = { status: "emptied" | "partial" | "failed" | "unverified",
                    itemsBefore, bytesBefore, itemsAfter: number | null, bytesAfter: number | null,
                    freedBytes, detail: string | null, logged: boolean, finishedAt }
```

camelCase, sin rutas, SIDs ni cuentas; la raíz la decide el backend (ningún argumento la acepta). Ids y token: enteros JS seguros (`0..=2^53-1`, si no `invalid_argument`). Comandos `async` con `run_blocking`. Se agregan exactamente 3 permisos `allow-*` a `capabilities/main-window.json` (patrón de C-09; sin `core:*`, plugins ni CSP).

### D-11 — Errores nuevos

`AppError` suma: `EmptyRequestNotFound` (`empty_request_not_found`), `EmptyRequestExpired` (`empty_request_expired`), `EmptyNotAcknowledged` (`empty_not_acknowledged`), `RecycleBinChanged` (`recycle_bin_changed`), `RecycleBinAlreadyEmpty` (`recycle_bin_already_empty`), `RecycleBinBlocked(reason)` (`recycle_bin_blocked`), `RecycleBinUnavailable` (`recycle_bin_unavailable`), `RecycleBinEmptyInProgress` (`recycle_bin_empty_in_progress`). Reutiliza `scan_in_progress`, `cleanup_in_progress`, `invalid_argument`, `store_*`. Cada `kind` con texto en `es.errors`. Un fallo del shell **no** es `AppError` (es `status: "failed"`).

### D-12 — "Ninguna otra ruta usa borrado permanente": tests de arquitectura

En `tests/architecture.rs` (sobre texto sin comentarios y sin ítems `#[cfg(test)]`, con los helpers existentes):
1. **Excepción de un archivo**: `FORBIDDEN_DELETION_TOKENS` se mantiene; se agrega `DECLARED_PERMANENT_EXCEPTIONS = [("SHEmptyRecycleBin", "src/adapters/windows/recycle_bin_empty.rs")]`. El token se tolera solo en ese archivo y **exactamente una vez**; en cualquier otro archivo sigue siendo violación. Se agrega `SHERB_` a los tokens prohibidos con la misma excepción.
2. **Uso restringido del contrato**: los identificadores `RecycleBinEmptier`, `WindowsRecycleBinEmptier` y `run_empty_recycle_bin` solo pueden aparecer en producción en una lista cerrada: `domain/ports/recycle_bin_empty.rs`, `domain/ports/mod.rs`, `domain/recycle_bin/*`, `adapters/windows/recycle_bin_empty.rs`, `adapters/windows/mod.rs`, `commands/recycle_bin.rs`, `lib.rs` (registro). Un futuro `scheduler`, `improvements` o `tray` que los use rompe el test (y exige un change aprobado que modifique la lista).
3. Tests unitarios del propio escáner de tokens: excepción tolerada en su archivo, detectada en otro, segunda ocurrencia detectada, comentario ignorado.
Además, garantías por tipos: `run_junk_scan`, `confirm_cleanup_run` y los demás flujos no reciben un `RecycleBinEmptier`; `run_empty_recycle_bin` solo acepta un `AcknowledgedEmpty`, que solo produce `acknowledge`.

### D-13 — Flujo de UI: dos diálogos distintos y casilla en el segundo **[DUEÑO]**

- **Entrada**: botón secundario "Vaciar Papelera…" en la tarjeta `recycle_bin` (estado `done`, `itemCount > 0` según la última medición conocida, sin escaneo/limpieza/vaciado en curso) y en `CleanupSummary` junto a `summarySpaceNote` cuando `movedCount > 0`. En mismatch, nada (D-05).
- **Paso 1** (`EmptyRecycleBinDialog`, `<dialog>` nativo, `role="alertdialog"`): título "Vaciar la Papelera"; "Se eliminarán para siempre {count} elementos ({size}) de la Papelera del disco del sistema. No se pueden recuperar." (sin letra de unidad: la respuesta no lleva rutas); botones "Cancelar" (foco) y "Continuar" (llama `acknowledge`; deshabilitado mientras está en vuelo, así un doble clic no hace nada más).
- **Paso 2** (otro `<dialog>`, otro título): "¿Eliminar para siempre?"; repite cantidad y tamaño; casilla "Entiendo que no se puede deshacer" **sin marcar al abrir**; botón `danger` "Eliminar para siempre {count} elementos" deshabilitado hasta marcarla (`aria-describedby` con el motivo); foco en "Cancelar". Cancelar/Esc/cerrar no llama nada (la solicitud vence sola).
- **Ejecución**: los diálogos se cierran; panel con `aria-live="polite"` "Vaciando la Papelera…", sin cancelar; "Buscar basura" y "Limpiar" deshabilitados. **Resultado**: mensaje por `status` (`emptied`: "Se liberaron {size}."; `partial`: "Se liberaron {size}. Quedaron {count} elementos que no se pudieron eliminar (probablemente en uso)."; `failed`: "No se pudo vaciar la Papelera. No se eliminó nada."; `unverified`: "Windows informó que la Papelera se vació, pero no se pudo comprobar."), aviso extra si `logged = false`; foco al título del resultado. La tarjeta de la Papelera muestra la medición posterior (override en el estado de la sesión hasta el próximo escaneo).
- **Errores**: `recycle_bin_changed` → "La Papelera cambió desde que la revisaste" + botón "Revisar de nuevo" (vuelve al paso 1 con una solicitud nueva); `empty_request_expired` → igual con su texto; `recycle_bin_already_empty` → "La Papelera ya está vacía" y la tarjeta pasa a 0; el resto por `kind` del catálogo.
- Alternativas para el segundo paso: B, escribir una palabra ("VACIAR") — más fricción y peor accesibilidad para una acción que el usuario ya eligió dos veces; C, un segundo diálogo sin casilla — un doble Enter/clic podría atravesar ambos pasos. Se recomienda la casilla.
- `ConfirmDialog` no cambia; el componente nuevo reutiliza `Button`, `focus.ts` y el patrón de `CleanupPreviewDialog`. Estado en `useEmptyRecycleBin` (máquina: `idle → preparing → step1 → acknowledging → step2 → emptying → result | error`) dentro del proveedor de Limpieza, para que la tarjeta y el resumen compartan el flujo.
- Textos nuevos (catálogo `es.recycleBin.*`, sin literales en componentes): `emptyAction`, `step1Title`, `step1Body`, `continue`, `step2Title`, `step2Body`, `acknowledgeCheckbox`, `confirmEmpty`, `confirmEmptyOne`, `cancel`, `emptying`, `resultTitle`, `result.{emptied,partial,failed,unverified}`, `notLogged`, `changed`, `expired`, `alreadyEmpty`, `reviewAgain`, `blockedMismatch`, `details.*` (familias de D-08) y los 8 `errors.*` de D-11.

### D-14 — Estructura de módulos

```
src-tauri/src/domain/recycle_bin/
  mod.rs        // re-exports + contrato (doc: única acción permanente, quién puede llamarla)
  root.rs       // VolumeRoot
  request.rs    // PreparedEmpty / AcknowledgedEmpty (type-state), TTL, errores de solicitud
  execute.rs    // prepare_empty (puro sobre RecycleBinInfo) y run_empty_recycle_bin (re-medición, puerto, post-medición, entrada de log)
  outcome.rs    // classify_empty + códigos de detalle
  *_tests.rs
src-tauri/src/domain/ports/recycle_bin_empty.rs   // trait RecycleBinEmptier, EmptyError
src-tauri/src/domain/ports/fakes.rs               // FakeRecycleBinEmptier (comparte estado con FakeRecycleBin)
src-tauri/src/adapters/windows/recycle_bin_empty.rs // WindowsRecycleBinEmptier + empty_error_from_hresult
src-tauri/src/commands/recycle_bin.rs (+ _tests)  // EmptyRequests, DTOs, lógica genérica + 3 comandos
src-tauri/src/commands/scan.rs                    // JobKind::EmptyRecycleBin, begin_empty
src/features/cleanup/{EmptyRecycleBinDialog,EmptyRecycleBinResult}.tsx, useEmptyRecycleBin.ts
```

Lógica de comandos genérica sobre `Store`, `RecycleBinInfo`, `RecycleBinEmptier`, `Clock` (patrón `CleanupPorts`), para probar con dobles y mock runtime sin tocar la Papelera real.

### D-15 — Tests

- **Dominio** (dobles): `VolumeRoot` (letras válidas/ inválidas, `as_path`); preparación (mismatch `different_user`/`undetermined` sin consultar al doble de medición, Papelera vacía, medición fallida, unidad desconocida, raíz pedida = `C:\`); type-state (reconocer vigente, reconocer dos veces, reemplazada, vencida en el borde exacto de 2 min); `run_empty_recycle_bin` (sin reconocer, token equivocado, vencida, cambió por `items` y por `bytes`, re-medición fallida → el doble de vaciado sin llamadas en todos esos casos); `classify_empty` con una tabla por fila de D-08 (rstest); entrada de log exacta por estado (`scan_id` nulo, `path` = raíz, detalle).
- **Persistencia**: tras un vaciado, `scan_cleaned_at` del último escaneo sigue `None` (SQLite temporal); la entrada `recycle_bin_emptied` se lee igual y sigue siendo inmutable.
- **Comandos** (mock runtime + dobles): DTOs exactos; los 8 errores; orden de chequeos (mismatch y `ActiveJob` no consumen la solicitud; todo lo demás sí); exclusividad en los 6 sentidos (escaneo/limpieza/vaciado); `logged: false` con un store que falla el append después de vaciar; baseline de capabilities (16 comandos).
- **Adaptador**: `empty_error_from_hresult` por tabla (siempre corre); hilo + `catch_unwind` con backend inyectable que entra en pánico → `BackendUnavailable` (siempre corre, sin llamar al shell); test real `#[ignore = "VACÍA la Papelera REAL de C: del usuario; correr solo en el smoke guiado con el dueño"]`.
- **Arquitectura**: D-12.
- **Frontend**: decodificadores (formas válidas e inválidas); botón en tarjeta/resumen según estado y mismatch; paso 1 (foco en Cancelar, Esc no reconoce, doble clic en Continuar = un solo `acknowledge`); paso 2 (casilla desmarcada al abrir, botón deshabilitado, Esc/Cancelar no vacían, solo el botón llama `empty` con el token); "Vaciando…" `aria-live`; resultados por estado y `logged`; `recycle_bin_changed`/`expired` → "Revisar de nuevo"; override de la tarjeta; `uiLiterals`; errores por `kind`.

### D-16 — Plan de smoke guiado (manual, con el dueño) **[DUEÑO]**

Principios: el agente **no** vacía la Papelera real por su cuenta; el dueño prepara la Papelera y hace él mismo los clics de confirmación final; instrucciones paso a paso; 10 s de aviso antes de cada lanzamiento de la UI o UAC; sin capturas de pantalla (el agente verifica por medición y por lectura de la base en solo lectura).

1. **Preparación por el dueño** (antes de cualquier ejecución real): abrir la Papelera, **restaurar todo lo que quiera conservar** (hoy contiene además 642 `pcopt-smoke-*` y 4 `pcopt-test-*` de C-09) y confirmar por escrito en el chat: "mi Papelera de C: puede vaciarse". Sin esa frase, el smoke se detiene en el paso 3.
2. **Marcadores**: el agente crea 20 archivos `pcopt-smoke-c10-*` (1 KB c/u) en una carpeta temporal y los envía a la Papelera con el shell (PowerShell, `Microsoft.VisualBasic.FileIO.FileSystem.DeleteFile(…, OnlyErrorDialogs, SendToRecycleBin)`), y mide con `SHQueryRecycleBin` (test ignorado de solo lectura de C-08) para anotar el estado inicial.
3. **Ensayo sin vaciar** (dev sin elevar, `PCOPT_DATA_DIR` temporal, aviso de 10 s): Limpieza → "Buscar basura" → la tarjeta muestra la cantidad medida; "Vaciar Papelera…" → paso 1 con la cantidad correcta → Esc (nada cambia, re-medición igual) → de nuevo → "Continuar" → paso 2 → verificar botón deshabilitado sin casilla → "Cancelar" (nada cambia). Repetir hasta el paso 2 y esperar > 2 min → confirmar → "La solicitud venció" (nada cambia).
4. **Cambio detectado**: abrir los pasos 1 y 2, el agente envía **un** marcador más a la Papelera, el dueño confirma → `recycle_bin_changed`, nada se vacía (re-medición = estado anterior + 1).
5. **Vaciado real**: el dueño pulsa "Revisar de nuevo" → paso 1 → "Continuar" → marca la casilla → "Eliminar para siempre". Verificar: resultado `emptied`, sin ventanas del shell ni sonido, medición posterior 0, una fila nueva en `cleanup_log` (`recycle_bin`, `C:\`, bytes = medidos, `recycle_bin_emptied`, `scan_id` nulo), tarjeta en 0, el escaneo sigue admitiendo "Limpiar" si tenía elegibles.
6. **Exclusividad** (opcional, sin vaciar): con 500 archivos de prueba en una limpieza en curso, verificar que "Vaciar Papelera…" no se ofrece.
7. **Elevado / release** (aviso de 10 s antes del UAC): repetir el paso 5 con 5 marcadores nuevos en el ejecutable con `requireAdministrator` y la misma cuenta (`match`), para confirmar que el vaciado elevado afecta a la Papelera del mismo usuario. Over-the-shoulder: solo si el dueño quiere (requiere otra cuenta admin); se espera `recycle_bin_blocked` sin medir.
- Alternativa: hacer el smoke en Windows Sandbox (descartable, sin tocar la Papelera del dueño). Requiere habilitar la característica y copiar el build; más aislado pero más lento y no prueba la cuenta real. Se ofrece si el dueño no quiere vaciar su Papelera.

### D-17 — Dependencias y features

Ninguna dependencia ni feature nueva: `SHEmptyRecycleBinW`/`SHERB_*` (`Win32_UI_Shell`) y `CoInitializeEx`/`CoUninitialize`/`COINIT_*` (`Win32_System_Com`) ya habilitadas. Si el apply necesitara otra feature, se agrega solo si `Cargo.lock` no cambia; si cambia, DETENER y consultar.

## Risks / Trade-offs

- [La API vacía lo que haya en la Papelera en el instante de la llamada, no "lo medido"] → re-medición con igualdad estricta (D-04) justo antes; ventana residual de milisegundos entre medir y vaciar; exclusividad con la limpieza (D-06). Aceptado: lo que otra app mande a la Papelera en esos milisegundos se pierde junto con lo que el usuario decidió eliminar.
- [**Supuesto**: con el token elevado, `SHEmptyRecycleBinW("C:\")` vacía solo `$Recycle.Bin\<SID del token>`, no las Papeleras de otros usuarios] → documentación de la Papelera por-usuario y la medición con `SHQueryRecycleBinW` del mismo token; se verifica en el smoke (paso 7) para la misma cuenta; el caso "otra cuenta" no puede ocurrir porque mismatch está bloqueado. Si el dueño quiere certeza sobre otras cuentas, se agrega un paso con una segunda cuenta local de prueba.
- [El shell podría mostrar UI a pesar de los flags] → `SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND`, `hwnd = None`; se verifica en el smoke.
- [Vaciado largo sin progreso (Papeleras de decenas de GB)] → comando asíncrono en `run_blocking`, UI no bloqueada, aviso "Vaciando…"; sin cancelación (la API no la ofrece).
- [Elementos en uso → vaciado parcial] → estado `partial` con los restantes; nunca se reintenta ni se cierran procesos (RN-LIM-05).
- [Un vaciado sin fila de log si la app muere entre vaciar y registrar] → ventana de una operación; documentado, igual que C-09.
- [`recycle_bin_changed` repetido si una app escribe en la Papelera continuamente] → el usuario reintenta; preferible a destruir lo no mostrado.
- [STA sin bomba de mensajes en el hilo del vaciado] → hilo de vida corta, una llamada; se verifica en el smoke que no se cuelga.
- [Token predecible] → no es un control de seguridad (D-01); la frontera de seguridad sigue siendo CSP + capabilities.

## Migration Plan

- Sin migración de esquema (D-09): bases v2 siguen v2.
- Capabilities: +3 permisos `allow-*` propios; sin cambios de CSP.
- Docs a actualizar en el apply: `CHANGES.md` (estado de C-10 y notas), KB 05 RN-LIM-06 (quitar "pregunta abierta", describir el protocolo), KB 06 US-010 (criterios), KB 07 (flujo de vaciado), KB 11 (fila `recycle_bin`: única acción permanente, protocolo, registro con `scan_id` nulo), KB 04 (`cleanup_log`: fila de vaciado).
- Rollback: revertir el change deja el log intacto; las filas `recycle_bin_emptied` siguen siendo válidas para el esquema 001.

## Decisiones del dueño (bloquean el apply)

**Aprobadas por el dueño el 2026-11-14**: todas según la recomendación (D-01 A, D-02 = 2 min, D-04 A = igualdad estricta, D-07 A = `SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND`, D-09 A = una fila sin migración, D-10 = 3 comandos + 3 permisos `allow-*`, D-13 = entradas en tarjeta y resumen con casilla en el paso 2, D-16 = smoke sobre la Papelera real del dueño, solo después de que la prepare y dé confirmación escrita "mi Papelera de C: puede vaciarse").

| ID | Decisión | Recomendación |
|---|---|---|
| D-01 | Protocolo de doble confirmación | 3 comandos con estado en el backend (preparar → reconocer con token → vaciar) |
| D-02 | Vida de la solicitud | En memoria, una vigente, un solo uso, vence a los 2 min |
| D-04 | Re-medición | Igualdad estricta de cantidad y tamaño; si cambió, `recycle_bin_changed` y volver al paso 1 |
| D-07 | Flags del shell | `SHERB_NOCONFIRMATION \| SHERB_NOPROGRESSUI \| SHERB_NOSOUND`, hilo propio con COM |
| D-09 | Registro | Una fila sin migración (`recycle_bin`, raíz, liberado, outcome por D-08, `scan_id` nulo); `logged:false` si falla tras vaciar |
| D-10 | Superficie IPC | 3 comandos nuevos + 3 permisos `allow-*` |
| D-13 | UI | Botón en tarjeta y en resumen; paso 2 distinto con casilla obligatoria |
| D-16 | Smoke | Dueño restaura lo que quiera y confirma por escrito; marcadores `pcopt-smoke-c10-*`; clics finales del dueño; Windows Sandbox como alternativa |

## Desvíos respecto de CHANGES.md / KB

- `CHANGES.md` C-10 "Registro en `cleanup_log` con outcome `recycle_bin_emptied`" → también `error` para un vaciado fallido, y `recycle_bin_emptied` con detalle `partial`/`unverified` (D-08/D-09).
- KB 05 RN-LIM-06 y KB 11 todavía dicen "pregunta abierta" (PA-02 ya resuelta) → se actualizan en el apply.
- KB 11 §Categorías: `recycle_bin` "solo medida… sin modificar" describe la **detección** (sin cambios); el vaciado es una acción aparte con su propio flujo.
- KB 11 "se protegen también `$Recycle.Bin` (salvo la acción de vaciar)": el vaciado no pasa por `path-protection` (no hay rutas de elementos); la regla de protección sigue igual para la limpieza.

## Open Questions

- Texto exacto de los mensajes del catálogo: se ajusta con la revisión UX (tarea 8.7) sin cambiar specs.

## Apply notes

### 1.1 Linea base (2026-11-14, rama `feat/recycle-bin-empty`, terminal no elevada)

| Comando | Resultado |
|---|---|
| `npx vitest run` | 317 passed (16 archivos) |
| `cargo test --locked` | lib 667 passed, 16 ignored; `architecture` 9; `manifest` 5; `no_network_crates` 9; `security_baseline` 21 |
| `cargo fmt --check` | limpio |
| `cargo clippy --all-targets --all-features --locked -- -D warnings` | limpio |
| `npm run build` | verde |

### 1.2 API de Win32 confirmada

Fuente: `windows-0.62.2` en `~/.cargo/registry` (la skill `winmd-api-search` indexa metadatos WinRT/`Windows.*`, no la API Win32 clasica del shell; se cargo y no aplica a `SHEmptyRecycleBinW`, por eso la verificacion fue contra el fuente del crate).

| Simbolo | Valor / firma | Modulo / feature |
|---|---|---|
| `SHEmptyRecycleBinW` | `unsafe fn(hwnd: Option<HWND>, pszrootpath: P1: Param<PCWSTR>, dwflags: u32) -> windows_core::Result<()>` | `Win32::UI::Shell` (`Win32_UI_Shell`, ya habilitada) |
| `SHERB_NOCONFIRMATION` / `SHERB_NOPROGRESSUI` / `SHERB_NOSOUND` | `1u32` / `2u32` / `4u32` | `Win32::UI::Shell` |
| `CoInitializeEx` | `unsafe fn(pvreserved: Option<*const c_void>, dwcoinit: COINIT) -> HRESULT` | `Win32::System::Com` (`Win32_System_Com`, ya habilitada) |
| `CoUninitialize` | `unsafe fn()` | `Win32::System::Com` |
| `COINIT_APARTMENTTHREADED` / `COINIT_DISABLE_OLE1DDE` | `COINIT(2)` / `COINIT(4)` | `Win32::System::Com` |
| `E_ACCESSDENIED` / `E_UNEXPECTED` | `0x80070005` / `0x8000FFFF` | `Win32::Foundation` |

`git diff --exit-code src-tauri/Cargo.lock` limpio tras la confirmacion y tras todo el grupo de dominio (D-17: ninguna feature nueva).

### Desvios y decisiones de apply

- `rstest` NO esta en las dev-dependencies y el change prohibe dependencias nuevas: las tablas parametrizadas (4.1) son un arreglo de filas con nombre recorrido en un solo test, con el nombre de la fila en el mensaje de aserto.
- La logica de etapas (`Prepared`/`Acknowledged`, id, token, vencimiento) vive en el dominio (`recycle_bin::request::EmptySlot`) y el estado gestionado de `commands/recycle_bin.rs` solo envuelve un `Mutex<EmptySlot>`: asi se prueba sin Tauri.
- `FakeRecycleBin` pasa a guardar su estado en un `Arc<Mutex<..>>` compartido con `FakeRecycleBinEmptier` (`bin.emptier(AfterEmpty::..)`).
- Adaptador (6.x): el backend inyectable devuelve `BackendError { Init, Hresult(i32) }` (no un `EmptyError` ya traducido) para que los tests prueben el mapeo real de `HRESULT` por el adaptador. El hilo usa `std::thread::scope` (sin `'static`). Conteo de `unsafe {` por archivo: `recycle_bin_empty.rs` 0 -> 3 (CoInitializeEx, la llamada del shell, CoUninitialize), el resto sin cambios; `Cargo.lock` sin cambios. Test real `real_empty_should_leave_the_system_recycle_bin_empty` escrito, listado como `ignored`, NO ejecutado.
- Arquitectura (7.x): `DECLARED_PERMANENT_EXCEPTIONS` lleva un maximo de ocurrencias por token: `SHEmptyRecycleBin` = 1 y `SHERB_` = 3 (las tres banderas), solo en `src/adapters/windows/recycle_bin_empty.rs`. RED real comprobado: una copia temporal de la llamada y del identificador `RecycleBinEmptier` en `src/commands/` hizo fallar `production_code_should_not_contain_permanent_deletion_calls` y `permanent_empty_contract_should_only_be_used_by_the_empty_flow` (archivos temporales eliminados).
- Comandos (8.x): `EmptyPorts.store` exige solo `CleanupLogStore` (permite un doble que falla el append). Los tres comandos obtienen el `Store` antes de todo, tambien `prepare` y `acknowledge` (sin base no hay auditoria y se falla en el paso 1, no tras dos confirmaciones). El token parte de `2^32` (contador independiente) para que nunca coincida con el id de su solicitud. Capability: 16 permisos `allow-*` propios y 0 `core:*`; `tauri.conf.json` y `Cargo.lock` sin cambios; 3 `*.toml` autogenerados nuevos. Pruebas IPC (mock runtime) solo ejercen mismatch, solicitud inexistente, argumentos invalidos y base corrupta: ninguna mide ni vacia la Papelera real.
- Frontend (9.x): la maquina de estados (`useEmptyRecycleBin`) usa una referencia sincrona de la fase y un contador de secuencia: un doble clic en "Continuar" reconoce una sola vez y una respuesta tardia de un paso cancelado se ignora. Los textos de `recycle_bin_changed`, `empty_request_expired` y `recycle_bin_already_empty` viven en `es.errors.*` (no se duplican en `es.recycleBin.*`). "Continuar" es secundaria (la unica primaria de la pantalla sigue siendo Limpiar / Buscar basura; el foco inicial esta en Cancelar); el paso 2 usa el boton `danger` con casilla. `ipc.test.ts` y `mockBackendEmpty.test.ts` cubren decodificadores y el backend simulado de 3 pasos. Se actualizaron dos tests de C-09 que afirmaban que no habia ninguna accion "vaciar" en la pantalla: ahora afirman que la unica es "Vaciar Papelera…" (su propio flujo) y que no aparece dentro del preview.
- Revision UX (9.6, ux-ui-revisor, solo lectura sobre el codigo; no hay URL que recorrer): sin hallazgos de severidad Alta. Descartados por ser de gusto o ya cubiertos: "Continuar" sin estilo primario (decision de diseno: una sola primaria por pantalla y foco en la accion segura), region `aria-live` durante la medicion (ya se anuncia tambien por el `Announcer`), contraste (cubierto por `contrast.test.ts`, sin colores nuevos). Verificado: foco inicial en Cancelar en ambos pasos, retorno del foco a "Vaciar Papelera…", casilla sin marcar al abrir y `aria-describedby` con el motivo del boton deshabilitado, foco al titulo del resultado.

### 10.1 Conteos finales (terminal NO elevada) frente a la linea base

| Comando | Linea base (1.1) | Final |
|---|---|---|
| `npx vitest run` | 317 passed (16 archivos) | **366 passed** (19 archivos) |
| `cargo test --locked` lib | 667 passed, 16 ignored | **736 passed, 17 ignored** (+1 ignorado: el test real del vaciado, no ejecutado) |
| `architecture` / `manifest` / `no_network_crates` / `security_baseline` | 9 / 5 / 9 / 21 | **17** / 5 / 9 / **22** |
| `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `npm run build` | verdes | verdes |
| `openspec validate recycle-bin-empty --strict` | - | valido |

`package-lock.json`, `src-tauri/Cargo.lock`, `tauri.conf.json` y `src-tauri/src/adapters/sqlite/migrations` sin cambios.

### 10.2 Smoke guiado: pendiente

Las instrucciones paso a paso estan en `openspec/changes/recycle-bin-empty/smoke-guide.md` (aviso de 10 s antes de cada lanzamiento de UI o UAC, sin capturas, compuerta escrita `mi Papelera de C: puede vaciarse`). NO se ejecuto: lo corren el agente principal y el dueño. Resultados por anotar aqui.


# Design

> Gobernanza **HIGH**. Las decisiones marcadas **[DUEÑO]** requieren aprobación explícita del dueño antes del apply. Las tareas asumen la opción recomendada; si el dueño elige otra, se actualizan specs y tasks antes de codear (`/opsx:update`).

## Context

Motivación: ver `proposal.md`. Estado actual relevante (observado en el código, no supuesto):

- **Protección** (C-03): `PathProtection::new(&fs, Result<ProtectionContext, ContextError>)` con `check`, `session()` (memo de ancestros para escaneos masivos), `reverify` y `prepare_delete`. La decisión es `Allowed(CheckedPath{path, kind, size_bytes, identity, rule})`, `SkippedCloud{path}` o `Protected(reason)` con `reason.code()` estable. La capa solo **veta**: el doc de `rules.rs` dice explícitamente que *qué* se limpia lo decide la whitelist de C-08. La lista cerrada de excepciones ya incluye `Windows\Temp`, `SoftwareDistribution\Download`, `Windows\Minidump`, `ProgramData\…\WER\Report{Archive,Queue}` y las cachés `Cache`/`Code Cache`/`GPUCache` (Chromium) y `cache2` (Firefox). `C:\Windows\Logs` **no** está habilitado (scenario explícito en el spec `path-protection`).
- **Privilegios** (C-05): `PrivilegeContext` gestionado por la app (calculado una vez), `scope_access(ctx, UserScope)` y `restrict_protection_context(raw, ctx)`. `resolve_protection_context(UserFolderSource::ProcessToken, app_dirs)` (adapter) documenta que C-08 DEBE pasar por `restrict_protection_context` antes de `from_raw`.
- **Puertos**: `FileSystem` (`read_dir` no sigue reparse points y trae `modified` = última escritura, sin identidad; `metadata`/`canonicalize`), `Trash` (solo mover), `Clock`, `ScanStore`/`SettingsStore`/`CleanupLogStore`. Dobles en `ports/fakes.rs` (`FakeFileSystem` registra cada operación con `calls()`), baterías de contrato `store_contract.rs` y `fs_contract.rs`.
- **SQLite**: solo `001_init.sql` (tablas `settings`, `scan`, `cleanup_log`); `MIGRATIONS` exige versiones contiguas desde 1 (test `real_migrations_should_be_contiguous_from_one_with_sql`). `scan.kind` ya admite `junk`; retención de 50 escaneos que nunca borra `running`. `foreign_keys = ON`.
- **IPC**: comandos finos `async` con `run_blocking`; `APP_COMMANDS` (fuente única para `build.rs` y el test de baseline) ↔ `capabilities/main-window.json`, que **no** incluye `core:default` ni `core:event:*`.
- **Frontend**: todo IPC en `src/lib/ipc.ts` con decodificadores `unknown → tipo` que lanzan `invalid_response`; pantallas registradas en `src/app/navigation.ts`; catálogo `es.ts`; `errors.ts` elige mensaje por `kind`; mocks con `mockIPC` (`src/tests/support/mockBackend.ts`). `@tauri-apps/api` 2.12.4: `mockIPC` implementa `transformCallback`/`runCallback`, así que un `Channel` puede simularse en Vitest.

## Goals / Non-Goals

**Goals:**
- Detección de solo lectura, fail-closed, 100 % testeable con dobles (dominio puro).
- Resultado persistido con lo que C-09 necesita para re-verificar (ruta real + identidad + tipo) sin rehacer el escaneo.
- Ningún permiso nuevo fuera de los 4 comandos propios.

**Non-Goals:**
- Optimizar el rendimiento del recorrido más allá del memo de ancestros de C-03 (se mide en el smoke; ver Riesgos).
- Detectar si un archivo está en uso (D-12), si un navegador está abierto o si Windows Update está descargando.
- Escaneos programados, escaneo de carpetas, reglas personalizadas.

## Decisions

### D-01 — Numeración de la migración **[DUEÑO]**

`CHANGES.md` asigna la migración 002 a C-07 y 003 a C-08, pero C-08 se implementa primero y el runner exige versiones contiguas.

- **A (recomendada)**: C-08 usa **`002_junk.sql`** (versión 2); C-07 pasará a 003. Se actualiza `CHANGES.md` en este change. Regla general: el número lo toma quien mergea primero.
- B: crear una 002 vacía "reservada" para C-07 y usar 003. Rompe la semántica "una migración = un cambio de esquema" y deja una versión sin contenido (el test de contigüidad exige SQL no vacío). Rechazada.
- C: esperar a C-07. Bloquea el camino crítico. Rechazada.

### D-02 — Modelo de persistencia **[DUEÑO]**

KB 04 define `junk_finding {id, scan_id, category, path, size_bytes, protected_flag, cleanup_rule_id}`. Se proponen columnas extra y una tabla nueva:

```sql
CREATE TABLE junk_finding (
  id             INTEGER PRIMARY KEY,
  scan_id        INTEGER NOT NULL REFERENCES scan(id) ON DELETE CASCADE,
  category       TEXT NOT NULL CHECK (category IN (<11 de la whitelist>, 'custom_rule')),
  path           TEXT NOT NULL CHECK (length(path) > 0),
  size_bytes     INTEGER NOT NULL CHECK (size_bytes >= 0),
  modified_at    INTEGER,                         -- última escritura, epoch ms (NULL si no se informó)
  protected_flag INTEGER NOT NULL CHECK (protected_flag IN (0,1)),
  skip_reason    TEXT,                            -- código estable (ProtectedReason::code o 'skipped_cloud')
  volume_serial  INTEGER,                         -- identidad (C-09: identity_changed)
  file_id        TEXT CHECK (file_id IS NULL OR length(file_id) = 32),  -- u128 en hex
  CHECK ((protected_flag = 1) = (skip_reason IS NOT NULL)),
  CHECK (protected_flag = 1 OR (volume_serial IS NOT NULL AND file_id IS NOT NULL)),
  UNIQUE (scan_id, path)
) STRICT;
CREATE INDEX idx_junk_finding_scan_category ON junk_finding(scan_id, category, size_bytes DESC);

CREATE TABLE junk_category_result (
  scan_id            INTEGER NOT NULL REFERENCES scan(id) ON DELETE CASCADE,
  category           TEXT NOT NULL CHECK (category IN (<los mismos>)),
  scope              TEXT NOT NULL CHECK (scope IN ('machine','per_user','mixed')),
  status             TEXT NOT NULL CHECK (status IN ('done','partial','blocked','unavailable')),
  block_reason       TEXT CHECK (block_reason IS NULL OR block_reason IN ('different_user','undetermined')),
  item_count         INTEGER NOT NULL CHECK (item_count >= 0),
  size_bytes         INTEGER NOT NULL CHECK (size_bytes >= 0),
  skipped_count      INTEGER NOT NULL CHECK (skipped_count >= 0),
  inaccessible_count INTEGER NOT NULL CHECK (inaccessible_count >= 0),
  CHECK ((status IN ('partial','blocked')) = (block_reason IS NOT NULL)),
  PRIMARY KEY (scan_id, category)
) STRICT;
```

Por qué:
- **Identidad** (`volume_serial`, `file_id`): sin ella C-09 no puede detectar "archivo reemplazado por otro con el mismo nombre" (`identity_changed`, spec `path-protection`). `file_id` es u128 → TEXT hex de 32 caracteres (INTEGER de SQLite es i64). `volume_serial` (u64) se guarda como i64 por reinterpretación de bits (documentado y testeado ida y vuelta).
- **`skip_reason`**: "los protegidos se persisten como omitidos" sin motivo no es accionable en modo avanzado. `skipped_cloud` se guarda como `protected_flag = 1` + `skip_reason = 'skipped_cloud'` (KB 04 solo tiene una marca; C-09 traduce con `skip_outcome`).
- **`junk_category_result`** (tabla nueva, no está en KB 04): los estados `blocked`/`unavailable`/`partial`, la Papelera (que no tiene hallazgos) y los contadores de omitidos/inaccesibles no se pueden reconstruir desde `junk_finding`. Alternativa B: devolver el resumen solo en memoria → al volver a la pantalla o reiniciar se pierde el porqué del bloqueo y C-12 no podría mostrar "Basura" sin re-escanear. Alternativa C: fila centinela de `recycle_bin` en `junk_finding` con ruta ficticia → riesgo de que C-09 intente moverla. Se recomienda la tabla.
- `custom_rule` entra en el CHECK (KB 04 lo enumera) para no reconstruir tablas STRICT en v0.3; el dominio de C-08 **no** tiene esa variante. `cleanup_rule_id` se omite hasta v0.3 (agregar una columna nullable es un `ALTER TABLE ADD COLUMN` trivial).
- `scope = 'mixed'` solo para `crash_dumps`.

### D-03 — Granularidad: un hallazgo por archivo

Opciones: (A, recomendada) un hallazgo por **archivo**; (B) uno por entrada de primer nivel de la ubicación con tamaño agregado. B es incompatible con el spec `path-protection` ("la decisión sobre un directorio no cubre su contenido"): mover una carpeta a la Papelera arrastraría descendientes no verificados (una junction interna, un hard link). Con A cada archivo tiene su decisión y su identidad; las carpetas vacías quedan (C-09 no las borra; ver Open Questions). Costo: más filas (ver Riesgos y D-04).

### D-04 — Escritura y retención de hallazgos **[DUEÑO]**

- Los hallazgos se acumulan en memoria durante el recorrido y se guardan al final en **una transacción** junto con el resumen por categoría, el `finish_scan(done)` y la purga de hallazgos de escaneos de basura anteriores (`complete_junk_scan`). Cancelado o error → `finish_scan(cancelled|error)` sin hallazgos.
- **Retención (recomendada)**: solo se conservan los hallazgos del **último** escaneo de basura `done`; los resúmenes por categoría de los escaneos anteriores se conservan (filas chicas, útiles para tendencias de C-12/C-20) y caen con la retención de 50 escaneos (cascada).
- Alternativas: B) hallazgos de los 50 escaneos (podría superar millones de filas: 50 × ~100 k); C) solo en memoria, sin persistir (C-09 no podría operar tras reiniciar y rompe el modelo de KB 04). Rechazadas.
- Consecuencia para C-09: un preview siempre se arma sobre el último escaneo `done`; `list_junk_findings` rechaza otros `scan_id` con `scan_not_found`.

### D-05 — Ubicaciones por categoría, derivadas del contexto de protección **[DUEÑO]**

Las ubicaciones salen de una tabla `const` del dominio relativa a las anclas del `ProtectionContext` ya validado (las mismas que usa la protección): nada de rutas fijas, variables de entorno leídas en el dominio ni datos del usuario. Así una ubicación siempre cae en una excepción o en terreno no protegido, y si el contexto falla no hay ubicaciones (fail-closed).

| Categoría | Ubicación (raíz) | Alcance | Filtro | Recorrido |
|---|---|---|---|---|
| `temp_user` | `user.temp` | por-usuario | última escritura ≥ 24 h (D-07) | recursivo |
| `temp_windows` | `<Windows>\Temp` | máquina | ≥ 24 h (D-07) | recursivo |
| `browser_cache_chrome` | `<Local>\Google\Chrome\User Data\<p>\{Cache, Code Cache, GPUCache}` | por-usuario | — | recursivo |
| `browser_cache_edge` | `<Local>\Microsoft\Edge\User Data\<p>\{…}` | por-usuario | — | recursivo |
| `browser_cache_brave` | `<Local>\BraveSoftware\Brave-Browser\User Data\<p>\{…}` | por-usuario | — | recursivo |
| `browser_cache_firefox` | `<Local>\Mozilla\Firefox\Profiles\<p>\cache2` | por-usuario | — | recursivo |
| `old_logs` | *(vacía, D-06)* | — | ≥ 30 días | — |
| `wu_cache` | `<Windows>\SoftwareDistribution\Download` | máquina | — | recursivo |
| `thumbnail_cache` | `<Local>\Microsoft\Windows\Explorer` | por-usuario | nombre `thumbcache_*.db` (sin distinguir mayúsculas) | **solo primer nivel** |
| `crash_dumps` | `<Local>\CrashDumps` | por-usuario | — | recursivo |
| `crash_dumps` | `<Windows>\Minidump` | máquina | — | recursivo |
| `crash_dumps` | `<ProgramData>\Microsoft\Windows\WER\ReportArchive` y `\ReportQueue` | máquina | — | recursivo |
| `recycle_bin` | Papelera del volumen del sistema (D-08) | por-usuario | — | consulta agregada |

- **Descubrimiento de perfiles de navegador**: se lista `User Data` (o `Profiles`) un solo nivel con `read_dir`; cada hijo que sea **directorio** (no reparse point) es un perfil candidato y se prueban sus tres subcarpetas exactas (`Cache`, `Code Cache`, `GPUCache`) o `cache2`. No se leen `Local State` ni `profiles.ini` (no hace falta parsear nada del navegador, y no se abren archivos de perfil). Listar `User Data` es lectura de nombres; ningún archivo de ese nivel se vuelve candidato. El `*` de la tabla coincide exactamente con el comodín de un componente de las excepciones de C-03.
- Firefox: solo `<Local>` (la caché vive ahí); `<Roaming>\…\Profiles` (logins, cookies) nunca se lista.
- `crash_dumps`: las ubicaciones son las que C-03 habilitó para esta categoría (design de C-03, tabla de excepciones). Se excluyen deliberadamente `<Local>\Microsoft\Windows\WER\*` (no listado en KB 11 ni en C-03), `<Windows>\MEMORY.DMP` y `LiveKernelReports` (protegidos; agregarlos exige delta de `path-protection`).
- Alternativa B: resolver rutas por variables de entorno (`%TEMP%`, `%LOCALAPPDATA%`) en el adapter. Rechazada: duplica la resolución, es falsificable y podría divergir del contexto que usa la protección.

### D-06 — `old_logs` sin ubicaciones en C-08 **[DUEÑO]**

KB 11 dice "logs viejos en ubicaciones conocidas" sin listarlas. Las ubicaciones típicas (`C:\Windows\Logs\CBS`, `\DISM`, `C:\Windows\Panther`, `System32\LogFiles`, `ProgramData\…`) están **protegidas** por C-03 y el spec `path-protection` usa justamente `CBS.log` como ejemplo de algo que una categoría no puede limpiar.

- **A (recomendada)**: la categoría existe (enum, CHECK, tarjeta "No disponible en esta versión") con **lista de ubicaciones vacía**. Se agrega a KB 10 una pregunta abierta "ubicaciones de `old_logs`" y a `CHANGES.md` la nota. Cuando el dueño defina la lista, un change posterior agrega la delta de `path-protection` (nuevas excepciones) + las ubicaciones. Fuente de edad ya decidida: **última escritura** (`LastWriteTime`, la que trae `read_dir`); el último acceso suele estar deshabilitado y la creación cambia al copiar.
- B: incluir en C-08 una delta de `path-protection` con excepciones `<Windows>\Logs\CBS` y `<Windows>\Logs\DISM` filtrando `*.log` ≥ 30 días. Amplía la superficie HIGH de este change, contradice un scenario vigente (MODIFIED) y `CbsPersist_*.log`/`.cab` merecen análisis propio.
- C: `*.log` ≥ 30 días dentro de los temporales. Ya los cubren `temp_*` → doble conteo. Rechazada.

### D-07 — Antigüedad mínima de los temporales: 24 h **[DUEÑO]**

KB no fija edad para `temp_user`/`temp_windows`. Un instalador en curso puede tener archivos no bloqueados en `%TEMP%` que, movidos a la Papelera, rompen la instalación. Recomendado: **solo candidatos con última escritura ≥ 24 h**; sin fecha → no candidato. Es una restricción (achica la whitelist, nunca la amplía), análoga al default de herramientas conocidas. Alternativas: 0 h (máximo espacio, más riesgo), 7 días (más conservador, deja mucho espacio). El umbral es una constante del dominio, no un ajuste.

### D-08 — Papelera: tamaño por API de shell, solo volumen del sistema **[DUEÑO]**

- Puerto nuevo `RecycleBinInfo::query(volume_root) -> Result<RecycleBinUsage { items: u64, bytes: u64 }, PortError>`; adapter `adapters/windows/recycle_bin.rs` con `SHQueryRecycleBinW` (feature `Win32_UI_Shell`, ya habilitada; firma a confirmar en el apply con `winmd-api-search`). Un único bloque `unsafe` con `// SAFETY:` (estructura `SHQUERYRBINFO` con `cbSize` inicializado, ruta UTF-16 terminada en NUL que vive durante la llamada).
- **Raíz = volumen del sistema** (`<system_drive>:\`), coherente con PA-12 (otros volúmenes protegidos en v0.1). Alternativa: `NULL` = todas las unidades → mostraría tamaño de volúmenes que C-10 no debería vaciar. C-10 debe usar la misma raíz.
- Es por-usuario: `SHQueryRecycleBinW` consulta la Papelera del usuario del token; en over-the-shoulder sería la del administrador → bloqueada por `scope_access` (no se llama).
- No se listan elementos (`$Recycle.Bin` es `volume_system_entry` protegido) ni se crean hallazgos: solo `junk_category_result`.

### D-09 — Recorrido en el dominio sobre el puerto `FileSystem` (sin `walkdir`)

- **A (recomendada)**: walker iterativo (pila explícita, sin recursión) en `domain/junk/walk.rs` usando `FileSystem::read_dir`, que ya no sigue reparse points y trae tipo, tamaño y última escritura. 100 % testeable con `FakeFileSystem` (junctions, errores, nube) y sin I/O en el dominio. Hijo `ReparsePoint` → omitido `reparse_point`, no se desciende. Error de `read_dir` → `inaccessible_count += 1`, se sigue. La raíz de cada ubicación se valida con `metadata`: `NotFound` → ubicación ausente (sin error); `ReparsePoint` o error → inaccesible.
- B: `walkdir` en un adapter (`follow_links(false)`). `walkdir` reporta junctions de Windows como directorios en algunas versiones y obliga a un segundo puerto; la dependencia queda para C-07 (`unused_dependencies` sigue en `allow`). Rechazada para C-08.
- Orden determinista (hijos ordenados por nombre plegado) para tests estables. Sin límite de profundidad: sin seguir enlaces no hay ciclos.

### D-10 — Verificación por candidato y mapeo de decisiones

Por cada archivo candidato (tras filtros de nombre/edad): `session.check(path)` de una `CheckSession` por escaneo (memo de ancestros de C-03).

| Decisión | Registro | Suma a elegibles |
|---|---|---|
| `Allowed(checked)` | `protected_flag=0`, `path = checked.path` (ruta real), `size = checked.size_bytes`, identidad | sí |
| `SkippedCloud{path}` | `protected_flag=1`, `skip_reason='skipped_cloud'` | no |
| `Protected(reason)` | `protected_flag=1`, `skip_reason=reason.code()` | no |

Se usa la ruta real de `CheckedPath` (no la listada) para que C-09 re-verifique exactamente lo que se mostró. Reparse points vistos al listar se registran como `reparse_point` sin llamar a `check` (no se resuelven). Solo cuenta como elegible un `Allowed` con `kind == File`; si entre el listado y la verificación el archivo pasó a ser otro tipo (carrera), se registra omitido con el código existente de C-03 `resolved_path_mismatch` (fail-closed, sin inventar códigos).

### D-11 — Alcance por ubicación y doble barrera en mismatch

- El alcance es **por ubicación**, no por categoría (`crash_dumps` es mixta). `UserScope` de C-05 se reutiliza.
- Barrera 1: antes de tocar una ubicación `PerUser`, `scope_access(ctx, PerUser)`; `Blocked(reason)` → no se lista ni se consulta nada; la categoría queda `blocked` (todas bloqueadas) o `partial` (alguna) con `block_reason`.
- Barrera 2: el contexto de protección del escaneo es `from_raw(restrict_protection_context(resolve_protection_context(ProcessToken, app_dirs), &priv))`: en mismatch no hay usuario actual → las anclas por-usuario no existen (D-05 no genera esas ubicaciones) y cualquier ruta del perfil queda `other_user_profile`.
- Tests: un `FakeFileSystem` con perfil poblado + contexto `mismatch` → `calls()` no contiene ninguna ruta bajo el perfil.

### D-12 — Archivos bloqueados o en uso: no se prueban al detectar **[DUEÑO]**

- **A (recomendada)**: la detección no abre archivos para probar bloqueos. Un archivo en uso aparece como elegible (si la protección lo permite) y C-09 lo reporta `skipped_locked` al intentar moverlo (RN-LIM-05). Si `metadata` falla con `InUse`, la protección ya lo deja `unresolvable_in_use` (omitido).
- B: abrir cada candidato con `FILE_SHARE_NONE` para detectar bloqueos. Invasivo (puede interferir con el proceso dueño), costoso, y el resultado caduca antes de limpiar (TOCTOU). Rechazada.
- Consecuencia de UX: el tamaño "liberable" es un máximo; el resumen de C-09 informa lo realmente liberado. La pantalla lo aclara ("Algunos archivos pueden estar en uso y se omitirán al limpiar").

### D-13 — Transporte del progreso: `Channel` de la invocación **[DUEÑO]**

- **A (recomendada)**: `start_scan(kind, on_progress: tauri::ipc::Channel<ScanEvent>)`. El canal pertenece a esa invocación (ordenado, sin difusión global), y el comando interno `plugin:__TAURI_CHANNEL__|fetch` está exento del ACL en Tauri 2.12.5 (verificado en `webview/mod.rs`): **no hay que agregar permisos `core:*`**. El comando devuelve el resumen final al terminar.
- B: eventos globales (`app.emit`) → exige `core:event:allow-listen`/`allow-unlisten` en la capability (ampliación prohibida sin aprobación) y cualquier ventana futura los recibe. Rechazada.
- Eventos: `started {scanId}`, `category {category}`, `progress {category, filesSeen, eligibleBytes}`, (fin = respuesta del comando). Throttling **por conteo** (cada 256 archivos vistos y en cada cambio de categoría) para mantener el dominio sin reloj y los tests deterministas. El dominio emite por un trait `ScanObserver`; el comando lo adapta al `Channel` (un error de envío se ignora: la UI pudo cerrarse).

### D-14 — Cancelación y concurrencia **[DUEÑO]**

- Estado gestionado `ActiveScan` (`Mutex<Option<{scan_id, cancel: Arc<AtomicBool>}>>`). `start_scan` toma el lugar o falla `scan_in_progress`; se libera siempre al terminar (guard RAII, también ante error).
- `cancel_scan(scan_id)`: si coincide con el activo, marca el flag y devuelve `Ok`; si no, `scan_not_found`. El walker consulta el flag (`&AtomicBool`, `std::sync`, permitido en dominio) antes de cada `read_dir` y cada candidato. Resultado `cancelled`: `finish_scan(cancelled)`, hallazgos descartados (D-04), la respuesta de `start_scan` trae `status: "cancelled"`.
- Alternativa: sin cancelación en C-08 (C-07 la introduce). Rechazada: un recorrido de cachés grandes puede durar decenas de segundos y la UI debe poder abortar (KB 12, Flujo 2).

### D-15 — Superficie IPC **[DUEÑO]**

| Comando | Args | Respuesta |
|---|---|---|
| `start_scan` | `{ kind: "junk", onProgress: Channel }` | `JunkScanSummaryDto` (`status: "done" \| "cancelled"`) |
| `cancel_scan` | `{ scanId }` | `null` |
| `get_last_junk_scan` | — | `JunkScanSummaryDto \| null` (último `done`) |
| `list_junk_findings` | `{ scanId, category, offset, limit (1..=500) }` | `{ total, items: [{ id, path, sizeBytes, modifiedAt \| null, skipReason \| null }] }` orden `size_bytes DESC, id ASC` |

`JunkScanSummaryDto = { scanId, status, startedAt, finishedAt, totalItems, totalBytes, categories: [{ category, scope, status, blockReason?, itemCount, sizeBytes, skippedCount, inaccessibleCount }] }` en camelCase. `kind` distinto de `junk` → `scan_kind_unsupported` (C-07 agregará `folders`). Rutas solo en `list_junk_findings` (el frontend nunca las envía de vuelta: C-09 usará IDs). Sin SID ni nombres de cuenta. Alternativa: `start_junk_scan` dedicado → se descarta por coherencia con el roadmap (`start_scan(kind)` compartido con C-07).

### D-16 — Escaneos huérfanos al abrir la base

`SqliteStore::open` ejecuta, tras migrar, `UPDATE scan SET status='error', finished_at=max(started_at, now) WHERE status='running'` (y borra sus hallazgos/resúmenes, que no deberían existir por D-04). Sin esto, un cierre a mitad deja filas `running` que la retención nunca borra. Riesgo: dos instancias simultáneas (ver Riesgos). Alternativa: hacerlo al iniciar cada escaneo → mismo riesgo, más tarde.

### D-17 — Contexto de protección por escaneo

Se resuelve **al iniciar cada escaneo** (no al arrancar la app): las carpetas del usuario pueden cambiar (TEMP redirigida) y resolver es barato. `app_dirs` = directorio de datos efectivo (el de `manage_store`, incluido `PCOPT_DATA_DIR` en dev) + carpeta del ejecutable. Se expone en `lib.rs` un estado gestionado `ScanEnvironment { data_dir }` para no recalcular. Tests de comando inyectan un `FakeFileSystem`/contexto vía la función de dominio, no vía el comando (el comando real solo se prueba con mock runtime en escenarios que no tocan el disco real: `scan_kind_unsupported`, `scan_in_progress`, `cancel_scan` sin activo, `get_last_junk_scan` vacío).

### D-18 — Estructura de módulos

```
src-tauri/src/domain/junk/
  mod.rs          // re-exports + doc de contrato para C-09
  category.rs     // JunkCategory (11), as_str/parse, scope de la categoría
  sources.rs      // tabla D-05 → Vec<JunkSource{category, root, scope, filter, depth}>
  walk.rs         // walker D-09 + filtros D-06/D-07
  scanner.rs      // orquesta: scope D-11, check D-10, papelera D-08, resumen, cancelación, observer
  model.rs        // JunkFinding, CategoryResult, JunkScanOutcome, ScanEvent
  *_tests.rs      // por archivo, con #[path] (patrón del repo)
src-tauri/src/domain/ports/recycle_bin.rs   // RecycleBinInfo + RecycleBinUsage
src-tauri/src/domain/ports/store.rs         // + JunkFindingStore; Store = … + JunkFindingStore
src-tauri/src/adapters/sqlite/migrations/002_junk.sql
src-tauri/src/adapters/windows/recycle_bin.rs
src-tauri/src/commands/scan.rs              // 4 comandos + DTOs + ActiveScan
src/features/cleanup/{CleanupScreen,CategoryCard,FindingsList,ScanProgress}.tsx
src/features/cleanup/categoryText.ts        // mapeo categoría/estado/motivo → catálogo
```

Type-state ligero: `JunkScanOutcome::{Completed(results), Cancelled}`; solo `Completed` puede pasarse a `complete_junk_scan`.

### D-19 — Errores nuevos

`AppError` suma `ScanInProgress` (`scan_in_progress`), `ScanNotFound` (`scan_not_found`), `ScanKindUnsupported` (`scan_kind_unsupported`) e `InvalidArgument` (`invalid_argument`, p. ej. `limit`). Cada `kind` con texto en `es.errors`. Errores de la base reutilizan los existentes (`store_*`). Un fallo de detección **por archivo** nunca es error (es omitido o inaccesible).

### D-20 — UI de la pantalla Limpieza

- Navegación: entrada "Limpieza" entre Inicio y Configuración; ícono nuevo `broom`/`trash`-like (`cleanup`) en `Icon.tsx` (SVG path propio, sin librería).
- Simple: estado vacío + "Buscar basura" (única acción primaria); al terminar, total liberable + grilla de tarjetas (`Card`) por categoría con nombre llano, tamaño (`format.ts`) y `StatusBadge` (icono + texto). Aviso de D-12. Sin botón "Limpiar" (C-09 lo agrega).
- Avanzado: mismas tarjetas + botón "Ver rutas" que despliega `FindingsList` (100 por página, "Cargar más"), con ruta, tamaño, fecha y motivo de omisión.
- Progreso: región `aria-live="polite"` con categoría actual y archivos vistos; "Cancelar" (`Button` secundario). Sin barras de progreso porcentuales (no se conoce el total).
- **Sin gráficos** (PA-01 abierta; la skill `dataviz` no aplica): los tamaños son texto; no se agregan barras proporcionales en C-08.
- Motivos de omisión: `es.cleanup.skipReasons` agrupa los ~40 códigos de C-03 por familia (protegido por el sistema, perfil de navegador, enlace no seguido, varios nombres, en la nube, no se pudo leer…) con texto genérico de respaldo; el código crudo se muestra al lado en avanzado.

### D-21 — Cómo se informa el mismatch en la UI **[DUEÑO]**

- **A (recomendada)**: el aviso global de C-05 (`PrivilegeBanner`) sigue igual; además cada tarjeta `blocked` muestra, en lugar del tamaño, el texto del catálogo según motivo ("La app se abrió con otra cuenta de Windows; los archivos de tu usuario no se analizan" / "No se pudo verificar la cuenta…") con `StatusBadge` `warning`; `partial` muestra el tamaño de lo escaneado + la misma nota. El botón "Buscar basura" sigue habilitado (las categorías de máquina se escanean).
- B: deshabilitar la pantalla entera en mismatch. Contradice PA-14 (bloqueo solo por-usuario). Rechazada.
- C: ocultar las tarjetas bloqueadas. Viola "estados no disponibles explícitos" (KB 12). Rechazada.

### D-22 — Dependencias y features

Ninguna dependencia nueva (Rust ni npm). `SHQueryRecycleBinW` está en `Win32_UI_Shell` (habilitada desde C-03). Si el apply descubre que hace falta otra feature del crate `windows`, se agrega solo si `Cargo.lock` no cambia; si cambia, DETENER y consultar. `walkdir` sigue sin uso (D-09).

### D-23 — Tamaño lógico

Se informa el tamaño lógico (`file_size`) que da la protección; no se calcula tamaño en disco (compresión NTFS/sparse/clusters). Puede sobrestimar lo liberable en cachés comprimidas; aceptable para v0.1 y consistente con `cleanup_log.size_bytes`.

### D-24 — Tests

- Dominio (mayoría): tabla por categoría con `FakeFileSystem` + contexto de prueba de C-03 (`protection/test_support.rs`): ubicaciones generadas por contexto, filtros de edad (con `Clock` fijo), `thumbcache_*` vs `iconcache_*`, perfiles de navegador (Default/Profile 2/archivo suelto/junction en `User Data`), cookies nunca listadas, junction/symlink no seguidos, raíz junction, hard links, nube, error de metadatos, carpeta inaccesible, mismatch (sin llamadas al perfil), contexto inválido, cancelación, progreso por conteo, solo lecturas (`calls()` sin `pin_ancestors`/Papelera).
- Persistencia: batería de contrato extendida (doble + SQLite temporal); `schema_tests` para CHECKs/cascada; migración 1→2 con datos.
- Adapter Windows: `recycle_bin` real `#[ignore = "consulta la Papelera real del usuario; correr a mano con --ignored"]`; walker real sobre `tempfile` con junction (`mklink /J` vía `std::os::windows::fs::symlink_dir` requiere privilegio → `#[ignore]` si hace falta, la junction usa el helper del `fs_contract` existente).
- Comandos: DTOs serializados exactos; mock runtime para errores; `security_baseline` (comandos ↔ capabilities).
- Frontend: decodificadores de `ipc.ts` (formas válidas e inválidas, `Channel` simulado vía `mockIPC`), pantalla (simple/avanzado, progreso, cancelación, bloqueado, no disponible, error por `kind`), `uiLiterals`.

## Risks / Trade-offs

- [Rendimiento: `check` por archivo canonicaliza y lee metadatos (abre un handle); con ~100 k archivos puede tardar decenas de segundos] → progreso + cancelación (D-13/D-14), memo de ancestros de C-03, medición obligatoria en el smoke; optimización (p. ej. reutilizar metadatos de `read_dir`) en un change posterior sin tocar la regla "cada candidato se verifica".
- [Memoria: hallazgos acumulados hasta el final (D-04), ~300 B/hallazgo → ~30 MB con 100 k] → aceptable; si el smoke muestra más de 250 k hallazgos, evaluar escritura por lotes en una transacción abierta.
- [Base grande por `junk_finding`] → retención al último escaneo (D-04) + índice compuesto.
- [Dos instancias de la app: D-16 marcaría `error` el escaneo vivo de la otra] → raro en v0.1 (sin bandeja); la instancia única llega con el tray (C-21). Documentado.
- [Archivos en uso inflan el tamaño "liberable"] → aviso en UI (D-12); C-09 informa lo liberado real.
- [Carrera entre escaneo y limpieza] → la cubre C-09 con `prepare_delete` (identidad persistida, D-02).
- [`old_logs` vacía decepciona lo prometido en KB 11] → tarjeta "No disponible en esta versión" + pregunta abierta en KB 10 (D-06).
- [Navegador abierto: su caché cambia durante el escaneo] → solo afecta números; C-09 re-verifica y omite bloqueados.
- [Windows Update descargando en `SoftwareDistribution\Download`] → detección inocua; C-09 debe evaluar (nota en `CHANGES.md` para C-09).
- [Dev sin elevar: `Windows\Temp`, `Minidump`, WER pueden ser ilegibles] → se informan como inaccesibles; el smoke se hace también con el `.exe` de release elevado.
- [Rutas con más de 260 caracteres en cachés] → `read_dir`/`canonicalize` de C-03 ya trabajan con rutas largas; test en `tempfile` con ruta > 260.

## Migration Plan

- Migración 002 aditiva (dos tablas nuevas + índice); bases v1 migran sin tocar datos. Rollback: una base v2 abierta por un binario v1 da `schema_too_new` (comportamiento existente de C-02); no hay downgrade automático (aceptable en v0.x).
- Capabilities: +4 permisos `allow-*` propios. Sin cambios de CSP.
- Docs a actualizar en el apply (tarea 1.3): `CHANGES.md` (C-07 → migración 003; nota `old_logs`; nota para C-09 sobre WU y escaneo `done` único), KB 04 (`junk_finding` real + `junk_category_result`), KB 10 (pregunta de ubicaciones de `old_logs`), KB 11 (fuentes concretas por categoría y edad de temporales).

## Decisiones del dueño (bloquean el apply)

**Aprobadas por el dueño el 2026-11-13**: todas según la recomendación (D-01, D-02, D-04, D-05, D-06 sin ubicaciones para `old_logs`, D-07 = 24 h, D-08, D-12, D-13, D-14, D-15, D-21).

| ID | Decisión | Recomendación |
|---|---|---|
| D-01 | Numeración de migración | 002 para C-08; C-07 pasa a 003 |
| D-02 | Columnas extra + tabla `junk_category_result` | Aprobar el esquema propuesto |
| D-04 | Persistencia al final + retención de hallazgos | Solo el último escaneo `done` |
| D-05 | Ubicaciones por categoría | Tabla D-05 (sin WER por-usuario ni `MEMORY.DMP`) |
| D-06 | `old_logs` | Sin ubicaciones en C-08; pregunta abierta en KB 10 |
| D-07 | Edad mínima de temporales | 24 h por última escritura |
| D-08 | Papelera | `SHQueryRecycleBinW` solo volumen del sistema |
| D-12 | Archivos en uso | No probar al detectar; C-09 los omite |
| D-13 | Transporte del progreso | `Channel` (sin permisos `core:*`) |
| D-14 | Cancelación / concurrencia | `cancel_scan` + un escaneo a la vez |
| D-15 | Superficie IPC | 4 comandos de la tabla |
| D-21 | Mismatch en UI | Banner global + nota por tarjeta bloqueada |

## Open Questions

- ¿C-09 debe ofrecer borrar carpetas que queden vacías tras mover sus archivos? No cambia C-08 (los hallazgos son archivos); se decide en C-09.
- Umbral exacto de throttling del progreso (256 archivos) — ajustable en el apply según la fluidez observada; no cambia specs.

## Apply notes

_(Completar durante el apply: firmas Win32 confirmadas, línea base de tests, conteos finales, revisión de `unsafe`, desvíos y resultados del smoke.)_

### Lote 1 (grupos 1 a 5): dominio y persistencia

#### 1.1 Línea base (antes de tocar código, terminal NO elevada)

`npm test`, `npm run build`, `cargo test --locked`, `cargo fmt --check` y `cargo clippy ... -D warnings` en verde.

| Suite | Línea base |
|-------|-----------|
| Vitest | 150 tests (10 archivos) |
| `cargo test` lib | 389 passed, 7 ignored |
| `tests/architecture.rs` | 5 |
| `tests/manifest.rs` | 5 |
| `tests/no_network_crates.rs` | 9 |
| `tests/security_baseline.rs` | 21 |

#### 1.2 Firma Win32 confirmada (`windows-0.62.2`, fuente del registro de cargo)

- `windows::Win32::UI::Shell::SHQueryRecycleBinW<P0>(pszrootpath: P0, pshqueryrbinfo: *mut SHQUERYRBINFO) -> windows_core::Result<()>` con `P0: Param<PCWSTR>`; enlaza `shell32.dll`; la envoltura convierte el `HRESULT` en `Result` (`.ok()`).
- `SHQUERYRBINFO` es `#[repr(C)]` (en x86 `packed(1)`) con los campos, en este orden, `cbSize: u32`, `i64Size: i64`, `i64NumItems: i64`. **Desvío de nombre respecto de la tarea**: el campo de la cantidad se llama `i64NumItems` (no `i64ItemCount`). Ambos son `i64`: hay que rechazar valores negativos (nunca `as` silencioso).
- `cbSize` debe inicializarse con `size_of::<SHQUERYRBINFO>()` antes de la llamada (24 bytes en x86_64 por la alineación de los `i64`).
- Feature `Win32_UI_Shell`: ya habilitada en `Cargo.toml` (C-03). No hace falta ninguna feature nueva: `Cargo.lock` sin cambios (D-22).

#### 1.3 Decisiones registradas

`CHANGES.md` (C-07 → migración 003, C-08 → 002 con `skip_reason`/identidad/`junk_category_result`, nota `old_logs`, notas para C-09 y C-10), KB 04 (`junk_finding` real + `junk_category_result` + retención), KB 10 (PA-15 "ubicaciones de `old_logs`") y KB 11 (tabla de ubicaciones concretas, edad de temporales y Papelera solo medida). Solo esos 4 archivos.

#### Hallazgos y decisiones menores del apply (sin cambiar specs)

- **`CategoryScope` de `old_logs`**: se declara `Machine`. Sin ubicaciones su estado es siempre `unavailable` (nunca `blocked`) aunque el contexto sea `mismatch`, que es lo que pide el spec ("`old_logs` informa `unavailable`"). Si el dueño define ubicaciones por-usuario (PA-15), hay que revisar este valor.
- **Papelera con contexto de protección inválido**: el spec pide que `recycle_bin` se mida igual aunque el contexto de protección falle, pero la unidad del sistema sale de ese contexto. Se usa `C:` como respaldo (`FALLBACK_SYSTEM_DRIVE` en `scanner.rs`), solo para MEDIR. El comando (grupo 7) podría pasar la unidad real del sistema si se prefiere; no cambia el dominio.
- **Doble barrera por-usuario**: además del contexto restringido (D-11, barrera 2), el escáner filtra las ubicaciones por-usuario con `scope_access` aunque el contexto de protección trajera al usuario (test `scan_should_not_walk_user_locations_in_mismatch_even_with_an_unrestricted_context`).
- **Rutas únicas por escaneo**: el escáner deduplica hallazgos por ruta (sin distinguir mayúsculas) porque la migración exige `UNIQUE(scan_id, path)`; con `TEMP` redirigido a `C:\Windows\Temp` dos categorías verían el mismo archivo y el guardado fallaría entero. Gana la primera categoría en el orden de `JunkCategory::ALL`.
- **Reparse points**: el recorrido solo informa como omitidos (`reparse_point`) los que pasan el filtro de nombre de su ubicación (p. ej. en `thumbnail_cache` solo `thumbcache_*.db`), y trata un reparse de nube como candidato para que la protección lo resuelva como `skipped_cloud`. Un reparse sin tamaño ni fecha se registra con tamaño 0.
- **Raíz de perfiles de navegador**: de `User Data` solo se listan los nombres; los hijos que no son directorios o son reparse points no se informan (ni como hallazgo ni como omitido) y nunca se tocan `Network`, `Login Data`, `Local State`, etc. (test con `calls()`).
- **Progreso**: `files_seen` y `eligible_bytes` son acumulados del escaneo completo (no por categoría); el aviso ocurre al listar cada archivo número múltiplo de 256, por lo que `eligible_bytes` no incluye aún al archivo que dispara el aviso.
- **Orden de las tareas**: el test de solo lectura (3.3) necesita el escáner (4.2); se escribió junto a él (`scan_should_only_read_and_never_pin_move_or_empty_anything`).
- **`volume_serial`** (u64) se guarda como i64 por reinterpretación de bits (`from_ne_bytes`, sin `as`) y `file_id` (u128) como 32 hex; la batería de contrato lo prueba con `0x8000_0000_dead_beef` y `u128::MAX` contra el doble y SQLite.
- **Escaneos huérfanos (D-16)**: `SqliteStore::open` los cierra con `finished_at = max(started_at, ahora)`, usando el reloj de SQLite (`strftime('%s','now')`) porque `open` no recibe un `Clock`. No aplica al doble en memoria (no hay "apertura"), por eso esos 5 casos viven solo en `store_tests.rs` y no en la batería de contrato.
- Archivos `src-tauri/permissions/autogenerated/*.toml` aparecen modificados tras compilar (solo fin de línea); no se versionan en este change.

#### Conteos tras el lote 1

| Suite | Línea base | Tras lote 1 |
|-------|-----------|-------------|
| `cargo test` lib | 389 (+7 ignored) | 493 (+7 ignored) |
| `architecture` / `manifest` / `no_network_crates` / `security_baseline` | 5 / 5 / 9 / 21 | 5 / 5 / 9 / 21 |
| Vitest | 150 | 150 |

`Cargo.lock` y `package-lock.json` sin cambios; `cargo fmt --check` y `clippy -D warnings` en verde.

### Lote 2 (grupos 6 a 9.1): adaptador, comandos, frontend y cierre

#### Backend (grupos 6 y 7)

- **Unidad del sistema (corrige la nota "Papelera con contexto invalido" del lote 1)**: se elimino `FALLBACK_SYSTEM_DRIVE` del dominio. `scan_junk` recibe ahora `system_drive: Option<char>`; el comando la obtiene con el nuevo `RawProtectionContext::system_drive()` (pura, de `windows_dir`, que el adaptador lee con `SHGetKnownFolderPath(FOLDERID_Windows)`) ANTES de restringir y validar el contexto, de modo que se mide la Papelera de la unidad real aunque el resto del contexto no sea valido. Si no se puede resolver (`None`), `recycle_bin` queda `unavailable` y nunca se consulta una letra adivinada. Specs sin cambios. `scan_junk` queda con 8 argumentos: `#[expect(clippy::too_many_arguments)]` con motivo.
- **`SHQUERYRBINFO`**: el adaptador `WindowsRecycleBin` usa `i64Size`/`i64NumItems`; la conversion es pura (`usage_from_raw`) y rechaza negativos con `PortError::Io` (sin `as`). El `HRESULT` de error se tipifica con `port_error` de C-05 (acceso denegado -> `PermissionDenied`). Test real `#[ignore]` ejecutado a mano: verde.
- **6.2**: los 3 tests de recorrido real sobre `TempDir` (archivo viejo/reciente/junction, ruta de mas de 260 caracteres, raiz ausente) pasaron sin cambiar el adaptador (`WindowsFileSystem` ya los cumplia); quedan como red de seguridad.
- **6.3 revision de `unsafe`**: `rg unsafe src-tauri/src` -> 1 bloque nuevo, solo en `adapters/windows/recycle_bin.rs`, con `// SAFETY:` (buffer UTF-16 con nulo vivo durante la llamada, `cbSize` = `size_of`, puntero de salida unico). Clippy con `undocumented_unsafe_blocks = deny` en verde.
- **`ScanEnvironment`** guarda el directorio de datos con un `Mutex` (se administra en `app_builder` y lo completa `manage_store`) para que el comando no falle en el runtime mock; las carpetas protegidas del escaneo son datos de la app + carpeta del ejecutable (`app_dirs`).
- **Respuesta de un escaneo cancelado**: `status: "cancelled"`, `categories: []`, totales 0 y `finishedAt` (no hay resumen persistido de un escaneo cancelado). El frontend debe aceptar `categories` vacias solo con `cancelled`.
- **Orden de comprobaciones de `start_scan`**: primero `kind` (`scan_kind_unsupported`), luego el lugar del escaneo (`scan_in_progress`), luego la base.
- **TDD de 7.x**: el codigo de `commands/scan.rs` se escribio antes que sus tests (desvio del ciclo estricto; los 20 tests de `commands::scan` + los de IPC en `lib.rs` se escribieron a continuacion y cubren cada escenario de las tareas). Se anota para transparencia.

# Design

> Gobernanza **HIGH**: es el primer change que mueve archivos **personales** del usuario (fotos, documentos), fuera de la whitelist de basura. Las decisiones marcadas **[DUEÑO]** requieren aprobación explícita antes del apply. Las tareas asumen la opción recomendada; si el dueño elige otra, se actualizan specs y tasks antes de codear (`/opsx:update`).

## Context

Motivación: ver `proposal.md`. Estado actual relevante (observado en el código, no supuesto):

- **Protección (C-03)**: `PathProtection::check(raw) -> ProtectionDecision` (con `session()` memoizada por escaneo) y `prepare_delete(&CheckedPath) -> Result<DeleteTarget, ProtectionDecision>`. `CheckedPath` trae `path`, `kind`, `size_bytes`, `modified` (por handle, D-27 de C-09), `identity`, `rule`. Las carpetas de contenido del usuario (Escritorio, Documentos, Descargas, Música, Imágenes, Videos) ya están en el contexto (`UserContext::content_folders() -> &[NormalizedPath]`, de `SHGetKnownFolderPath`), **sin clave** que diga cuál es cuál. Esas carpetas en sí son `protected/anchor`; su contenido es `allowed` salvo raíces protegidas, reparse, hard links > 1 o nube (`C:\Users\ana\Downloads\setup-viejo.exe` → `allowed` en el spec de `path-protection`). En mismatch, `restrict_protection_context` quita al usuario → todo el perfil es `other_user_profile`. Otros volúmenes → `unsupported_volume` (PA-12).
- **Recorrido (C-08)**: `domain/junk/walk.rs::walk_source(fs, &JunkSource, now, visitor)`: iterativo, determinista, sin seguir reparse, cancelación consultada antes de cada listado y entrada; acoplado a `JunkSource` (filtro y profundidad de basura).
- **Limpieza (C-09)**: `run_cleanup(CleanupPreview, &PrivilegeContext, protection, fs, trash, log, clock, cancel, observer)`; `CleanupItem { finding_id, category: JunkCategory, path, size_bytes, modified_at, identity }`; barrera 1 por `category.scope()`; orden por ítem: alcance → `prepare_delete` → tamaño/fecha (`changed_since_scan`) → 24 h (`recently_modified`) → `move_to_trash(&DeleteTarget)` → post-verificación → log. `PreviewSlot` (uno activo, un uso, 10 min), `PROGRESS_EVERY_ITEMS = 32`. El doc de `domain/cleanup/mod.rs` y `CHANGES.md` fijan el contrato para C-11: "construir el preview con filas propias y reutilizar `run_cleanup`; no reimplementar la ejecución".
- **Exclusividad (C-09/C-10)**: `ActiveJob` con `JobKind::{Scan(Option<ScanId>), Cleanup(PreviewId), EmptyRecycleBin}`; `cancel_scan(scanId)` y `cancel_cleanup(previewId)` actúan por tipo.
- **SQLite**: esquema v2 (001 + 002). `scan.kind` ya admite `duplicates`. `cleanup_log.category` es texto libre no vacío (sin `CHECK` de enum) → `duplicates` entra sin migrar el log. Retención: 50 escaneos (cascada de hijos), hallazgos solo del último `done`, `running` → `error` al abrir. `CHANGES.md` reserva 003 a C-07 y 004 a C-11, con la regla de C-08 D-01 "el número lo toma quien mergea primero".
- **Dependencias**: `blake3 = "=1.8.7"` fijado sin consumidor (el comentario de `[lints.cargo]` lo reserva para C-11); `windows` 0.62.2 con `Win32_Storage_FileSystem` (incluye `FILE_FLAG_OPEN_REPARSE_POINT`, `FILE_FLAG_OPEN_NO_RECALL`, `FILE_FLAG_SEQUENTIAL_SCAN`, `GetFileInformationByHandleEx`). No hay `rayon`.
- **Frontend**: navegación Inicio / Limpieza / Configuración; `CleanupPreviewDialog`, `ConfirmDialog`, `RadioGroup`, `Card`, `density.tsx` (variantes por modo), catálogo `es.ts`, `errors.ts` por `kind`, `mockBackend.ts` con `Channel` simulado.

## Goals / Non-Goals

**Goals:**
- Imposible por construcción borrar la última copia de un grupo: lo valida el dominio al previsualizar, lo re-verifica el contenido al confirmar y lo garantiza el orden de la ejecución (copia conservada verificada antes que las demás).
- Nunca leer un archivo protegido ni un marcador de nube; nunca seguir enlaces; el escaneo no escribe nada.
- Reutilizar la ejecución de C-09 sin copiarla: un único camino de Papelera, re-verificación y log.
- Dominio 100 % testeable con dobles (incluido el contenido de los archivos); el adaptador real solo sobre `tempdir`.

**Non-Goals:**
- Carpetas arbitrarias, otros volúmenes, AppData, rutas protegidas (ver proposal "Fuera de alcance").
- Rendimiento máximo (sin paralelismo; se mide en el smoke, D-24).
- Reemplazar copias por hard links o dedupe a nivel de bloque.

## Decisions

### D-01 — Alcance: las 6 carpetas de contenido del usuario actual **[DUEÑO]**

- **A (recomendada)**: las raíces son las carpetas de contenido del contexto de protección con su clave (`desktop`, `documents`, `downloads`, `music`, `pictures`, `videos`). Simple: siempre las 6. Avanzado: casillas para desmarcar (al menos una). La interfaz envía **claves**, nunca rutas. Para eso `UserContext` pasa a guardar las carpetas con su clave (`Vec<(ContentFolder, NormalizedPath)>`; refactor interno de C-03 sin cambio de comportamiento: `content_folders()` sigue devolviendo las rutas).
- Estado por raíz (spec "Carpetas anidadas o no disponibles"): se evalúa con `metadata` sin seguir reparse (no con `check`, porque la carpeta en sí es `anchor`): no existe → `unavailable/not_found`; no es directorio → `not_a_directory`; reparse → `reparse_point`; nube → `cloud`; otra unidad que la del sistema → `unsupported_volume`; contexto inválido → `context_unavailable`. Una raíz que es descendiente de otra seleccionada → `nested` (se recorre una vez, desde la de arriba); la atribución de cada archivo es a la raíz **más profunda** que lo contiene (así "Descargas dentro de Documentos" se muestra como Descargas).
- B: carpeta arbitraria con selector. Exige `tauri-plugin-dialog` (dependencia + capability nueva, regla dura: aprobación explícita) o un `IFileOpenDialog` propio por COM (más `unsafe`), además de validar la raíz (`check_rule_root` existe para C-21). Diferida a un change propio; se agrega **PA-16** en KB 10.
- C: todo `C:` (o todo el perfil). Recorre AppData, Program Files, etc.: lento, casi todo protegido, y duplicados de aplicaciones (DLL, assets de juegos) que el usuario no debe tocar. Rechazada.

### D-02 — Exclusiones dentro de las raíces **[DUEÑO]**

- **A (recomendada)**: además de no seguir reparse (C-08), no se desciende a carpetas cuyo nombre (comparado como NTFS, `fold_component`) está en la lista cerrada `.git`, `.hg`, `.svn`, `node_modules`. Son duplicados "legítimos" (dependencias repetidas entre proyectos, objetos de control de versiones) cuyo borrado rompe proyectos y que un usuario de modo simple no puede juzgar. Se cuentan como `excludedDirs`. Es restrictivo (achica, nunca amplía; RN-GLB-01).
- B: sin exclusiones (el usuario decide). Rechazada por el riesgo para usuarios comunes.
- C: lista más larga (`.venv`, `venv`, `target`, `build`, `bin`, `obj`). Nombres genéricos que también usan usuarios para carpetas propias; se deja para cuando el dueño lo pida.

### D-03 — Tamaño mínimo: 1 MiB por defecto, opciones cerradas por escaneo **[DUEÑO]**

- **A (recomendada)**: `min_size_bytes ∈ {102 400, 1 048 576, 10 485 760, 104 857 600}` (100 KiB, 1 MiB, 10 MiB, 100 MiB), por defecto 1 MiB (SU-06; la UI muestra "1 MB" con `format.ts`). Es un **parámetro del escaneo**, elegible solo en avanzado, guardado en el resultado; no es un ajuste persistido. Valida SU-06 tal como pide KB 09 ("opción configurable en modo avanzado").
- B: ajuste persistido en `settings`. Más superficie (clave nueva, validación, pantalla Configuración) sin beneficio claro. Rechazada.
- C: valor libre. Abre tamaños mínimos de 0–1 byte (miles de archivos chicos, cientos de miles de lecturas). Rechazada.

### D-04 — Elegibilidad de cada archivo al escanear

Por cada archivo listado (en este orden; nada se abre hasta el paso 5):
1. Tamaño del listado `< min_size` → se ignora (contador `belowMinSize`); evita resolver la protección de miles de archivos chicos.
2. `PathProtection::check` con la sesión memoizada del escaneo → `protected` (contador por motivo), `skipped_cloud` (contador `cloud`), `allowed(CheckedPath)`.
3. `hard_links > 1` ya da `protected/multiple_hard_links` en C-03 → se cuenta aparte como `hardLinked` (spec "Archivos con varios hard links excluidos").
4. Tamaño **por handle** (`CheckedPath.size_bytes`) `< min_size` → se ignora (el listado puede ir atrasado).
5. Solo entonces es candidato `(CheckedPath, raíz)`. Si dos candidatos tienen la misma identidad (no debería pasar sin hard links, pero es barato), queda uno.

### D-05 — Etapas y hash parcial

- Etapa 1: agrupar candidatos por `size_bytes` (por handle); descartar tamaños únicos.
- Etapa 2: hash parcial = `blake3(primeros 64 KiB ‖ últimos 64 KiB)` (RN-DUP-01, DD-13). Si `size <= 128 KiB` se lee el archivo completo una vez y ese hash vale como parcial **y** completo (no se relee). Clave `(size, parcial)`; descartar únicos.
- Etapa 3: `blake3` completo. Clave `(size, completo)`; grupos de 2+ con identidades distintas (RN-DUP-02). Hash en hex minúscula de 64 caracteres.
- Orden determinista: grupos y copias ordenados por ruta normalizada dentro de cada etapa (tests reproducibles).
- Alternativa: agregar un bloque del medio al parcial (más discriminante para archivos con encabezado y cola iguales, p. ej. imágenes de disco del mismo tamaño). No cambia el resultado (el completo decide); solo ahorra lecturas en casos raros. Se mantiene lo de la KB.
- Sin comparación byte a byte: con blake3 (256 bits) una colisión accidental es despreciable; la re-verificación de D-14 vuelve a hashear al confirmar.

### D-06 — Lectura del contenido: puerto `ContentReader` y hash en el dominio

- Puerto nuevo `domain/ports/content.rs` (spec `domain-ports`): `trait ContentReader { fn open(&self, path: &Path) -> Result<Box<dyn ContentHandle>, PortError>; }` y `trait ContentHandle { fn meta(&self) -> Result<FileMeta, PortError>; fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<usize, PortError>; }`. `meta()` se lee **del archivo abierto** (identidad, tamaño, última escritura, nube, tipo).
- Adaptador `adapters/windows/content.rs`: `CreateFileW` con `FILE_READ_DATA | FILE_READ_ATTRIBUTES`, `FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE` (nunca bloquea al usuario ni a la Papelera), `OPEN_EXISTING`, `FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_OPEN_NO_RECALL | FILE_FLAG_SEQUENTIAL_SCAN`; metadatos por handle con el mismo código que `WindowsFileSystem::metadata` (`FileAttributeTagInfo`, `FileBasicInfo`, `FileStandardInfo`, `FileIdInfo`); lectura con `ReadFile` + `OVERLAPPED.Offset` (o `SetFilePointerEx` + `ReadFile`). `ERROR_SHARING_VIOLATION`/`LOCK_VIOLATION` → `PortError::InUse`.
- Defensa en profundidad contra la nube: el dominio nunca llama a `open` para un candidato que no sea `allowed`, y además `hash_file` rechaza el handle si `meta().cloud_placeholder` antes de leer un byte. `FILE_FLAG_OPEN_NO_RECALL` evita la recuperación incluso si el atributo cambió entre medio.
- El hash se calcula en el **dominio** (`domain/duplicates/hash.rs`) con `blake3::Hasher` sobre bloques de 1 MiB: `blake3` es cómputo puro (sin E/S), no está en la lista prohibida del dominio y así el falso positivo del parcial y el "cambió durante la lectura" se prueban con el doble.
- Verificación por archivo (spec "Lectura del contenido verificada"): al abrir, `meta()` debe coincidir en identidad, tamaño y fecha con el `CheckedPath` del escaneo; al terminar de leer, `meta()` otra vez: tamaño o fecha distintos → `changedDuringScan` y se descarta. Fallo al abrir/leer → `unreadable` (por `InUse` u otro).
- B: hash en el adaptador (`fn blake3_file(path) -> [u8; 32]`). Menos código de puerto, pero el dominio no podría probar etapas ni cambios a mitad sin el disco real. Rechazada.

### D-07 — Concurrencia y cancelación: secuencial **[DUEÑO]**

- **A (recomendada)**: todo el escaneo corre en un único hilo de `run_blocking` (como C-08): un archivo a la vez, bloques de 1 MiB, cancelación (`AtomicBool` de `ActiveJob`) consultada antes de cada entrada del recorrido, antes de cada archivo y **entre bloques** (un archivo de 50 GiB se cancela en milisegundos). Amigable con discos rígidos (sin saltos de cabezal entre archivos) y sin dependencias nuevas.
- B: paralelismo (rayon o hilos propios) en la etapa 3. En SSD NVMe podría acelerar, en HDD empeora; `rayon` sería dependencia nueva (y la feature `rayon` de `blake3` cambia `Cargo.lock`). Solo si el smoke (D-24) muestra que el escaneo es inaceptable, y con nueva aprobación.

### D-08 — Migración 003 y esquema **[DUEÑO]**

Numeración: **003** (`003_duplicates.sql`), porque C-07 no está implementado y el runner exige versiones contiguas (regla de C-08 D-01: el número lo toma quien mergea primero); `CHANGES.md` pasa C-07 a 004 en este change. Si C-07 mergeara antes, esta migración se renumera a 004 sin cambios de contenido.

```sql
CREATE TABLE duplicate_scan_result (            -- uno por escaneo de duplicados terminado
  scan_id INTEGER PRIMARY KEY REFERENCES scan(id) ON DELETE CASCADE,
  min_size_bytes INTEGER NOT NULL CHECK (min_size_bytes IN (102400,1048576,10485760,104857600)),
  files_seen, below_min_size, protected_count, cloud_count, hard_linked_count,
  changed_count, unreadable_count, inaccessible_dirs, excluded_dirs,
  hashed_bytes INTEGER NOT NULL CHECK (... >= 0)            -- todos los contadores >= 0
) STRICT;
CREATE TABLE duplicate_scan_root (              -- estado por carpeta de contenido
  scan_id INTEGER NOT NULL REFERENCES scan(id) ON DELETE CASCADE,
  root TEXT NOT NULL CHECK (root IN ('desktop','documents','downloads','music','pictures','videos')),
  status TEXT NOT NULL CHECK (status IN ('done','not_selected','nested','unavailable')),
  reason TEXT,                                  -- código estable; obligatorio solo si unavailable
  files_seen INTEGER NOT NULL CHECK (files_seen >= 0),
  CHECK ((status = 'unavailable') = (reason IS NOT NULL)),
  PRIMARY KEY (scan_id, root)
) STRICT;
CREATE TABLE duplicate_group (
  id INTEGER PRIMARY KEY,
  scan_id INTEGER NOT NULL REFERENCES scan(id) ON DELETE CASCADE,
  size_bytes INTEGER NOT NULL CHECK (size_bytes > 0),
  blake3 TEXT NOT NULL CHECK (length(blake3) = 64 AND blake3 NOT GLOB '*[^0-9a-f]*'),
  file_count INTEGER NOT NULL CHECK (file_count >= 2),
  resolved_at INTEGER CHECK (resolved_at IS NULL OR resolved_at > 0),
  UNIQUE (scan_id, size_bytes, blake3)          -- cubre el índice (scan_id, size_bytes) de KB 04
) STRICT;
CREATE INDEX idx_duplicate_group_blake3 ON duplicate_group(blake3);
CREATE TABLE duplicate_file (
  id INTEGER PRIMARY KEY,
  group_id INTEGER NOT NULL REFERENCES duplicate_group(id) ON DELETE CASCADE,
  root TEXT NOT NULL CHECK (root IN ('desktop','documents','downloads','music','pictures','videos')),
  path TEXT NOT NULL CHECK (length(path) > 0),
  mtime INTEGER,                                -- última escritura por handle (epoch ms), nullable
  volume_serial INTEGER NOT NULL,
  file_id TEXT NOT NULL CHECK (length(file_id) = 32),
  kept INTEGER NOT NULL DEFAULT 0 CHECK (kept IN (0,1)),
  UNIQUE (group_id, path),
  UNIQUE (group_id, volume_serial, file_id)     -- dos copias de un grupo nunca son el mismo archivo
) STRICT;
CREATE INDEX idx_duplicate_file_root ON duplicate_file(root, group_id);
```

- Identidad con la misma codificación que `junk_finding` (volumen `u64` como `i64` por bits, id `u128` en hex de 32).
- `file_count` evita un `COUNT` por fila al ordenar por recuperable (`size_bytes * (file_count - 1) DESC, id ASC`); el store valida que coincida con las copias guardadas.
- Desvío de KB 04 (que solo lista `duplicate_group(id, scan_id, size_bytes, blake3)` y `duplicate_file(id, group_id, path, mtime, kept)`): se agregan `file_count`, `resolved_at`, `root`, identidad y las dos tablas de resultado (sin ellas el resumen no sobrevive a un reinicio). Se documenta en KB 04 en el apply.
- B: sin tablas de resultado (resumen solo en memoria). Tras reiniciar la app no se podría explicar "por qué no aparece mi carpeta". Rechazada.

### D-09 — Guardado y retención

- Guardado **al final**, atómico (`DuplicateStore::complete_duplicate_scan(scan_id, &DuplicateScanResults, finished_at)`), igual que C-08 D-04: resultado + raíces + grupos + copias + `finish_scan(done)` + purga de grupos de escaneos de duplicados anteriores (se conserva su `scan` y su resultado). Cancelado o con error → no se guarda nada (el escaneo queda `cancelled`/`error`).
- Retención: la de 50 escaneos (cascada) + "solo el último `done` tiene grupos". Escaneos `running` al abrir → `error` y sin grupos (delta de `local-store`).
- PA-13 (retención final) sigue abierta; esto no la empeora (un solo juego de grupos vivo).

### D-10 — Modelo de decisión: nada preseleccionado, `kept` solo al resolver **[DUEÑO]**

- **A (recomendada)**:
  - Ningún grupo llega con copia elegida (ni en el DTO ni en la UI). Las elecciones viven en el **frontend** (proveedor de la pantalla) hasta confirmar; cerrar la app las descarta.
  - Simple: un `RadioGroup` por grupo ("¿Cuál querés conservar?") → conserva exactamente una; las demás se muestran "va a la Papelera" solo **después** de elegir.
  - Avanzado: casillas "Conservar" por copia; un grupo cuenta como decidido cuando tiene ≥ 1 marcada y ≥ 1 sin marcar; la UI no deja desmarcar la última (spec "Detalle en modo avanzado").
  - El backend recibe `decisions: [{ groupId, keepFileIds: number[] }]` y deriva las copias a eliminar (`grupo \ conservadas`), validando todo (D-12). `kept` en la base solo lo escribe la **resolución** posterior a la ejecución (D-17); un test de arquitectura verifica que la única sentencia SQL que asigna `kept` está en `resolve_duplicate_groups`.
- B: persistir las elecciones a medida que el usuario las hace. Un clic no es una decisión confirmada y quedaría "conservada" sin confirmación (contradice RN-DUP-03 / "kept solo por decisión"). Rechazada.
- C: elegir en lote ("conservar siempre la de Documentos"). Útil con cientos de grupos, pero es una regla de auto-selección; diferida (avanzado, change propio si el dueño la pide).

### D-11 — "Elección guiada" sin recomendación **[DUEÑO]**

- **A (recomendada)**: guía = (1) una línea de ayuda fija por grupo ("Elegí la copia que querés conservar. Las demás van a la Papelera y podés recuperarlas desde ahí."); (2) por copia, datos neutrales: nombre de la carpeta de contenido ("Descargas"), ruta relativa a ella (`Viajes\2019\foto.jpg`) y fecha de modificación; (3) el grupo muestra nombre, tamaño, cantidad de copias y espacio recuperable. Sin insignia "recomendada", sin "la más vieja" resaltada, sin preselección (DD-13 rechazó "auto-elegir la más vieja").
- B: insignia informativa "más antigua"/"más reciente". Empuja la decisión hacia una heurística que DD-13 descartó. Rechazada.
- Sin vista previa del archivo ni "abrir ubicación": exigirían `fs`/asset protocol o `shell` en el webview (regla dura). Si el usuario necesita mirar el archivo, la ruta está visible en avanzado.

### D-12 — Preview de duplicados sobre el `PreviewSlot` de C-09 **[DUEÑO]**

- **A (recomendada)**: `build_duplicate_preview(input) -> Result<DuplicateCleanupPreview, DuplicatePreviewError>` (puro, sin disco):
  1. `scanId` debe ser el último escaneo de duplicados `done` → si no, `scan_not_found`.
  2. Decisiones no vacías, sin grupos repetidos, ids en `0..=2^53-1` → si no, `invalid_argument`.
  3. Cada grupo existe en ese escaneo → si no, `duplicate_group_not_found`; no resuelto → si no, `duplicate_group_resolved`.
  4. `keepFileIds` vacío → `duplicate_keep_required` (RN-DUP-04, "intento de borrar todas las copias"); con ids repetidos o ajenos al grupo, o que conservan todas → `invalid_argument`.
  5. Ítems = copias a eliminar, como `CleanupItem { finding_id: file.id, category: CleanupCategory::Duplicates, path, size_bytes, modified_at: mtime, identity }`, ordenados por grupo (orden de la decisión) y dentro por ruta.
  6. Guarda de 24 h con la fecha persistida (por handle): recientes se excluyen y se informan (`recentCount/recentBytes`), igual que C-09 D-04.
  7. Plan de verificación: por grupo, el hash completo y las copias conservadas (ruta, identidad, tamaño, fecha).
  - `DuplicateCleanupPreview` = `CleanupPreview` (ítems + totales) + plan. Se guarda en el **mismo** `PreviewSlot` generalizado (`ActivePreview::{Junk, Duplicates}`): un solo preview activo de cualquier tipo, un uso, 10 min. `confirm_cleanup` con un id de preview de duplicados (o al revés) → `preview_not_found`.
- B: slot y TTL propios de duplicados. Dos previews vivos a la vez y dos relojes de vencimiento para el mismo `ActiveJob`; sin beneficio. Rechazada.
- Un grupo cuyas copias a eliminar son todas recientes queda en el preview con 0 ítems; si el preview total queda en 0 ítems, confirmar falla `nothing_to_clean` (C-09).

### D-13 — Integración con `run_cleanup`: categoría generalizada + gancho por ítem **[DUEÑO]**

- **A (recomendada)**:
  - `CleanupCategory::{Junk(JunkCategory), Duplicates}` reemplaza a `JunkCategory` en `CleanupItem`, `PreviewCategory` y el `Tally`; `as_str()` (`duplicates` para la nueva) y `scope()` (`Duplicates` es `PerUser` → barrera 1 en mismatch, spec `cleanup`). Los DTO de basura no cambian (solo usan variantes `Junk`).
  - `run_cleanup` recibe un `guard: &mut impl ItemGuard` y `progress_every: u64`. `ItemGuard::verify(&mut self, item, &DeleteTarget, cancel) -> GuardVerdict { Proceed, Skip(CleanupOutcome, &'static str), Interrupted }` corre **después** de la guarda de 24 h y **antes** de `move_to_trash`, con la fijación viva. `Interrupted` → el ítem no se registra, la ejecución termina `cancelled` y cuenta ese ítem entre los no procesados. La basura pasa `NoGuard` (siempre `Proceed`) y `progress_every = 32`; duplicados pasa `DuplicateContentGuard` y `progress_every = 1` (pocos ítems, grandes).
- B: verificar el contenido **antes** de llamar a `run_cleanup`. Fuera de la fijación (ventana TOCTOU entre hashear y mover) y los omitidos no quedarían en el log por el camino estándar. Rechazada.
- C: un `run_duplicate_cleanup` propio. Duplica el código más crítico; contradice la nota de C-09. Rechazada.

### D-14 — Re-verificación del contenido al confirmar **[DUEÑO]**

- **A (recomendada)**, en `DuplicateContentGuard`:
  1. **Copia conservada, una vez por grupo** (al llegar a su primer ítem): para cada conservada, `ContentReader::open(path)` → `meta()` con identidad, tamaño y fecha iguales a los persistidos, no nube → blake3 completo igual al del grupo. Basta **una** conservada válida. Si ninguna → todas las copias del grupo `skipped_protected / kept_copy_unavailable` (resultado cacheado por grupo). Si se cancela a mitad → `Interrupted`.
  2. **Cada copia a eliminar**: abrir `target.path()` (la ruta real fijada), identidad del handle = `target.checked().identity` (si no → `skipped_protected / identity_changed`, código existente de C-03), blake3 completo = hash del grupo (si no → `skipped_locked / content_changed`); fallo de apertura/lectura → `skipped_locked / content_unreadable` (o `in_use` si es `InUse`).
  - Costo: se relee cada copia a eliminar y una conservada por grupo (≈ `(eliminadas + 1) × tamaño`). Con la gobernanza HIGH se prioriza no perder datos; el progreso es por ítem y la cancelación entre bloques.
- B: solo metadatos (identidad + tamaño + fecha por handle) de conservada y eliminadas, sin releer. Rápido, pero un cambio de contenido que preserve la fecha (herramientas que restauran `mtime`, edición por bloques) pasaría. Alternativa si el smoke muestra tiempos inaceptables.
- C: re-verificar solo el hash parcial. Mitad de camino sin garantía. Rechazada.
- La conservada **no** se fija (no se va a mover); si el usuario la borra **después** de su verificación y antes de que termine la ejecución, es una acción propia del usuario (riesgo documentado).

### D-15 — Outcomes y códigos nuevos, sin migrar `cleanup_log`

| Situación | outcome | `error_detail` |
|---|---|---|
| Hash completo de la copia ≠ hash del grupo | `skipped_locked` | `content_changed` (nuevo) |
| Copia a eliminar no se pudo abrir/leer (no `InUse`) | `skipped_locked` | `content_unreadable` (nuevo) |
| Copia a eliminar abierta sin compartir lectura | `skipped_locked` | `in_use` (existente) |
| Identidad del handle ≠ identidad verificada | `skipped_protected` | `identity_changed` (existente) |
| Ninguna copia conservada válida | `skipped_protected` | `kept_copy_unavailable` (nuevo) |

`category = duplicates`, `scan_id` = escaneo de duplicados. Los 3 códigos nuevos van en `domain/cleanup/detail.rs` y en `es.cleanupReasons` (familias: "cambiaron desde la búsqueda", "no se pudieron leer", "la copia que elegiste conservar ya no estaba igual"). El escaneo de duplicados nunca se considera "limpiado" (no se usa `scan_cleaned_at` para duplicados: la unidad es el grupo, D-17).

### D-16 — Guardas de C-09 aplicadas a duplicados **[DUEÑO]**

- **A (recomendada)**: se aplican igual: `changed_since_scan` (tamaño/fecha por handle vs persistidos por handle, sin el desfase del listado) y **24 h** (`recently_modified`) a las copias a eliminar. Una copia escrita hoy (una descarga recién bajada dos veces) no se mueve hasta mañana; el diálogo de revisión lo dice. Coherente con "ante duda, no tocar" y con que el archivo podría estar en uso.
- B: eximir a duplicados de la guarda de 24 h (solo `changed_since_scan` + contenido). Más útil para "lo bajé dos veces hoy", pero abre la puerta a mover archivos que otra app está escribiendo (p. ej. sincronización). Alternativa razonable si el dueño la prefiere: cambia un parámetro de `run_cleanup` y un scenario.

### D-17 — Resolución del grupo tras la ejecución **[DUEÑO]**

- **A (recomendada)**: al terminar `run_cleanup` (cualquier estado), el comando calcula qué grupos tuvieron **al menos un ítem procesado** (registrado: movido u omitido) y llama a `resolve_duplicate_groups(scan_id, [(group_id, kept_ids)], now)` en **una** transacción: `resolved_at = now` y `kept = 1` en las conservadas. Los grupos sin ítems procesados (cancelación o aborto antes de llegar) quedan abiertos y sin marcas. Un grupo resuelto ya no admite preview (`duplicate_group_resolved`) hasta un escaneo nuevo; la UI lo muestra "Resuelto" (plegado) con la copia conservada. Si la resolución falla (base), el resumen se devuelve igual con `groupsResolved: false` y los grupos siguen abiertos (un nuevo preview volvería a re-verificar todo: los movidos darán `unresolvable_not_found`, inocuo).
- Un grupo con un ítem movido y otro omitido (en uso) queda resuelto: para reintentar, "Buscar de nuevo" (lo dice el resumen).
- B: escaneo entero "limpiado" tras la primera ejecución (modelo de C-09 D-20). Impediría resolver los grupos de a poco, que es el uso natural. Rechazada.
- C: resolver al crear el preview o al confirmar (antes de ejecutar). Marcaría `kept` aunque la ejecución se cancele antes del grupo. Rechazada.

### D-18 — Over-the-shoulder

- `start_duplicate_scan` en mismatch → `duplicates_blocked { reason }` sin crear escaneo (las carpetas de contenido serían de la cuenta elevada). La UI muestra el motivo en lugar de "Buscar duplicados" (además del aviso global de C-05).
- Al confirmar, la barrera 1 de `run_cleanup` (categoría `Duplicates`, por-usuario) y el contexto re-resuelto y restringido (C-09 D-05) se aplican sin cambios.
- `preview_duplicate_cleanup` en mismatch → también `duplicates_blocked` (no tiene sentido previsualizar lo que no se puede mover).

### D-19 — Exclusividad y cancelación

- `start_duplicate_scan` toma `ActiveJob` como `JobKind::Scan` (sin tipo nuevo): con cualquier trabajo activo falla con el error del trabajo activo (`scan_in_progress`, `cleanup_in_progress`, `recycle_bin_empty_in_progress`). Cancelación con el `cancel_scan(scanId)` existente.
- `confirm_duplicate_cleanup` toma `JobKind::Cleanup(previewId)`; cancelación con el `cancel_cleanup(previewId)` existente. `preview_duplicate_cleanup` no toma el lugar pero lo consulta (`ensure_idle`), como `preview_cleanup`.
- Motivo de no paralelizar escaneo de duplicados y limpieza de basura aunque toquen carpetas distintas: contención de disco, un único `Channel` de progreso por trabajo y un modelo mental simple ("una operación a la vez").

### D-20 — Progreso

- Escaneo: `Channel<DuplicateScanEvent>`: `started { scanId }`, `walking { filesSeen }` cada 256 archivos, `partial { done, total }` cada 64 archivos, `full { doneBytes, totalBytes }` cada 16 MiB leídos, y uno al cambiar de etapa (por conteo, sin reloj en el dominio: tests deterministas). La respuesta del comando es el resumen.
- Ejecución: los `CleanupEvent` de C-09 (`started`, `progress`) con `progress_every = 1`.

### D-21 — Superficie IPC **[DUEÑO]**

| Comando | Args | Respuesta |
|---|---|---|
| `start_duplicate_scan` | `{ roots?: RootKey[], minSizeBytes?: number, onProgress: Channel }` | `DuplicateScanSummaryDto` |
| `get_last_duplicate_scan` | — | `DuplicateScanSummaryDto \| null` |
| `list_duplicate_groups` | `{ scanId, offset, limit (1..=200), root?: RootKey, minSizeBytes?: number }` | `DuplicateGroupPageDto` |
| `preview_duplicate_cleanup` | `{ scanId, decisions: [{ groupId, keepFileIds: number[] }] }` | `DuplicateCleanupPreviewDto` |
| `confirm_duplicate_cleanup` | `{ previewId, onProgress: Channel }` | `DuplicateCleanupSummaryDto` |
| `cancel_scan` / `cancel_cleanup` (existentes) | sin cambios | sin cambios |

```
RootKey = "desktop" | "documents" | "downloads" | "music" | "pictures" | "videos"
DuplicateScanSummaryDto = { scanId, startedAt, finishedAt, minSizeBytes,
  roots: [{ root, status: "done"|"not_selected"|"nested"|"unavailable", reason: string|null, filesSeen }],
  filesSeen, belowMinSize, protectedCount, cloudCount, hardLinkedCount, changedCount, unreadableCount,
  inaccessibleDirs, excludedDirs, hashedBytes,
  groupCount, duplicateFileCount, reclaimableBytes, openGroupCount }
DuplicateGroupPageDto = { scanId, total, groups: [{ groupId, sizeBytes, blake3, reclaimableBytes,
  resolvedAt: number|null,
  files: [{ fileId, root, path, relativePath, modifiedAt: number|null, kept: boolean }] }] }
DuplicateCleanupPreviewDto = CleanupPreviewDto de C-09 (previewId, scanId, createdAt, expiresAt,
  totalItems, totalBytes, recycleBinAvailable, categories[duplicates]) + { groupCount }
DuplicateCleanupSummaryDto = CleanupSummaryDto de C-09 + { groupsResolved: boolean, resolvedGroupIds: number[] }
```

camelCase; ids como enteros JS seguros validados (`<= 2^53 - 1`). Rutas solo en `list_duplicate_groups` (como `list_junk_findings`); los resúmenes de ejecución sin rutas. `blake3` siempre viaja (el modo simple no lo muestra). Se agregan exactamente 5 permisos `allow-*`; 21 en total, 0 `core:*`.

- Alternativa: reusar `confirm_cleanup` para ambos tipos de preview (4 comandos nuevos). El tipo de respuesta y la resolución posterior difieren; un comando propio mantiene decodificadores y tests separados. Rechazada por claridad.

### D-22 — Errores nuevos

`AppError` suma `DuplicatesBlocked { reason }` (`duplicates_blocked`, con `different_user`/`undetermined` como C-10), `DuplicateGroupNotFound` (`duplicate_group_not_found`), `DuplicateGroupResolved` (`duplicate_group_resolved`) y `DuplicateKeepRequired` (`duplicate_keep_required`). Reutiliza `invalid_argument`, `scan_not_found`, `scan_in_progress`, `cleanup_in_progress`, `recycle_bin_empty_in_progress`, `preview_not_found`, `preview_expired`, `preview_stale`, `nothing_to_clean` y `store_*`. Cada `kind` con texto en `es.errors`. Un fallo por archivo o por ítem nunca es `AppError` (es contador u outcome).

### D-23 — UI: pantalla Duplicados **[DUEÑO]**

- Navegación: Inicio, Limpieza, **Duplicados**, Configuración (delta de `app-shell`).
- **Simple** (una acción primaria por estado):
  1. Sin escaneo: texto "Busca archivos repetidos de 1 MB o más en Escritorio, Documentos, Descargas, Imágenes, Música y Videos. No se mueve nada sin que elijas." + **"Buscar duplicados"**.
  2. Buscando: panel `aria-live="polite"` por etapa ("Buscando archivos: N vistos", "Comparando: N de M", "Verificando contenido: X de Y") + "Cancelar".
  3. Resultados: tarjeta resumen ("N grupos de archivos repetidos. Podés recuperar hasta X.") + omitidos en lenguaje llano (en la nube, protegidos, carpetas no disponibles) + lista paginada (20 + "Ver más") de tarjetas de grupo con `RadioGroup` sin valor inicial; barra inferior con **"Revisar y mover N copias a la Papelera"** (deshabilitado con motivo `aria-describedby` si no hay elecciones) y "Buscar de nuevo" (secundario).
  4. Revisión: diálogo modal con el patrón de `CleanupPreviewDialog` (`<dialog>`, `alertdialog`, foco en "Cancelar"): grupos, copias, total, nota "Se conserva la copia que elegiste en cada grupo; las demás van a la Papelera y podés recuperarlas", nota de recientes, aviso si la Papelera no está disponible (sin confirmar); botón `danger` "Mover N copias a la Papelera". Esc/Cancelar → nada.
  5. Progreso de la ejecución con "Cancelar"; resumen (C-09 D-23 + familias nuevas de D-15) con foco en su título; los grupos resueltos se pliegan como "Resuelto · conservaste <carpeta>\<ruta relativa>".
  - Mismatch: en lugar del botón, mensaje `duplicates_blocked` explicado.
- **Avanzado** (mismas piezas, más densidad): antes de buscar, casillas de carpetas (al menos una) y `select` de tamaño mínimo (4 opciones); filtros de resultados por carpeta y tamaño mínimo (consulta al backend, D-21); por grupo, hash blake3 (monoespaciada, truncado con el valor completo accesible) y ruta completa por copia; casillas "Conservar" (≥ 1, D-10); tabla de omitidos por motivo y estado por carpeta con su código crudo junto al texto (como C-08).
- Textos nuevos en `es.duplicates.*` y `es.errors.*` (sin literales en componentes, `uiLiterals.test.ts`).
- Alternativa: los duplicados como sección dentro de Limpieza. Mezcla basura (automática por categorías) con decisiones por archivo; la pantalla Limpieza ya tiene su acción primaria. Rechazada.

### D-24 — Presupuesto de rendimiento (se mide en el smoke)

- Objetivo orientativo (no bloquea specs): recorrido ≥ 2 000 archivos/s (C-08 midió 17 k archivos en 3–11 s); etapa 2 ≈ 128 KiB por candidato; etapa 3 limitada por el disco (≥ 150 MB/s en SSD en release). Re-verificación al confirmar ≈ `(eliminadas + 1) × tamaño` por grupo.
- Memoria: candidatos en memoria (≈ 200 B por archivo → 100 k archivos ≈ 20 MB); grupos persistidos sin tope, UI paginada.
- Medición: fixture del smoke (tiempos por etapa) y, solo con autorización del dueño, un escaneo de solo lectura de sus 6 carpetas reales (archivos/s, MB/s, grupos), sin decidir nada. Si el escaneo real supera ~5 min o la re-verificación resulta inaceptable, se vuelve al dueño con D-07 B o D-14 B antes de seguir.

### D-25 — Estructura de módulos

```
src-tauri/src/domain/duplicates/
  mod.rs        // re-exports + contrato (solo lectura al escanear; borrado solo vía cleanup)
  model.rs      // ContentFolder (RootKey), MinSize, DuplicateScanRequest, Candidate, DuplicateGroup,
                // DuplicateScanResults, RootStatus, contadores
  roots.rs      // raíces seleccionadas -> estado por raíz, anidadas, atribución a la más profunda
  walk.rs       // recorrido de una raíz con exclusiones (reusa el núcleo de junk/walk.rs, ver abajo)
  hash.rs       // hash parcial/completo sobre ContentReader + verificación antes/después
  scanner.rs    // scan_duplicates: etapas 1-3, cancelación, observer (D-04/D-05/D-20)
  decision.rs   // validación de decisiones (D-12 pasos 2-4)
  preview.rs    // build_duplicate_preview -> DuplicateCleanupPreview (D-12)
  guard.rs      // DuplicateContentGuard: ItemGuard de D-14
  *_tests.rs
src-tauri/src/domain/cleanup/{model,execute,preview,lifecycle,summary,detail}.rs // CleanupCategory, ItemGuard, progress_every, ActivePreview, 3 códigos
src-tauri/src/domain/protection/context.rs   // carpetas de contenido con clave
src-tauri/src/domain/ports/content.rs        // ContentReader / ContentHandle
src-tauri/src/domain/ports/store.rs          // DuplicateStore (+ Store lo incluye)
src-tauri/src/domain/ports/fakes.rs          // FakeContentReader
src-tauri/src/adapters/windows/content.rs    // WindowsContentReader (+ content_tests.rs)
src-tauri/src/adapters/sqlite/migrations/003_duplicates.sql, duplicates.rs (+ contrato)
src-tauri/src/commands/duplicates.rs         // 5 comandos + DTOs (+ duplicates_tests.rs)
src/features/duplicates/                     // DuplicatesScreen, DuplicatesProvider, useDuplicateScan,
                                             // useDuplicateCleanup, GroupCard, ReviewDialog, ...Text.ts
```

- Recorrido: se extrae de `junk/walk.rs` un núcleo genérico (raíz, profundidad ilimitada, predicado de exclusión de carpetas, visitor) que usan ambos; los tests de C-08 son la red de seguridad del refactor. Alternativa: un recorrido propio de duplicados (código duplicado del punto más sensible: no seguir reparse). Rechazada.
- Type-state: `DuplicateCleanupPreview` solo lo crea `build_duplicate_preview`; `run_cleanup` consume el `CleanupPreview` interno por valor; `DeleteTarget` sigue saliendo solo de `prepare_delete`.

### D-26 — Tests

- Dominio (dobles `FakeFileSystem` + `FakeContentReader`): raíces (6 por defecto, subset, clave inválida/repetida/vacía, anidada, no existe, otro volumen, reparse, nube, contexto inválido); exclusiones (`.git`, `node_modules` con mayúsculas mezcladas); elegibilidad (bajo mínimo, protegido por motivo, nube **sin apertura** — `FakeContentReader::opened()` sin esa ruta —, hard link, tamaño por handle vs listado); etapas: **grupo de 2**, **grupo de 3**, **falso positivo del parcial** (head/tail iguales, medio distinto → sin grupo y con hash completo calculado), tamaño único sin lecturas, parcial distinto sin hash completo, archivo ≤ 128 KiB leído una sola vez; cambio durante la lectura (tamaño/fecha/identidad) → `changed`; `InUse` → `unreadable`; cancelación en cada etapa y entre bloques; progreso por conteo.
- Decisiones/preview: **intento de borrar todas las copias → `duplicate_keep_required`**; conservar todas, ids ajenos/repetidos, grupo repetido → `invalid_argument`; grupo inexistente/resuelto; escaneo no último; recientes excluidos; orden de ítems; totales.
- Ejecución con `run_cleanup` (dobles): copia movida; `content_changed`; `content_unreadable`; `in_use`; identidad distinta; conservada borrada/cambiada → todo el grupo `kept_copy_unavailable` y los demás grupos siguen; 2 conservadas con una inválida → procede; cancelación durante el re-hash → `Interrupted` (no registrado, `cancelled`); fijación viva durante el guard; mismatch al confirmar → barrera 1; basura sin cambios de comportamiento (la batería de C-09 sigue verde con `NoGuard`).
- Persistencia: contrato (doble + SQLite temporal): guardado atómico, grupo de 1 copia o identidad repetida rechazados, purga del anterior, hallazgos de basura intactos, paginación/orden/filtros, resolución (válida, sin conservada, doble, de otro escaneo), cascada por retención, `running` → `error` sin grupos; esquema: `CHECK`s de hash, raíz, identidad, `kept` por defecto 0; migración 2 → 3 conservando datos.
- Adaptador `WindowsContentReader` (siempre corre, solo `tempdir`): lectura exacta por offset, metadatos por handle, otro proceso puede escribir/borrar con el archivo abierto, archivo abierto sin compartir → `InUse`, junction abierta como enlace (no destino), hard link (identidad compartida). Symlink de archivo (privilegio) y marcador de nube real → `#[ignore = "motivo"]`.
- Arquitectura: `kept` solo asignado en `resolve_duplicate_groups`; dominio sin `std::fs`/`windows`/`rusqlite`/`tauri` (incluye `duplicates/`); sin borrado permanente nuevo (el test de C-09/C-10 sigue verde).
- Comandos: DTOs exactos, errores (`duplicates_blocked`, `duplicate_group_not_found`, `duplicate_group_resolved`, `duplicate_keep_required`, `invalid_argument`, `preview_stale`, exclusividad), `confirm_cleanup` con un preview de duplicados → `preview_not_found`, resolución tras completar/cancelar, baseline de capabilities (21 permisos).
- Frontend: decodificadores; pantalla simple (acción primaria por estado, `RadioGroup` sin valor inicial, revisar deshabilitado con motivo, grupo resuelto plegado), avanzado (carpetas, tamaño, filtros, hash, casillas con mínimo 1), diálogo de revisión (foco, Esc no ejecuta), progreso y cancelación, resumen con familias nuevas, `duplicates_blocked`, navegación con 4 entradas, `uiLiterals`.

### D-27 — Smoke guiado (manual, con el dueño) **[DUEÑO]**

Se escribe `smoke-guide.md` en el apply (patrón de C-10). Reglas: un paso por vez con confirmación del dueño; **aviso y cuenta atrás real de 10 s** antes de cada lanzamiento de la UI o de UAC; sin capturas (verificación por sistema de archivos, Papelera y base en solo lectura); terminal no elevada salvo la variante elevada; los clics finales de "Mover N copias a la Papelera" los hace el dueño; si algo no coincide, se detiene.

- Fixture en `%USERPROFILE%\Downloads\pcopt-smoke-c11\` (la única raíz seleccionada en avanzado): grupo A (2 copias de 2 MiB), grupo B (3 copias de 5 MiB), par falso positivo (5 MiB, mismos 64 KiB inicial y final), único, par bajo mínimo (900 KiB), par hard link, junction hacia una carpeta "outside" con una copia de A, `node_modules\` con un par, y una copia de A escrita ahora (reciente). Base temporal `PCOPT_DATA_DIR`.
- Pasos: escaneo (solo A y B entre los archivos `pcopt-smoke-*`; contadores de hard link, excluidas y bajo mínimo; si aparecen duplicados reales de Descargas del dueño, **no se tocan**); decisión simple en A y de 2 de 3 en B (avanzado); revisión con Esc (nada movido) y confirmación del dueño; verificación de Papelera (copias con origen en la carpeta del smoke), conservadas intactas, `cleanup_log` (categoría `duplicates`) y grupos resueltos; `kept_copy_unavailable` (el agente mueve la conservada a otra subcarpeta del smoke entre la revisión y la confirmación); `content_changed` (el agente modifica una copia a eliminar y restaura su fecha); tiempos por etapa (D-24); opcional con autorización: escaneo de solo lectura de las 6 carpetas reales sin decidir; opcional: variante elevada (release, `match`) de solo escaneo.
- Cierre: borrar la carpeta del smoke (la junction primero con `rmdir` sin `/s`); lo que quedó en la Papelera, a criterio del dueño.

### D-28 — Dependencias y features

Ninguna dependencia nueva. `blake3` pasa a tener consumidor (actualizar el comentario de `[lints.cargo]`). Las constantes de apertura y `GetFileInformationByHandleEx` están en `Win32_Storage_FileSystem` (habilitada); se confirman con `winmd-api-search` / fuente de `windows` 0.62.2. Si algo exige una feature que cambie `Cargo.lock`, DETENER y consultar.

## Risks / Trade-offs

- [Borrar la última copia real porque la conservada cambió o desapareció] → validación en el preview, verificación de la conservada (identidad + tamaño + fecha + hash) antes de mover cualquier otra del grupo, y re-hash de cada copia con la fijación viva (D-14). Ventana residual: el usuario borra la conservada **después** de su verificación (acción propia, documentada).
- [Leer un marcador de nube y provocar una descarga] → nunca se abre lo que no es `allowed`; `hash_file` rechaza el handle con atributo de nube; `FILE_FLAG_OPEN_NO_RECALL`; test con el doble que registra aperturas.
- [Archivos grandes en uso (PST de Outlook abierto, discos de VM)] → apertura compartida; si el otro proceso no comparte lectura → `unreadable`; si escribe durante el hash → `changed`. Nunca se fuerza.
- [La lectura puede actualizar la fecha de último acceso (si NTFS la mantiene)] → impacto en PA-03 (C-19 evalúa "apps sin uso" con señales de acceso); las raíces son carpetas personales, no de programas. Evitarlo exigiría abrir con `FILE_WRITE_ATTRIBUTES` (`SetFileTime(-1)`): se descarta para mantener el handle de solo lectura.
- [Antivirus escaneando cada archivo leído] → más lento; se mide en el smoke.
- [Escaneos largos en carpetas enormes] → progreso por etapa, cancelación entre bloques, sin paralelismo (D-07); contingencia con aprobación.
- [Duplicados "legítimos" (proyectos, bibliotecas de fotos que esperan su copia)] → exclusiones de D-02, decisión siempre humana, Papelera recuperable.
- [Copias escritas hoy no se mueven] → guarda de 24 h (D-16), explicada en la revisión.
- [Re-verificación costosa (relee lo que se va a mover)] → progreso por ítem y cancelación entre bloques; medición en el smoke; alternativa D-14 B con aprobación.
- [Refactor de `run_cleanup` y del recorrido de C-08 (código HIGH ya verificado)] → la batería completa de C-08/C-09 es la red de seguridad (línea base en la tarea 1.1); `NoGuard` + `progress_every = 32` mantienen el comportamiento de basura idéntico.
- [Numeración de migración] → 003 ahora; si C-07 mergea antes, renumerar el archivo (contenido igual).
- [Over-the-shoulder] → escaneo y preview bloqueados; barrera 1 al confirmar.

## Migration Plan

- Esquema v2 → v3 con `003_duplicates.sql` (solo tablas nuevas; no toca `scan`, `cleanup_log` ni `junk_finding`). Bases v2 se migran al abrir; rollback del binario: una base v3 abierta por una versión anterior da "base de una versión más nueva" (requisito existente de `local-store`) → recuperación manual desde Configuración.
- Capabilities: +5 permisos `allow-*`; sin cambios de CSP.
- Docs a actualizar en el apply (tarea 1.3): `CHANGES.md` (C-11 en curso + decisiones; C-07 → migración 004; nota para C-13 de que los duplicados no entran en flujos automáticos), KB 04 (tablas reales), KB 05 RN-DUP-06 (carpetas, exclusiones, tamaño), KB 02 (nombres reales de comandos en vez de `find_duplicates`/`resolve_duplicate_group`), KB 06 US-011, KB 07 (flujo de duplicados), KB 09 SU-06 (opciones cerradas), KB 10 (PA-16 nueva), KB 11 (re-verificación de contenido y códigos nuevos), KB 12 (filtros reales).

## Decisiones del dueño (bloquean el apply)

**Aprobadas por el dueño el 2026-11-14** ("aprobalas todas"): todas las decisiones D-01..D-28 según la recomendación (D-01 A, D-02 A, D-03 A, D-07 A, D-08 A, D-10 A, D-11 A, D-12 A, D-13 A, D-14 A, D-16 A, D-17 A, D-21, D-23, D-27 tal como están escritas).

| ID | Decisión | Recomendación |
|---|---|---|
| D-01 | Alcance | Las 6 carpetas de contenido del usuario actual por clave; desmarcables en avanzado; sin carpetas arbitrarias (PA-16) |
| D-02 | Exclusiones | No seguir reparse + no descender a `.git`, `.hg`, `.svn`, `node_modules` |
| D-03 | Tamaño mínimo | 1 MiB por defecto; 100 KiB / 1 / 10 / 100 MiB por escaneo en avanzado; no persistido |
| D-07 | Concurrencia | Secuencial, bloques de 1 MiB, cancelación entre bloques; sin rayon |
| D-08 | Migración | `003_duplicates.sql` (C-07 → 004) con 4 tablas (resultado, raíces, grupos, copias) |
| D-10 | Modelo de decisión | Nada preseleccionado; elecciones en el frontend; `kept` solo al resolver |
| D-11 | Elección guiada | Datos neutrales + texto de ayuda; sin "recomendada" ni lotes |
| D-12 | Preview | Validación en el dominio + `PreviewSlot` compartido con C-09 |
| D-13 | Integración | `CleanupCategory` + gancho `ItemGuard` dentro de `run_cleanup` |
| D-14 | Re-verificación | Conservada (identidad, tamaño, fecha, hash) una vez por grupo + re-hash de cada copia con la fijación viva |
| D-16 | Guardas | 24 h y `changed_since_scan` también para duplicados |
| D-17 | Resolución | Grupos con ≥ 1 ítem procesado quedan resueltos (`resolved_at` + `kept`) en una transacción |
| D-21 | Superficie IPC | 5 comandos nuevos + 5 permisos; reusa `cancel_scan` y `cancel_cleanup` |
| D-23 | UI | Pantalla Duplicados propia (simple con `RadioGroup` sin valor; avanzado con carpetas, tamaño, filtros, hash, casillas) |
| D-27 | Smoke | Fixture en `Descargas\pcopt-smoke-c11`, solo esa raíz, clics finales del dueño, escaneo real de solo lectura opcional |

## Desvíos respecto de CHANGES.md / KB

- `CHANGES.md` C-11: "Migración 004" → **003** (D-08; C-07 pasa a 004).
- KB 04: columnas y tablas adicionales (D-08).
- KB 02: comandos `find_duplicates` / `resolve_duplicate_group` → los 5 de D-21 (la resolución ocurre al confirmar, no como comando aparte).
- KB 12 "filtros" en avanzado → por carpeta y tamaño mínimo (sin filtro por nombre ni extensión en v0.1).
- RN-DUP-06 "no se buscan dentro de rutas protegidas" → además solo en las 6 carpetas de contenido y con exclusiones de herramientas (D-01, D-02).
- RN-DUP-05 / US-011 "borrado por flujo de limpieza estándar" → se cumple vía `run_cleanup` con una verificación adicional de contenido (D-13, D-14).

## Preguntas abiertas de KB 10 que toca este change

- **PA-12** (resuelta para v0.1): las carpetas de contenido redirigidas a otro volumen quedan `unavailable/unsupported_volume`; la búsqueda multi-volumen se reevalúa con PA-12.
- **PA-13** (retención): solo el último escaneo de duplicados conserva grupos; no agrega retención nueva al log.
- **PA-03** (apps sin uso): la lectura de contenido puede actualizar la fecha de último acceso de archivos personales (ver Risks); C-19 debe tenerlo en cuenta si usa esa señal.
- **IN-01** (simple vs avanzado): la pantalla Duplicados aplica el patrón de modos; revisión UX en la tarea 9.7.
- **PA-08** (profundidad Win10): `FILE_FLAG_OPEN_NO_RECALL` y `FileIdInfo` existen en Windows 10; sin diferencias esperadas.
- **Nueva PA-16** (se agrega en el apply): ¿se permite buscar duplicados en carpetas arbitrarias (selector de carpetas por plugin `dialog` o COM propio, validado con `check_rule_root`) y en qué versión?

## Open Questions

- Cadencias de progreso (256 archivos, 64 parciales, 16 MiB): ajustables en el apply según la fluidez observada; no cambian specs.
- Tamaño de página de la UI (20 grupos + "Ver más"): ajustable con la revisión UX sin cambiar specs.
- Texto exacto del catálogo: se ajusta con la revisión UX (tarea 9.7).

## Apply notes

### Línea base (tarea 1.1, antes de tocar código)

`npm test`: 19 archivos, **366** tests. `npm run build`: verde. `cargo test --locked`: lib **736** passed + 17 ignored; `architecture` 17; `manifest` 5; `no_network_crates` 9; `security_baseline` 22. `cargo fmt --check` y `cargo clippy --all-targets --all-features --locked -- -D warnings`: verdes. `domain::junk`: 73 tests (red de seguridad del refactor de `walk`).

### APIs confirmadas (tarea 1.2)

Confirmadas contra el fuente de `windows-0.62.2` y `blake3-1.8.7` (`~/.cargo/registry`); no hizo falta cambiar `Cargo.lock` ni features.

| API | Módulo / feature | Nota |
|---|---|---|
| `FILE_READ_DATA`, `FILE_READ_ATTRIBUTES`, `FILE_SHARE_*`, `FILE_FLAG_OPEN_REPARSE_POINT`, `FILE_FLAG_OPEN_NO_RECALL` (0x100000), `FILE_FLAG_SEQUENTIAL_SCAN` | `Win32::Storage::FileSystem` (habilitada) | `FILE_FLAG_OPEN_NO_RECALL` y `FILE_FLAG_SEQUENTIAL_SCAN` existen como constantes `FILE_FLAGS_AND_ATTRIBUTES` |
| `GetFileInformationByHandleEx` + `FileAttributeTagInfo`, `FileBasicInfo`, `FileStandardInfo`, `FileIdInfo` | `Win32::Storage::FileSystem` | ya usadas por `WindowsFileSystem` (`query_info`) |
| `ERROR_SHARING_VIOLATION` (32), `ERROR_LOCK_VIOLATION` (33) | `Win32::Foundation` | ya mapeadas a `PortError::InUse` en `map_io_error` |
| `CreateFileW` / `ReadFile` con `OVERLAPPED` | `Win32::Storage::FileSystem` | **no se usan directamente**: `std::fs::OpenOptions` (`access_mode`, `share_mode`, `custom_flags`) abre con los mismos flags y `std::os::windows::fs::FileExt::seek_read` hace `ReadFile` con offset; así no se agrega `unsafe` nuevo para leer |
| `blake3::Hasher::new`, `update(&[u8]) -> &mut Self`, `finalize() -> Hash`, `Hash::to_hex() -> ArrayString` | `blake3` (sin features extra) | `blake3::hash(&[u8])` para el parcial de archivos chicos |

### Part 1 (grupos 1 a 4): desvíos y notas

- RED/GREEN por tarea se hizo con tests y código del mismo grupo escritos juntos y ejecutados en cada paso; el RED observado fue el de compilación (símbolos inexistentes) y, en el escáner, varios fallos de aserción reales al primer run (fixture sin ancestros y sin `canonical_as_is`, orden de rutas esperado, `files_seen` por carpeta), corregidos en el test o en el fixture, nunca debilitando una aserción de comportamiento.
- `RawUserFolders.content_folders` pasa a `Vec<(ContentFolder, String)>`; `UserContext` guarda además `keyed_content_folders` en paralelo y `content_folders()` sigue devolviendo `&[NormalizedPath]` (sin tocar las reglas de C-03).
- `junk/walk.rs`: se agregó `walk_directory(fs, root, exclude, visitor)` (raíz, profundidad ilimitada, predicado de exclusión de carpetas) y el método con valor por defecto `WalkVisitor::excluded_dir`; el comportamiento de basura no cambia (`domain::junk`: 73 tests antes y después).
- `RootReason::Inaccessible` (`inaccessible`) se agrega a los motivos de raíz de D-01 para un `metadata` que falla por permiso u otro error de E/S distinto de "no existe" (la tabla de D-01 no lo nombraba).
- `FakeContentReader` vive en `ports/fakes_content.rs` (el doble comparte el `FakeFileSystem` y lee los metadatos con `peek`, sin registrar llamadas) y la batería de contrato en `ports/content_contract.rs`.

_(Se completa durante el apply.)_

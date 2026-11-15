# Design

> Gobernanza **MEDIUM** (análisis de solo lectura), con decisiones que tocan reglas **HIGH**: el selector de carpeta y "Mostrar en el Explorador" usan COM del shell en un proceso elevado (alternativa a los plugins `dialog`/`shell`, que exigirían ampliar capabilities), se agrega un modo de validación en `path-protection` y el análisis lista nombres de archivos personales. Las decisiones marcadas **[DUEÑO]** requieren aprobación explícita antes del apply. Las tareas asumen la opción recomendada; si el dueño elige otra, se actualizan specs y tasks antes de codear (`/opsx:update`).

## Context

Motivación: ver `proposal.md`. Estado actual relevante (observado en el código, no supuesto):

- **Recorrido (C-08/C-11)**: `domain/junk/walk.rs` expone `walk_directory(fs, root, exclude, visitor) -> WalkReport` sobre el puerto `FileSystem`: iterativo (pila explícita), determinista (hijos ordenados por nombre plegado NTFS), no sigue reparse points (`EntryKind::ReparsePoint` sin nube → `visitor.reparse_point(path)`), trata un reparse de nube como archivo, consulta la cancelación antes de cada listado y de cada entrada, y cuenta `inaccessible` sin la ruta ni el motivo. El `WalkVisitor` no recibe "carpeta listada" (no sabe a qué carpeta pertenece cada archivo salvo por su ruta).
- **Adaptador de listado**: `WindowsFileSystem::read_dir` abre la carpeta para leer sus metadatos (no sigue reparse) y lista con `std::fs::read_dir` (datos de la enumeración, sin abrir cada archivo): `size_bytes` lógico solo para `kind = File` (un reparse de nube llega con tamaño 0), `identity` y `hard_links` en `None`. Un error al leer una entrada falla el listado de toda la carpeta.
- **Protección (C-03)**: `evaluate_rules(ctx, path) -> RuleVerdict` es puro (motivos `windows_dir`, `program_files`, `program_data`, `volume_root`, `volume_system_entry`, `other_user_profile`, `anchor`…); `PathProtection::check_rule_root(raw)` valida raíces de reglas (rechaza `C:\`, `C:\Windows`, el perfil…), demasiado restrictivo para un análisis de solo lectura. `ProtectionContext::system_drive()`, `users_root()`, `user()`; en mismatch `restrict_protection_context` quita al usuario y todo el perfil pasa a `other_user_profile`. `junk::sources::sources(ctx)` devuelve las ubicaciones de la whitelist.
- **Trabajos (C-09/C-10/C-11)**: `ActiveJob` con `JobKind::{Scan(Option<ScanId>), Cleanup, EmptyRecycleBin}`; `begin_scan()` + `JobGuard::bind(scan_id)` + `cancel_flag()`; `cancel_scan(scanId)`. `start_scan(kind)` acepta solo `junk` y devuelve `JunkScanSummaryDto`; con `folders` responde `scan_kind_unsupported` (hay test en `error.rs`). Progreso por `Channel` de la propia invocación (sin `core:*`).
- **SQLite**: esquema v3 (001 + 002 + 003). `scan.kind` ya admite `folders`. Retención de 50 escaneos con cascada; "solo el último `done` conserva hijos" para basura y duplicados; `running` → `error` al abrir. El runner exige versiones contiguas: la próxima es **004** (como dice `CHANGES.md`).
- **COM en el backend**: `adapters/windows/recycle_bin_empty.rs` ya inicializa COM en STA por llamada (`CoInitializeEx` / `CoUninitialize` con `catch_unwind`). Features de `windows` 0.62.2 habilitadas: `Win32_Storage_FileSystem`, `Win32_System_Com`, `Win32_UI_Shell`, `Win32_UI_WindowsAndMessaging`, entre otras.
- **Seguridad**: 21 permisos `allow-*`, 0 `core:*`, sin plugins; CSP de producción `style-src 'self'` (sin `unsafe-inline`) e `img-src 'self'`; `withGlobalTauri: false`.
- **Dependencias**: `walkdir = "=2.5.0"` fijado desde C-01 "para C-07" y sin consumidor; no hay librería de gráficos (PA-01 abierta).
- **Frontend**: navegación Inicio / Limpieza / Duplicados / Configuración (`navigation.ts`, `useNavigation().navigate`), `density.tsx`, `Card`, `Button`, `StatusBadge`, catálogo `es.ts`, `errors.ts`, `mockBackend.ts` con `Channel` simulado.

## Goals / Non-Goals

**Goals:**
- El análisis no escribe, no abre ni lee contenido de archivos y no sigue enlaces: solo lista carpetas y lee metadatos de la enumeración.
- Ninguna ruta libre viaja de la interfaz al backend: raíces por tipo o por id de selección, elementos por id del último análisis.
- Sin plugins ni capabilities nuevas fuera de los 6 permisos de comandos propios; sin dependencias nuevas.
- Dominio 100 % testeable con dobles; adaptadores reales solo sobre `tempdir` (los que dependen de la máquina o de UI nativa, `#[ignore]`).
- No bloquear en PA-01: visualización sin librería.

**Non-Goals:**
- Exactitud de "espacio en disco ocupado" (asignado, hard links únicos, compresión): se usa tamaño lógico y se explica la diferencia (D-06).
- Rendimiento máximo (sin paralelismo; se mide en el smoke, D-21).
- Historia de análisis y tendencias (C-25).

## Decisions

### D-01 — Alcance: raíz del volumen del sistema por defecto + una carpeta elegida en avanzado **[DUEÑO]**

- **A (recomendada)**: dos tipos de raíz, ambos en el volumen del sistema (PA-12):
  - `system_drive`: la raíz del volumen del sistema (`C:\`, tomada de `ProtectionContext::system_drive()`, nunca de la interfaz). Es la única opción del modo simple ("Analizar disco C:").
  - `picked`: una carpeta elegida con el selector (D-02) y validada (D-03). Solo en avanzado, junto a "Disco C:".
  - Las rutas protegidas **se recorren** (análisis de solo lectura; saber que `C:\Windows` ocupa 30 GB es parte de la respuesta), marcadas como "sistema" (D-07) y sin otra acción que mostrarlas en el Explorador. Los perfiles ajenos y entradas privadas se miden como opacas (D-04).
- B: solo el perfil del usuario. No responde "qué llena mi disco" (Windows, Program Files, `pagefile.sys`, otros perfiles quedan fuera). Rechazada.
- C: las 6 carpetas de contenido (como C-11). Igual que B y además pierde AppData, donde suelen estar los grandes consumidores (cachés, máquinas virtuales, juegos). Rechazada.
- D: solo `system_drive`, selector diferido a PA-16. Viable y más chico, pero contradice la nota de C-05 (este es el primer consumidor del selector) y obliga a que el smoke escanee el disco real del dueño. Alternativa si el dueño no aprueba D-02.
- Otros volúmenes: no se ofrecen (PA-12); el selector rechaza carpetas de otro volumen con `unsupported_volume`.

### D-02 — Selector de carpeta: `IFileOpenDialog` propio en un adaptador de Windows **[DUEÑO]**

- **A (recomendada)**: puerto `FolderPicker { fn pick(&self) -> Result<Option<PathBuf>, PortError> }` (en `domain/ports/shell.rs`) y adaptador `adapters/windows/folder_picker.rs`: en un hilo propio con COM STA (`CoInitializeEx` + guarda que llama `CoUninitialize`), `CoCreateInstance(FileOpenDialog)` → `SetOptions(FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST | FOS_NOCHANGEDIR | FOS_DONTADDTORECENT)` → `Show(hwnd de la ventana principal)` → `GetResult` → `GetDisplayName(SIGDN_FILESYSPATH)` → `CoTaskMemFree`. `HRESULT_FROM_WIN32(ERROR_CANCELLED)` → `Ok(None)`. Todo dentro de `catch_unwind` (patrón de `recycle_bin_empty.rs`); `unsafe` mínimo con `// SAFETY:`.
  - El comando `pick_folder_root` valida la ruta (D-03) y la guarda en un `PickedRootSlot` del backend (una sola selección viva; elegir otra la reemplaza; no vence hasta cerrar la app) y devuelve `{ pickId, path }`. La ruta se devuelve **solo para mostrarla** (la eligió el propio usuario); `start_folder_scan` recibe el `pickId`, nunca la ruta.
  - Un solo diálogo a la vez (`folder_picker_busy`). El diálogo es modal respecto de la ventana principal; no toma `ActiveJob` (no toca el disco).
  - Cubre UIPI: el diálogo es del propio proceso (elevado), así que no hay arrastre entre niveles de integridad. La UI **no** ofrece zona de arrastrar y soltar.
- B: plugin `tauri-plugin-dialog`. Dependencia nueva (npm + Cargo, cambia los lockfiles) y permiso de plugin en la capability: rompe "sin plugins" y requiere aprobación explícita; además el webview recibiría la ruta y la mandaría de vuelta (ruta libre desde la interfaz). Por dentro usa el mismo `IFileOpenDialog`. Rechazada.
- C: diferir a PA-16 (ver D-01 D).
- Riesgo propio de A y B por igual: el diálogo del shell carga extensiones de shell de terceros (vista previa, iconos) dentro del proceso **elevado**. Se mitiga con `FOS_PICKFOLDERS` (sin vista previa de archivos) y se documenta en KB 11/13; es el mismo riesgo que cualquier diálogo de apertura de una app elevada.

### D-03 — Validación de la raíz de análisis en `path-protection` **[DUEÑO]**

- **A (recomendada)**: `PathProtection::check_analysis_root(raw) -> Result<NormalizedPath, ProtectedReason>` con el mismo algoritmo de resolución que `check_rule_root` (normalización léxica, ruta real con nombres largos, sin reparse en la ruta ni como objetivo, forma léxica = forma real, existe y es carpeta), pero con otra tabla de veredictos, porque **no habilita ningún borrado**:
  - Acepta: `allowed`, `volume_root` (equivale a `system_drive`), `anchor` (incluye `C:\Users` y el perfil propio), `windows_dir`, `program_files`, `program_data`, `browser_profile`, `app_dir`. Una carpeta de nube como raíz se acepta (sus archivos cuentan como nube, D-06).
  - Rechaza con su motivo: `unsupported_volume` (otro volumen, red o dispositivo), `other_user_profile` (privacidad, D-04), `volume_system_entry` (`$Recycle.Bin`, `System Volume Information`…), `context_unavailable`, reparse en la ruta u objetivo, forma no coincidente, no existe, no es carpeta.
  - Se re-ejecuta al iniciar el escaneo (la carpeta pudo cambiar entre la selección y el inicio); el recorrido además trata una raíz que es reparse como inaccesible (ya lo hace `walk_root`).
- B: reutilizar `check_rule_root`. Rechaza `C:\`, `C:\Windows`, `C:\Program Files` y el perfil: justo lo que el usuario quiere analizar. Rechazada.
- C: validar en el comando sin pasar por `path-protection`. Duplicaría la resolución de rutas (8.3, reparse, forma), el punto más delicado. Rechazada.

### D-04 — Carpetas opacas: privacidad de otros perfiles y de la Papelera **[DUEÑO]**

- **A (recomendada)**: una carpeta es **opaca** si es la primera, bajando desde la raíz, que cumple: `evaluate_rules` → `other_user_profile`; o es `$Recycle.Bin` o `System Volume Information` en la raíz del volumen. De una carpeta opaca se recorre el contenido para sumar tamaño y cantidades (si se puede listar), pero **ninguna ruta de su interior** se guarda: no hay filas hijas, no entra en el top de archivos y sus carpetas inaccesibles solo suman al contador (sin ruta). En la UI: "Perfil de otro usuario", "Papelera" o "Información del sistema", con tamaño y sin navegación ni "Mostrar en el Explorador".
  - En over-the-shoulder (mismatch) el contexto restringido no tiene usuario, así que **todos** los perfiles son opacos (coherente con PA-14: lo por-usuario no se detalla).
- B: no descender a las carpetas opacas (tamaño desconocido, "no analizado"). Más privado, pero el disco queda con un hueco grande sin explicar en equipos compartidos. Alternativa válida si el dueño prefiere no listar perfiles ajenos en absoluto.
- C: tratarlas como cualquier carpeta. Expone nombres de archivos de otros usuarios a quien use la app elevada. Rechazada.

### D-05 — Recorrido: núcleo de C-08/C-11 con dos ganchos opcionales, sin `walkdir` **[DUEÑO]**

- **A (recomendada)**: reutilizar `walk_directory` (puerto `FileSystem`, sin exclusiones: predicado que nunca excluye) y agregar al `WalkVisitor` dos métodos **con implementación vacía por defecto**: `dir_listed(&Path)` (carpeta listada, antes de sus entradas) y `dir_inaccessible(&Path, &PortError)` (no se pudo listar; el contador `inaccessible` sigue igual). C-08 y C-11 no los implementan: su comportamiento no cambia (sus baterías son la red de seguridad).
  - El agregador asigna a cada archivo la carpeta actual (la última `dir_listed`, porque el núcleo entrega los archivos de una carpeta justo después de listarla) y suma; al final consolida los tamaños de abajo hacia arriba.
- B: usar `walkdir` (como dice `CHANGES.md`). Recorre el disco real directamente: el dominio no podría probarse con el doble de `FileSystem` y habría un segundo recorrido distinto en el punto más sensible (no seguir reparse; `walkdir` sigue o no según `follow_links`, y su detección de junctions en Windows depende de la versión). Rechazada.
- `walkdir` queda sin consumidor: se actualiza el comentario de `[lints.cargo]`; **quitarla** cambia `Cargo.lock` y queda como decisión aparte del dueño (se anota en `CHANGES.md`).

### D-06 — Contabilidad de tamaños **[DUEÑO]**

- **A (recomendada)**: tamaño **lógico** de la enumeración (el que muestra el Explorador en "Tamaño"), sin abrir archivos:
  - Archivos (`kind = File`): suman su tamaño a su carpeta y ancestros, y cuentan en `file_count`.
  - Hard links: se cuentan **una vez por nombre** (la enumeración no trae identidad; abrir cada archivo para deduplicar sería millones de aperturas). `C:\Windows\WinSxS` aparece más grande que su espacio real, igual que en el Explorador.
  - Nube (reparse de nube): cuentan en `cloud_files`, con **0 bytes** (un marcador solo-nube no ocupa disco y el adaptador ya informa 0); limitación: un archivo de OneDrive descargado también suma 0. La UI lo explica ("N archivos en la nube no se suman").
  - Reparse sin nube (junction, symlink, punto de montaje): no se siguen y cuentan en `reparse_skipped` (sin tamaño).
  - Flujos alternativos (ADS), compresión NTFS y archivos dispersos: no se consideran (tamaño lógico del flujo principal).
  - Espacio del volumen: puerto `VolumeSpace { fn space(&self, root: &Path) -> Result<VolumeSpaceInfo { total_bytes, free_bytes }, PortError> }` con `GetDiskFreeSpaceExW` (adaptador `adapters/windows/volume.rs`). Si la raíz es `system_drive`, "no contabilizado" = `max(0, (total − libre) − total_analizado)` (archivos sin acceso, metadatos del sistema, nube descargada, diferencias de asignación); si el analizado supera al usado (hard links, compresión) se informa 0 y una nota. Si falla la consulta, el resumen sale sin datos del volumen.
- B: tamaño asignado (`GetCompressedFileSizeW` o `FileStandardInfo.AllocationSize` por archivo). Una llamada por archivo (millones) y aun así cuenta hard links repetidos. Rechazada para v0.1.
- C: deduplicar hard links por identidad. Exige abrir cada archivo con más de un nombre posible; inviable en un disco entero. Rechazada.
- D: informar el tamaño lógico de los reparse de nube (cambio del adaptador compartido). Cambiaría contadores de C-08/C-11 (que hoy ven esos archivos con 0 bytes). Diferida.

### D-07 — Granularidad de lo persistido: hasta 20 000 carpetas, las más grandes **[DUEÑO]**

- **A (recomendada)**: se guardan **todas** las carpetas si son ≤ 20 000; si hay más, las 20 000 más grandes, ordenadas por (tamaño desc, profundidad asc, ruta plegada asc). Como una carpeta nunca es más chica que una hija, ese orden garantiza que el conjunto guardado es **cerrado por ancestros** (toda carpeta guardada tiene su padre guardado; la raíz siempre). Se prueba como propiedad.
  - Por carpeta: `size_bytes` (recursivo), `direct_bytes` (archivos directamente dentro), `file_count` y `dir_count` (recursivos), `access` (`complete`, `incomplete` si algún descendiente no se pudo listar, `inaccessible` si ella misma no se pudo listar), `opaque_reason`, `system` (D-07 b) y `junk_hint` (D-14).
  - "Resto" de una carpeta en la UI = `size_bytes − direct_bytes − Σ hijos guardados` ("Carpetas más chicas"); "Archivos sueltos" = `direct_bytes`.
  - b) `system` = `evaluate_rules` da `windows_dir`, `program_files`, `program_data` o `volume_system_entry` (texto: "Archivos de Windows o de programas: no los borres a mano").
  - El resultado informa `folders_stored` y `folders_truncated`.
  - Memoria durante el recorrido: un nodo por carpeta (nombre, padre, sumas, banderas; ≈ 150–250 B con el índice por ruta) → 300 000 carpetas ≈ 75 MB; las carpetas dentro de opacas no tienen nodo propio. Se mide en el smoke (D-21).
- B: corte por profundidad (p. ej. 3 niveles). `C:\Users\ana\AppData\Local\…` necesita más de 3 niveles para llegar a lo interesante y `WinSxS` tiene decenas de miles de hijos en el nivel 3. Rechazada.
- C: todas las carpetas siempre. Un `C:` típico tiene 150 000–400 000 carpetas: decenas de MB por análisis en la base. Rechazada.
- D: umbral fijo (≥ 1 MiB). Sin tope duro de filas. Rechazada por imprevisible.

### D-08 — Top de archivos grandes: los 500 más grandes **[DUEÑO]**

- **A (recomendada)**: se guardan los 500 archivos más grandes (montículo acotado durante el recorrido; empates por ruta plegada), fuera de carpetas opacas, con ruta, tamaño lógico, fecha de modificación de la enumeración y la carpeta guardada más cercana (`folder_id`). La UI ordena **ese** conjunto por tamaño, por más nuevos o por más viejos (US-007 "ordenable por tamaño y fecha"); el texto lo aclara ("los 500 archivos más grandes").
- B: top por carpeta (los N más grandes de cada carpeta guardada). Multiplica filas (hasta 20 000 × N). Diferida.
- C: 100 archivos. Suficiente para el modo simple, corto para avanzado. El número es una constante: ajustable sin cambiar el esquema.

### D-09 — Migración 004 y esquema **[DUEÑO]**

`004_folders.sql` (contigua a la 003 de C-11, como dice `CHANGES.md`):

```sql
CREATE TABLE folder_scan_result (                   -- uno por análisis terminado
  scan_id INTEGER PRIMARY KEY REFERENCES scan(id) ON DELETE CASCADE,
  root_kind TEXT NOT NULL CHECK (root_kind IN ('system_drive','picked')),
  root_path TEXT NOT NULL CHECK (length(root_path) > 0),
  volume_total_bytes INTEGER CHECK (volume_total_bytes IS NULL OR volume_total_bytes > 0),
  volume_free_bytes INTEGER CHECK (volume_free_bytes IS NULL OR volume_free_bytes >= 0),
  total_bytes, file_count, dir_count, inaccessible_dirs, reparse_skipped, cloud_files,
  opaque_dirs, folders_stored INTEGER NOT NULL CHECK (... >= 0),   -- todos los contadores >= 0
  folders_truncated INTEGER NOT NULL CHECK (folders_truncated IN (0,1)),
  CHECK ((volume_total_bytes IS NULL) = (volume_free_bytes IS NULL))
) STRICT;
CREATE TABLE folder_snapshot (
  id INTEGER PRIMARY KEY,
  scan_id INTEGER NOT NULL REFERENCES scan(id) ON DELETE CASCADE,
  parent_id INTEGER REFERENCES folder_snapshot(id) ON DELETE CASCADE,
  path TEXT NOT NULL CHECK (length(path) > 0),
  depth INTEGER NOT NULL CHECK (depth >= 0),
  size_bytes INTEGER NOT NULL CHECK (size_bytes >= 0),
  direct_bytes INTEGER NOT NULL CHECK (direct_bytes >= 0 AND direct_bytes <= size_bytes),
  file_count INTEGER NOT NULL CHECK (file_count >= 0),
  dir_count INTEGER NOT NULL CHECK (dir_count >= 0),
  access TEXT NOT NULL CHECK (access IN ('complete','incomplete','inaccessible')),
  opaque_reason TEXT CHECK (opaque_reason IS NULL OR opaque_reason IN
    ('other_user_profile','recycle_bin','system_volume_information')),
  system INTEGER NOT NULL CHECK (system IN (0,1)),
  junk_hint INTEGER NOT NULL CHECK (junk_hint IN (0,1)),
  CHECK ((depth = 0) = (parent_id IS NULL)),
  UNIQUE (scan_id, path)
) STRICT;
CREATE INDEX idx_folder_snapshot_path ON folder_snapshot(path, scan_id);          -- KB 04 (tendencias)
CREATE INDEX idx_folder_snapshot_children ON folder_snapshot(scan_id, parent_id, size_bytes DESC);
CREATE TABLE large_file (
  id INTEGER PRIMARY KEY,
  scan_id INTEGER NOT NULL REFERENCES scan(id) ON DELETE CASCADE,
  folder_id INTEGER NOT NULL REFERENCES folder_snapshot(id) ON DELETE CASCADE,
  path TEXT NOT NULL CHECK (length(path) > 0),
  size_bytes INTEGER NOT NULL CHECK (size_bytes > 0),
  modified_at INTEGER CHECK (modified_at IS NULL OR modified_at > 0),
  UNIQUE (scan_id, path)
) STRICT;
CREATE INDEX idx_large_file_size ON large_file(scan_id, size_bytes DESC);
CREATE INDEX idx_large_file_modified ON large_file(scan_id, modified_at);
CREATE TABLE folder_scan_issue (                    -- muestra de carpetas sin acceso (<= 50)
  scan_id INTEGER NOT NULL REFERENCES scan(id) ON DELETE CASCADE,
  path TEXT NOT NULL CHECK (length(path) > 0),
  reason TEXT NOT NULL CHECK (reason IN ('access_denied','not_found','io_error')),
  PRIMARY KEY (scan_id, path)
) STRICT;
```

- Se insertan las carpetas por profundidad (padre antes que hija; las claves foráneas están activas). El store valida: raíz única de profundidad 0, cada `parent_id` del mismo escaneo, `folder_id` de cada archivo del mismo escaneo, ≤ 50 incidencias.
- Desvío de KB 04 (que lista `folder_snapshot(id, scan_id, path, size_bytes, file_count, parent_id)`): columnas `depth`, `direct_bytes`, `dir_count`, `access`, `opaque_reason`, `system`, `junk_hint` y tres tablas nuevas (`folder_scan_result`, `large_file`, `folder_scan_issue`). Sin el resultado, el resumen y "por qué no aparece esta carpeta" no sobreviven a un reinicio. Se documenta en KB 04 en el apply.
- B: el top de archivos y las incidencias como JSON en el resultado. Sin `CHECK`, sin índices para ordenar por fecha ni paginar. Rechazada.

### D-10 — Guardado y retención

- Guardado **al final** y atómico (`FolderStore::complete_folder_scan(scan_id, &FolderScanResults, finished_at)`): resultado + carpetas + archivos + incidencias + `finish_scan(done)` + purga de carpetas, archivos e incidencias de los análisis de carpetas anteriores (se conservan su `scan` y su `folder_scan_result`, livianos). Cancelado o con error → no se guarda nada (el escaneo queda `cancelled`/`error`). La cancelación se consulta por última vez justo antes de guardar; una vez abierta la transacción, el guardado termina.
- Retención: la de 50 escaneos (cascada) + "solo el último `done` conserva carpetas y archivos". `running` → `error` al abrir y sin datos (delta de `local-store`).
- B: conservar los análisis anteriores para comparar. Lo necesita C-25 (tendencias, con su propia retención de ~90 días, KB 04); por eso el índice `(path, scan_id)` se crea ya. Diferida a C-25. PA-13 sigue abierta y no empeora.

### D-11 — Carpetas inaccesibles

- `dir_inaccessible(path, error)`: `PermissionDenied` → `access_denied`; `NotFound` (desapareció durante el recorrido) → `not_found`; resto → `io_error`. La carpeta queda con `access = inaccessible` y tamaño 0; sus ancestros con `incomplete` ("al menos X"). El escaneo sigue.
- Se guardan las primeras 50 (orden del recorrido, determinista) en `folder_scan_issue`; el total en `inaccessible_dirs`. Dentro de una carpeta opaca: solo el contador.
- La raíz misma inaccesible → análisis `done` con raíz `inaccessible`, tamaño 0 y la incidencia (no es error del comando).

### D-12 — Visualización: lista con barras proporcionales, sin librería; treemap diferido **[DUEÑO]**

- **A (recomendada)**: en ambos modos, lista ordenada por tamaño con una barra proporcional al tamaño de la carpeta padre (componente propio `ProportionBar`: SVG con un `<rect>` cuyo ancho es un atributo en porcentaje; sin estilos en línea, compatible con la CSP `style-src 'self'`), `aria-hidden` y siempre acompañada del valor en texto ("12,4 GB · 31 %"). Un solo tono del tema; estados (sistema, sin acceso, incompleta, opaca) con icono + texto, nunca solo color (skill `dataviz`). En avanzado, la lista es navegable (entrar a una carpeta, migas de pan) con una fila "Carpetas más chicas" y otra "Archivos sueltos".
  - No fuerza PA-01: no hay serie temporal ni gráfico en vivo; PA-01 se decide en C-06.
- B: treemap propio en SVG (algoritmo squarified puro en TS, ~150 líneas, testeable) solo en avanzado, además de la lista. Más trabajo de accesibilidad (foco por rectángulo, etiquetas que no entran) y de prueba; aporta poco sobre la lista navegable. Propuesta como seguimiento opcional (post C-06 o C-12).
- C: treemap con librería (ECharts, visx, d3-hierarchy). Dependencia nueva y bloqueada por PA-01. Rechazada para v0.1.
- `KB 12` dice "Treemap/lista completa": se cumple con la lista; el treemap queda anotado como desvío.

### D-13 — "Mostrar en el Explorador" sin el plugin `shell` **[DUEÑO]**

- **A (recomendada)**: comando `reveal_in_explorer({ scanId, target: { kind: "folder" | "file", id } })`:
  1. `scanId` debe ser el último análisis de carpetas `done` (si no, `scan_not_found`); el id debe ser de ese análisis (si no, `folder_not_found`).
  2. Carpeta opaca → `reveal_unavailable{opaque}` (además la UI no ofrece la acción). En mismatch → `reveal_unavailable{different_user|undetermined}` (D-15).
  3. Ruta persistida re-verificada: `metadata` sin seguir reparse; no existe → `reveal_unavailable{target_missing}`.
  4. Puerto `Explorer { fn reveal(&self, path: &Path) -> Result<(), PortError> }`; adaptador `adapters/windows/explorer.rs`: hilo con COM STA, `SHParseDisplayName(ruta)` → `SHOpenFolderAndSelectItems(pidl, 0, null, 0)` (abre la carpeta contenedora y selecciona el elemento; para la raíz del volumen, abre la raíz), `CoTaskMemFree` del pidl. Error → `reveal_unavailable{shell_error}`.
  - **Nunca** `ShellExecute`/"abrir" sobre el elemento (un `.exe` o `.lnk` no se ejecuta jamás); un test de arquitectura verifica que `ShellExecute*` no aparece en el código de producción.
  - La ventana del Explorador la abre el shell del usuario (el Explorador no corre elevado); se verifica en la variante elevada del smoke.
- B: `explorer.exe /select,"<ruta>"` con `std::process::Command`. Lanzar procesos desde la app elevada, problemas de comillas y comas en rutas, y el proceso nuevo hereda el token elevado. Rechazada.
- C: plugin `opener`/`shell` (`revealItemInDir`). Prohibido por la regla dura y por `security-baseline`. Rechazada.
- D: diferir. Posible si el dueño no aprueba COM en el backend; entonces la UI solo muestra la ruta (copiable) en avanzado.

### D-14 — "Enviar a revisión de limpieza" = enlace a Limpieza para ubicaciones de la whitelist **[DUEÑO]**

- **A (recomendada)**: no se crea ningún camino de borrado. `junk_hint = 1` si la carpeta es o está dentro de una ubicación de `junk::sources::sources(ctx)` cuya categoría tiene alcance permitido (`scope_access`, D-15). En esas carpetas la UI ofrece "Revisar en Limpieza", que solo navega a la pantalla Limpieza (`navigate("cleanup")`), donde el usuario escanea y revisa con el preview de C-09. Los archivos grandes y el resto de las carpetas solo tienen "Mostrar en el Explorador" (el usuario decide fuera de la app, con la Papelera del Explorador).
- B: una categoría de limpieza "archivos grandes" que arma un preview con `CleanupItem` propios y usa `run_cleanup` (patrón de C-11). Es una categoría nueva fuera de la whitelist: la regla dura exige su propio `/opsx:propose` aprobado (gobernanza HIGH). Diferida.
- C: sin ninguna acción de limpieza. Pierde el puente natural "esta carpeta enorme es la caché de Chrome". Rechazada.

### D-15 — Over-the-shoulder

- `start_folder_scan` **no se bloquea** en mismatch: el análisis es de solo lectura y de máquina. El contexto de protección se resuelve restringido (`restrict_protection_context`): todos los perfiles son opacos (D-04) y `junk_hint` solo marca ubicaciones de alcance máquina (`temp_windows`, `wu_cache`…).
- `pick_folder_root` en mismatch valida con el contexto restringido: cualquier carpeta bajo la raíz de perfiles se rechaza con `other_user_profile`.
- `reveal_in_explorer` en mismatch → `reveal_unavailable{different_user|undetermined}`: el Explorador se abriría con la cuenta elevada, no con la del usuario sentado. La UI muestra el motivo en lugar del botón.
- La UI muestra, además del aviso global de C-05, "En este modo los perfiles de usuario se muestran solo como tamaño total".

### D-16 — Concurrencia, exclusividad y cancelación

- `start_folder_scan` toma `ActiveJob` como `JobKind::Scan` (sin tipo nuevo): con cualquier trabajo activo falla con el error de ese trabajo (`scan_in_progress`, `cleanup_in_progress`, `recycle_bin_empty_in_progress`). Cancelación con el `cancel_scan(scanId)` existente.
- Un solo hilo de `run_blocking` (como C-08/C-11): sin paralelismo; amigable con discos rígidos; sin dependencias nuevas.
- Granularidad: el núcleo consulta la cancelación antes de cada listado y de cada entrada (una carpeta de 20 000 entradas se termina de listar, pero su recorrido se corta entre entradas); última consulta antes de guardar (D-10).
- `pick_folder_root` y `reveal_in_explorer` no toman `ActiveJob` (no recorren el disco); el selector tiene su propia exclusión (`folder_picker_busy`). Elegir carpeta durante un escaneo está permitido; iniciar otro, no.

### D-17 — Progreso

- `Channel<FolderScanEvent>`: `started { scanId, rootKind }`; `walking { filesSeen, dirsSeen, bytesSeen }` cada 4 096 archivos o cada 512 carpetas listadas (lo que ocurra primero, por conteo, sin reloj en el dominio: tests deterministas); `saving` una vez antes de guardar. Ningún evento lleva rutas (los nombres de carpetas opacas nunca viajan).
- La respuesta del comando es el resumen (`done` o `cancelled`).

### D-18 — Superficie IPC **[DUEÑO]**

| Comando | Args | Respuesta |
|---|---|---|
| `start_folder_scan` | `{ root: { kind: "system_drive" } \| { kind: "picked", pickId }, onProgress: Channel }` | `FolderScanSummaryDto` |
| `get_last_folder_scan` | — | `FolderScanSummaryDto \| null` |
| `list_folder_children` | `{ scanId, folderId: number \| null, sort: "size" \| "name", offset, limit (1..=200) }` | `FolderPageDto` |
| `list_large_files` | `{ scanId, sort: "size" \| "newest" \| "oldest", offset, limit (1..=200) }` | `LargeFilePageDto` |
| `pick_folder_root` | — | `{ status: "picked", pickId, path } \| { status: "cancelled" }` |
| `reveal_in_explorer` | `{ scanId, target: { kind: "folder" \| "file", id } }` | `null` |
| `cancel_scan` (existente) | sin cambios | sin cambios |

```
FolderScanSummaryDto = { scanId, status: "done"|"cancelled", startedAt, finishedAt,
  root: { kind: "system_drive"|"picked", path, folderId: number|null },
  volume: { totalBytes, freeBytes } | null, unaccountedBytes: number|null, overCounted: boolean,
  totalBytes, fileCount, dirCount, inaccessibleDirs, reparseSkipped, cloudFiles, opaqueDirs,
  foldersStored, foldersTruncated: boolean, issues: [{ path, reason }] }   // issues <= 50
FolderDto = { id, name, path, sizeBytes, directBytes, fileCount, dirCount,
  access: "complete"|"incomplete"|"inaccessible",
  opaque: "other_user_profile"|"recycle_bin"|"system_volume_information"|null,
  system: boolean, junkHint: boolean, hasChildren: boolean }
FolderPageDto = { scanId, folder: FolderDto, breadcrumb: [{ id, name }], total,
  items: FolderDto[], remainderBytes }          // folderId null = raíz
LargeFileDto = { id, name, path, folderId, sizeBytes, modifiedAt: number|null }
LargeFilePageDto = { scanId, total, items: LargeFileDto[] }
```

- camelCase; ids como enteros JS seguros validados (`<= 2^53 - 1`); `offset >= 0`. Un resumen `cancelled` no tiene carpetas (`root.folderId = null`, contadores del recorrido parcial). Listar sobre un `scanId` que no es el último `done` → `scan_not_found`; `folderId` ajeno → `folder_not_found`; una carpeta opaca no tiene hijos (página vacía, `hasChildren: false`).
- `start_scan(kind = "folders")` **sigue** respondiendo `scan_kind_unsupported` (sin cambios en `start_scan`).
- Se agregan exactamente 6 permisos `allow-*`: 27 en total, 0 `core:*`, 0 plugins.
- Alternativa: `start_scan(kind = "folders")` como dice `CHANGES.md`. Un comando con dos formas de respuesta (`JunkScanSummaryDto` o `FolderScanSummaryDto`) y argumentos que solo valen para un tipo complica decodificadores y tests; C-11 ya eligió un comando propio (`start_duplicate_scan`). Rechazada por consistencia (desvío anotado).

### D-19 — Errores nuevos

`AppError` suma `FolderRootRejected { reason }` (`folder_root_rejected`, `reason` = código de `ProtectedReason` o `not_a_directory`/`not_found`), `FolderPickNotFound` (`folder_pick_not_found`: `pickId` desconocido o reemplazado), `FolderPickerBusy` (`folder_picker_busy`), `FolderPickerUnavailable` (`folder_picker_unavailable`: COM o diálogo falló), `FolderNotFound` (`folder_not_found`) y `RevealUnavailable { reason }` (`reveal_unavailable`: `target_missing`, `opaque`, `different_user`, `undetermined`, `shell_error`). Reutiliza `invalid_argument`, `scan_not_found`, `scan_in_progress`, `cleanup_in_progress`, `recycle_bin_empty_in_progress`, `store_*`. Cada `kind` con texto en `es.errors`; `message` sin rutas. Un fallo por carpeta nunca es `AppError` (es incidencia).

### D-20 — UI: pantalla Espacio en disco **[DUEÑO]**

- Navegación: Inicio, **Espacio en disco**, Limpieza, Duplicados, Configuración (orden del mapa de KB 12; delta de `app-shell`).
- **Simple** (una acción primaria por estado):
  1. Sin análisis: "Mirá qué ocupa lugar en tu disco. Solo se lee: no se borra ni se mueve nada." + **"Analizar disco C:"**.
  2. Analizando: panel `aria-live="polite"` ("Analizando: N archivos, X GB") + "Cancelar".
  3. Resultados: tarjeta resumen ("Disco C: X usados de Y. Analizamos Z en N archivos." + "No contabilizado: W" con ayuda) + **"Lo que más ocupa"**: las 5 carpetas más grandes **de la raíz** con barra, tamaño y etiqueta llana (Sistema, Perfil de otro usuario…) + **"Archivos más grandes"**: los 5 primeros del top con tamaño y fecha. Por elemento: "Mostrar en el Explorador" y, si `junkHint`, "Revisar en Limpieza". Avisos llanos: carpetas sin acceso, archivos en la nube no sumados, modo over-the-shoulder. "Analizar de nuevo" como secundaria.
- **Avanzado** (mismas piezas, más densidad):
  - Antes de analizar: raíz con dos opciones (`RadioGroup`): "Disco C:" o "Una carpeta…" (botón "Elegir carpeta" que abre el selector; muestra la ruta elegida y el motivo si se rechazó). "Analizar" queda deshabilitado con motivo si se eligió "Una carpeta…" y no hay selección válida.
  - Lista navegable: migas de pan, columnas nombre / tamaño + barra / % del padre / archivos / carpetas / estado (con código crudo junto al texto), orden por tamaño o nombre, paginación (50 + "Ver más"), filas "Carpetas más chicas" y "Archivos sueltos".
  - Top de archivos: tabla con nombre, ruta completa, tamaño, fecha; orden por tamaño / más nuevos / más viejos; paginación.
  - Panel de detalles: contadores (inaccesibles, reparse no seguidos, nube, opacas, carpetas guardadas y si se truncó) y la muestra de carpetas sin acceso con su motivo.
- Textos nuevos en `es.disk.*` y `es.errors.*` (sin literales en componentes, `uiLiterals.test.ts`); tamaños con `format.ts`.

### D-21 — Presupuesto de rendimiento (se mide en el smoke)

- Objetivo orientativo (no bloquea specs): `C:` completo (≈ 1 M archivos, ≈ 250 000 carpetas) en ≤ 5 min en release sobre SSD; memoria pico ≤ 200 MB; guardado ≤ 3 s (≤ 20 000 + 500 + 50 filas en una transacción con sentencias preparadas).
- El costo dominante esperado es una apertura por carpeta en `read_dir` (metadatos de la carpeta) más la enumeración; no hay aperturas por archivo.
- Medición: fixture del smoke (tiempos) y, **solo con autorización del dueño**, un análisis de solo lectura del disco real con base temporal. Si supera ~5 min o la memoria se dispara, se vuelve al dueño (opciones: listar con `FindFirstFileExW` + `FIND_FIRST_EX_LARGE_FETCH` sin abrir cada carpeta, o paralelizar el listado) antes de seguir.

### D-22 — Estructura de módulos

```
src-tauri/src/domain/disk_usage/
  mod.rs        // re-exports + contrato (solo lectura; nunca abre archivos; sin borrado)
  model.rs      // RootRequest (SystemDrive | Picked(PickId)), FolderScanResults, FolderNode, LargeFile,
                // FolderIssue, OpaqueReason, Access, contadores, límites (20 000 / 500 / 50)
  root.rs       // resolución de la raíz (D-01, D-03) sobre PathProtection
  aggregate.rs  // FolderAggregator: WalkVisitor (dir_listed, dir_inaccessible, archivos, reparse)
  classify.rs   // opaca / sistema / junk_hint (D-04, D-07 b, D-14) con evaluate_rules y sources
  select.rs     // selección de las 20 000 carpetas (cerrada por ancestros) y resto
  top_files.rs  // montículo acotado de 500
  scanner.rs    // scan_folders: recorrido + consolidación + observer (D-16, D-17)
  *_tests.rs
src-tauri/src/domain/junk/walk.rs            // ganchos dir_listed / dir_inaccessible (vacíos por defecto)
src-tauri/src/domain/protection/service.rs   // check_analysis_root
src-tauri/src/domain/ports/{volume,shell}.rs // VolumeSpace, FolderPicker, Explorer (+ dobles en fakes)
src-tauri/src/domain/ports/store.rs          // FolderStore (+ Store lo incluye)
src-tauri/src/adapters/windows/{volume,folder_picker,explorer}.rs (+ tests)
src-tauri/src/adapters/sqlite/migrations/004_folders.sql, folders.rs (+ contrato)
src-tauri/src/commands/disk_usage.rs         // 6 comandos + DTOs + PickedRootSlot + PickerGate (+ tests)
src/features/disk/                           // DiskScreen, DiskProvider, useFolderScan, useFolderPage,
                                             // useLargeFiles, FolderList, LargeFileList, ProportionBar,
                                             // RootPicker, DiskSummary, diskText.ts
```

- Type-state: `FolderScanResults` solo lo crea `scan_folders` (el store no recibe estructuras armadas a mano fuera de tests); `AnalysisRoot` solo lo crea `check_analysis_root` o la resolución de `system_drive`.

### D-23 — Tests

- Dominio (doble `FakeFileSystem`): **árbol temporal** con tamaños conocidos (sumas por carpeta, `direct_bytes`, cantidades); **junction no seguida** (su destino no suma; `reparse_skipped`); symlink de archivo no sumado; nube con 0 bytes y `cloud_files`; **carpeta sin permisos** (`PermissionDenied` en el listado → `inaccessible`, ancestros `incomplete`, escaneo sigue, incidencia con motivo); carpeta que desaparece (`not_found`); raíz inaccesible; **cancelación** antes del primer listado, a mitad y justo antes de guardar → `Cancelled` sin resultados; progreso por conteo; opacas (perfil ajeno, `$Recycle.Bin`, SVI; mismatch = todos los perfiles) sin rutas internas en carpetas, top ni incidencias; selección de 20 000 con propiedad "cerrada por ancestros" y empates; top 500 con empates; `system` y `junk_hint` (incluido mismatch).
- Núcleo de recorrido: la batería de C-08 y C-11 sin cambios + los ganchos nuevos (orden: `dir_listed` antes de los archivos de esa carpeta; `dir_inaccessible` con el error).
- `check_analysis_root` (tests de tabla): `C:\`, `C:\Windows`, `C:\Program Files`, perfil propio, Descargas → aceptadas; `C:\Users\bob`, `D:\Datos`, `\\srv\x`, `C:\$Recycle.Bin`, junction, carpeta inexistente, archivo, contexto inválido → rechazadas con su motivo; en mismatch, el perfil propio → `other_user_profile`.
- Persistencia (doble + SQLite temporal, batería de contrato): guardado atómico, padre antes que hija, raíz única, `folder_id`/`parent_id` de otro escaneo rechazados, > 50 incidencias rechazadas, purga del análisis anterior conservando su resultado, hallazgos y duplicados intactos, paginación/orden de hijos y archivos, `running` → `error` sin datos, cascada por retención; esquema: `CHECK`s y migración v1/v2/v3 → v4 conservando datos.
- Adaptadores (solo `tempdir`, corren siempre): listado de un árbol temporal con junction y con carpeta denegada por `icacls` (patrón de `fs_tests.rs`, ACL restaurada al final); `VolumeSpace` sobre la unidad del `tempdir` (total > 0, libre ≤ total). `#[ignore = "abre UI nativa; se prueba en el smoke"]`: selector y Explorador (solo se verifica que compilan; su lógica está detrás de puertos con dobles).
- Arquitectura: dominio sin `std::fs`/`windows`/`rusqlite`/`tauri` (incluye `disk_usage/`); ningún `ShellExecute*` ni `remove_*` nuevo en producción; `unsafe` nuevo solo en `volume.rs`, `folder_picker.rs`, `explorer.rs`.
- Comandos: DTOs exactos, raíz `picked` con `pickId` desconocido → `folder_pick_not_found`, raíz que dejó de ser válida → `folder_root_rejected`, exclusividad, cancelación, `list_*` con `scanId` viejo / id ajeno / `limit` fuera de rango, `reveal_in_explorer` (opaca, mismatch, ausente, error del shell, ok con doble), selector (cancelado, elegido válido, rechazado, ocupado, error), `start_scan("folders")` sigue `scan_kind_unsupported`, baseline de capabilities (27 permisos).
- Frontend: decodificadores; pantalla simple (acción primaria por estado, top 5 + 5, avisos, "Revisar en Limpieza" navega, "Mostrar en el Explorador" llama al comando), avanzado (selector de raíz y su rechazo, navegación con migas, orden, paginación, filas de resto, top ordenable, detalles), progreso y cancelación, errores del catálogo, navegación con 5 entradas, `ProportionBar` sin `style` en línea, `uiLiterals`, contraste.

### D-24 — Dependencias y features

Ninguna dependencia nueva (npm ni Cargo). En el apply (tarea 1.2) se confirman con `winmd-api-search` y el fuente de `windows` 0.62.2: `GetDiskFreeSpaceExW` (`Win32_Storage_FileSystem`), `FileOpenDialog`/`IFileOpenDialog`/`IShellItem`/`FOS_*`/`SIGDN_FILESYSPATH`, `SHParseDisplayName`/`SHOpenFolderAndSelectItems` (`Win32_UI_Shell`, posiblemente `Win32_UI_Shell_Common` para `ITEMIDLIST`), `CoCreateInstance`/`CoTaskMemFree` (`Win32_System_Com`) y cómo obtener el `HWND` de la ventana principal (`WebviewWindow::hwnd()` de Tauri devuelve el tipo de su propia versión de `windows`: convertir por el valor del puntero). Habilitar una feature de `windows` no agrega paquetes a `Cargo.lock`; si alguna exigiera un paquete nuevo, DETENER y consultar.

### D-25 — Smoke guiado (manual, con el dueño) **[DUEÑO]**

Se escribe `smoke-guide.md` en el apply (patrón de C-10/C-11). Reglas:

1. Un paso por vez; el dueño confirma antes del siguiente.
2. **Aviso y cuenta atrás real de 10 s** (`for ($i=10; $i -gt 0; $i--) { Write-Host $i; Start-Sleep 1 }`) antes de cada lanzamiento de la UI o de UAC.
3. Sin capturas de pantalla: verificación por la base en solo lectura (`python -I`, `mode=ro`), el sistema de archivos y, si el dueño lo delega, el DOM por CDP (`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222` + `playwright-core` `connectOverCDP` desde una carpeta scratch; solo texto, roles y estados). El diálogo nativo del selector y la ventana del Explorador no son del webview: los maneja el dueño.
4. **Raíz verificada antes de cada análisis**: antes de pulsar "Analizar", el agente lee en el DOM (o el dueño confirma) que la raíz elegida es la carpeta del smoke. Las carpetas reales del dueño (incluido `C:` completo) **nunca** se analizan sin su autorización explícita en ese paso.
5. Base temporal con `PCOPT_DATA_DIR` (nunca la base real); terminal NO elevada salvo la variante elevada.
6. Si algo no coincide, se detiene y se anota.

- Fixture (la crea el agente): `%USERPROFILE%\Downloads\pcopt-smoke-c07\` con `big\pcopt-smoke-big.bin` (64 MiB, fecha de hace 30 días), `medium\nested\` (3 × 8 MiB), `small\` (100 × 4 KiB), `empty\`, `recent.bin` (2 MiB, fecha actual), `locked\` (16 MiB adentro, luego `icacls /deny` de listado) y la junction `link-outside` → `%USERPROFILE%\Downloads\pcopt-smoke-c07-outside\` (32 MiB, no debe sumarse). Segunda fixture `pcopt-smoke-c07-many\` (30 000 archivos de 1 byte) para cancelar.
- Pasos: build y arranque en desarrollo; pantalla en simple (solo texto y botón, **sin pulsar**); avanzado → "Una carpeta…" → selector (el dueño elige la fixture) → raíz verificada → analizar; verificación de sumas, `locked` inaccesible con `access_denied`, junction no seguida (`reparse_skipped = 1`, 32 MiB ausentes), top de archivos por tamaño y por fecha, filas de resto; navegación y migas; "Mostrar en el Explorador" sobre `pcopt-smoke-big.bin` (el dueño confirma que se abrió la carpeta con el archivo seleccionado y que nada se ejecutó); selector sobre la junction → rechazo `reparse_point`; cancelación con la fixture `many` (resultado `cancelled`, el análisis anterior sigue visible); tiempos. Opcionales, cada uno con autorización explícita: análisis de solo lectura de `C:` completo (tiempos, memoria, carpetas guardadas, "no contabilizado"); variante elevada (release, `match`): selector y "Mostrar en el Explorador" desde el proceso elevado.
- Cierre: el agente restaura la ACL de `locked` (`icacls /remove:d`) y quita la junction con `rmdir` sin `/s`; **la regla global del dueño bloquea `rm -rf`**, así que el agente le entrega el comando exacto para borrar las dos fixtures y la base temporal (`Remove-Item -LiteralPath … -Recurse -Force`) y el dueño lo corre.

## Risks / Trade-offs

- [Listar nombres de archivos de otros usuarios desde una app elevada] → carpetas opacas (D-04), sin rutas internas en base, eventos, top ni incidencias; selector rechaza perfiles ajenos; mismatch = todos los perfiles opacos.
- [Extensiones de shell de terceros cargadas en el proceso elevado por el selector o `SHParseDisplayName`] → `FOS_PICKFOLDERS` (sin vista previa), hilo propio con `catch_unwind`; mismo riesgo que el plugin `dialog`; documentado en KB 11/13.
- [Ejecutar un archivo al "mostrarlo"] → solo `SHOpenFolderAndSelectItems`; test de arquitectura sin `ShellExecute*`; ruta por id del último análisis, re-verificada sin seguir reparse.
- [Tamaños distintos de "espacio usado" (hard links de WinSxS, compresión, nube descargada)] → tamaño lógico explicado, "no contabilizado" acotado a 0 con nota (D-06).
- [Análisis largo o con mucha memoria en discos grandes] → progreso, cancelación entre entradas, tope de 20 000 carpetas persistidas, medición con autorización y contingencia (D-21).
- [CSP estricta y barras] → SVG con atributos (sin `style` en línea); test que lo verifica y comprobación en release en el smoke.
- [Cambiar el núcleo de recorrido compartido con C-08/C-11 (código HIGH ya verificado)] → ganchos con implementación vacía por defecto; las baterías de C-08/C-11 son la red de seguridad (línea base en la tarea 1.1).
- [`SHOpenFolderAndSelectItems` desde un proceso elevado] → el Explorador no corre elevado y atiende la petición con el shell del usuario; se verifica en la variante elevada del smoke; en mismatch está bloqueado.
- [`HWND` de Tauri con otro tipo de `windows`] → conversión por el valor del puntero, confirmada en la tarea 1.2.
- [Archivos que cambian durante el análisis] → es una foto aproximada; las carpetas que desaparecen se informan `not_found`; nada se borra, así que no hay riesgo de TOCTOU destructivo.

## Migration Plan

- Esquema v3 → v4 con `004_folders.sql` (solo tablas nuevas; no toca `scan`, `cleanup_log`, `junk_*` ni `duplicate_*`). Bases v1/v2/v3 se migran al abrir; una base v4 abierta por una versión anterior da "base de una versión más nueva" (requisito existente) → recuperación manual desde Configuración.
- Capabilities: +6 permisos `allow-*`; sin plugins ni cambios de CSP.
- Docs a actualizar en el apply (tarea 1.3): `CHANGES.md` (C-07 en curso + decisiones; `walkdir` sin consumidor; nota para C-12: el dashboard puede consumir `get_last_folder_scan`; nota para C-25: retención e índice para tendencias), KB 04 (tablas reales), KB 02 (comandos reales), KB 05 (reglas del análisis: solo lectura, opacas, raíces), KB 06 US-006/US-007 (criterios reales), KB 10 (PA-16 actualizada: el selector existe para el análisis; duplicados siguen limitados), KB 11 y KB 13 (selector y Explorador en proceso elevado, sin plugins), KB 12 (pantalla real y treemap diferido).

## Decisiones del dueño (bloquean el apply)

**Aprobadas por el dueño el 2026-11-14** ("apruebo"): todas las decisiones D-01..D-25 según la recomendación (D-01 A, D-02 A, D-03 A, D-04 A, D-05 A, D-06 A, D-07 A, D-08 A, D-09 A, D-12 A, D-13 A, D-14 A, D-18 A, D-20, D-25 tal como están escritas).

| ID | Decisión | Recomendación |
|---|---|---|
| D-01 | Alcance | `C:\` por defecto (simple y avanzado) + una carpeta elegida del volumen del sistema (solo avanzado); rutas protegidas recorridas en solo lectura |
| D-02 | Selector | `IFileOpenDialog` propio en un adaptador; la interfaz solo recibe y envía un `pickId`; sin plugin `dialog` |
| D-03 | Validación de raíz | `check_analysis_root` en `path-protection`: acepta sistema/programas/perfil propio, rechaza perfiles ajenos, otros volúmenes, entradas de sistema de la raíz y reparse |
| D-04 | Privacidad | Perfiles ajenos, `$Recycle.Bin` y SVI opacos: tamaño sí, rutas internas nunca; en mismatch todos los perfiles |
| D-05 | Recorrido | Núcleo de C-08/C-11 con 2 ganchos opcionales; `walkdir` sin usar (quitarla: decisión aparte) |
| D-06 | Tamaños | Lógico; hard links por nombre; nube 0 bytes con contador; "no contabilizado" contra el espacio del volumen |
| D-07 | Granularidad | Hasta 20 000 carpetas (todas si hay menos), las más grandes, cerradas por ancestros |
| D-08 | Top de archivos | 500 más grandes, ordenables por tamaño y fecha dentro del top |
| D-09 | Migración | `004_folders.sql` con 4 tablas (resultado, carpetas, archivos grandes, incidencias) |
| D-12 | Visualización | Lista con barras SVG sin librería; treemap diferido; PA-01 no se fuerza |
| D-13 | Explorador | `SHOpenFolderAndSelectItems` por id del último análisis; nunca ejecuta; bloqueado en mismatch |
| D-14 | Revisión de limpieza | Solo enlace "Revisar en Limpieza" en ubicaciones de la whitelist; sin camino de borrado nuevo |
| D-18 | IPC | 6 comandos nuevos + 6 permisos (27); `start_scan` sin cambios |
| D-20 | UI | Pantalla "Espacio en disco" (simple: resumen + 5 carpetas + 5 archivos; avanzado: raíz, lista navegable, top, detalles) |
| D-25 | Smoke | Fixtures en `Descargas\pcopt-smoke-c07*`, raíz verificada antes de analizar, `C:` real solo con autorización, cierre con comando para el dueño |

## Desvíos respecto de CHANGES.md / KB

- `CHANGES.md` C-07 "Recorrido con `walkdir`" → núcleo de recorrido sobre el puerto `FileSystem` (D-05).
- `CHANGES.md` C-07 "`start_scan(kind=folders)`" → comando propio `start_folder_scan`; `start_scan` sin cambios (D-18).
- `CHANGES.md` C-07 / KB 12 "treemap/lista" → lista con barras; treemap diferido (D-12).
- `CHANGES.md` C-07 "enviar a revisión de limpieza" → enlace a Limpieza solo para ubicaciones de la whitelist (D-14).
- KB 04 `folder_snapshot` → columnas y tablas adicionales (D-09); retención "~90 días" → solo el último análisis hasta C-25 (D-10).
- US-007 "ordenable por tamaño y fecha" → dentro del top de 500 (D-08).

## Preguntas abiertas de KB 10 que toca este change

- **PA-01** (librería de gráficos): no se fuerza; la lista con barras no usa librería. El treemap queda para cuando C-06 decida PA-01.
- **PA-12** (otros volúmenes): solo el volumen del sistema; el selector rechaza otros volúmenes con `unsupported_volume`.
- **PA-13** (retención): solo el último análisis conserva carpetas y archivos; C-25 definirá la retención para tendencias.
- **PA-16** (carpetas arbitrarias): el selector propio queda implementado para el análisis (solo lectura); permitirlo en duplicados sigue siendo decisión de PA-16 (se actualiza su texto en el apply).
- **PA-14** (resuelta): aplicada como "perfiles opacos" y Explorador bloqueado en mismatch (D-15).
- **PA-08** (Windows 10): `IFileOpenDialog`, `SHOpenFolderAndSelectItems` y `GetDiskFreeSpaceExW` existen desde Vista; sin diferencias esperadas.
- **PA-03** (apps sin uso): el análisis solo enumera (no abre archivos), así que no altera la fecha de último acceso de los archivos.
- **IN-01** (simple vs avanzado): pantalla con ambos modos; revisión UX en la tarea 8.8.
- **IN-03** (siempre admin): el selector y el Explorador agregan COM del shell al proceso elevado (riesgo de extensiones de terceros, ver Risks).

## Open Questions

- Cadencias de progreso (4 096 archivos / 512 carpetas): ajustables en el apply según la fluidez observada; no cambian specs.
- Tamaño de página de la UI (50 + "Ver más") y textos exactos: se ajustan con la revisión UX (tarea 8.8).
- Carpeta inicial del selector (perfil del usuario o la última elegida): detalle del adaptador, sin impacto en specs.

## Apply notes

### 1.1 Línea base (antes de tocar código)

| Suite | Resultado |
|---|---|
| Vitest (`npx vitest run`) | 466 passed, 23 files |
| cargo lib | 935 passed, 19 ignored (954) |
| `--test architecture` | 19 passed |
| `--test manifest` | 5 passed |
| `--test no_network_crates` | 9 passed |
| `--test security_baseline` | 22 passed |
| `domain::junk` (red de 2.1) | 73 passed |
| `domain::duplicates` (red de 2.1) | 107 passed |
| `cargo fmt --check`, `clippy --all-targets --all-features --locked -D warnings`, `npm run build` | verdes |

Nota: 9 archivos de `src-tauri/permissions/autogenerated/` aparecen modificados solo por fin de línea (ignorados).

### 1.2 APIs de Windows confirmadas (fuente de `windows-0.62.2` + `winmd-api-search`)

| API | Módulo (`windows::Win32::…`) | Feature |
|---|---|---|
| `GetDiskFreeSpaceExW(dir, Option<*mut u64> x3) -> Result<()>` | `Storage::FileSystem` | `Win32_Storage_FileSystem` (ya activa) |
| `FileOpenDialog` (CLSID), `IFileOpenDialog`, `IFileDialog::{SetOptions(FILEOPENDIALOGOPTIONS), GetResult}`, `IModalWindow::Show(hwnd)`, `FOS_PICKFOLDERS/FORCEFILESYSTEM/PATHMUSTEXIST/NOCHANGEDIR/DONTADDTORECENT`, `IShellItem::GetDisplayName(SIGDN_FILESYSPATH) -> PWSTR`, `SIGDN_FILESYSPATH` | `UI::Shell` | `Win32_UI_Shell` (ya activa) |
| `SHParseDisplayName(name, bc, *mut *mut ITEMIDLIST, sfgaoin, Option<*mut u32>)`, `SHOpenFolderAndSelectItems(pidl, Option<&[*const ITEMIDLIST]>, flags)` | `UI::Shell` | `Win32_UI_Shell` |
| `ITEMIDLIST` | `UI::Shell::Common` | **`Win32_UI_Shell_Common` (agregada)** |
| `CoCreateInstance`, `CoTaskMemFree`, `CoInitializeEx`, `CoUninitialize` | `System::Com` | `Win32_System_Com` (ya activa) |
| `ERROR_CANCELLED` (1223); `HRESULT_FROM_WIN32(1223)` = `0x800704C7` | `Foundation` | `Win32_Foundation` |
| `HWND` de la ventana principal | `WebviewWindow::hwnd() -> Result<HWND>` de Tauri 2.12.5 | `Cargo.lock` solo tiene `windows 0.62.2`: es el mismo tipo; igualmente se pasa como `isize` (valor del puntero) al adaptador |

`git diff --exit-code src-tauri/Cargo.lock` limpio tras agregar `Win32_UI_Shell_Common` (D-24).

### 2.1 Ganchos del recorrido — RED

`cargo test --lib --locked domain::junk::walk` (4 tests nuevos en `walk_tests.rs`, antes del código de producción):

```
error[E0407]: method `dir_listed` is not a member of trait `WalkVisitor`
error[E0407]: method `dir_inaccessible` is not a member of trait `WalkVisitor`
error: could not compile `pc-optimizer` (lib test) due to 2 previous errors
```
GREEN 2.1: walk 23 passed, domain::junk 77 (73 + 4), domain::duplicates 107 (sin cambio).

### 2.2 `check_analysis_root` — RED

`cargo test --lib --locked domain::protection::service` (7 tests nuevos, antes del código):

```
error[E0599]: no method named `check_analysis_root` found for struct `service::PathProtection<'a, F>` in the current scope
   --> src\domain\protection\service_tests.rs:912:18
```

### 2.3 Puertos `VolumeSpace`, `FolderPicker`, `Explorer` — RED

`cargo test --lib --locked domain::ports::shell` (6 tests de dobles en `shell_ports_tests.rs`, con `shell.rs`/`volume.rs`/`fakes_shell.rs` vacíos):

```
error[E0432]: unresolved imports `shell::Explorer`, `shell::FolderPicker`
error[E0432]: unresolved imports `volume::VolumeSpace`, `volume::VolumeSpaceInfo`
error[E0432]: unresolved imports `crate::domain::ports::fakes_shell::FakeExplorer`, `...::FakeFolderPicker`, `...::FakeVolumeSpace`
error: could not compile `pc-optimizer` (lib test) due to 3 previous errors
```

### 3.1 `disk_usage::model` — RED

`cargo test --lib --locked domain::disk_usage::model` (7 tests en `model_tests.rs`, `model.rs` sin símbolos):

```
error[E0425]: cannot find type `RootRequest` in this scope
error[E0425]: cannot find value `MAX_ISSUES` in this scope
error[E0425]: cannot find value `MAX_STORED_FOLDERS` in this scope
error[E0433]: cannot find type `Access` in this scope  (3x) / `IssueReason` (9x) / `OpaqueReason` (5x) / `FolderSort` (5x) / `LargeFileSort` (7x)
error: could not compile `pc-optimizer` (lib test) due to 48 previous errors
```

### 3.2 `disk_usage::classify` — RED

`cargo test --lib --locked domain::disk_usage::classify` (7 tests en `classify_tests.rs`, `classify.rs` vacío):

```
error[E0433]: cannot find type `Classifier` in this scope   (7x)
error[E0433]: cannot find type `OpaqueReason` in this scope  (7x)
error: could not compile `pc-optimizer` (lib test) due to 14 previous errors
```

### 3.3 `disk_usage::aggregate` — RED

`cargo test --lib --locked domain::disk_usage::aggregate` (11 tests en `aggregate_tests.rs`, `aggregate.rs` solo con el `mod tests`):

```
error[E0422]: cannot find struct, variant or union type `FolderIssue` in this scope
error[E0425]: cannot find type `FolderTree` in this scope   (3x)
error[E0425]: cannot find type `TreeNode` in this scope
error[E0433]: cannot find type `FolderAggregator` in this scope
error: could not compile `pc-optimizer` (lib test) due to 6 previous errors
```
GREEN 3.2: 7/7; GREEN 3.3: 11/11 (decisión menor: los marcadores de nube cuentan solo en cloud_files, no en file_count).

### 3.4 `disk_usage::select` — RED

`cargo test --lib --locked domain::disk_usage::select` (6 tests, incluida la propiedad "cerrada por ancestros" sobre 4 árboles generados con semilla fija y 5 topes):

```
error[E0425]: cannot find function `remainder_bytes` in this scope   (4x)
error[E0425]: cannot find function `select_folders` in this scope     (5x)
error[E0425]: cannot find type `Selection` in this scope
error[E0425]: cannot find value `MAX_STORED_FOLDERS` in this scope
error: could not compile `pc-optimizer` (lib test) due to 11 previous errors
```

### 3.5 `disk_usage::top_files` — RED

`cargo test --lib --locked domain::disk_usage::top_files` (7 tests; `top_files.rs` solo con el `mod tests`):

```
error[E0425]: cannot find function `nearest_kept` in this scope   (5x)
error[E0425]: cannot find type `TopFile` in this scope / `TopFiles` (1x + 4x E0433)
error[E0609]: no field `top` on type `aggregate::FolderTree`      (7x)
error: could not compile `pc-optimizer` (lib test) due to 18 previous errors
```

### 4.1 `disk_usage::scanner` — RED

`cargo test --lib --locked domain::disk_usage::scanner` (14 tests en `scanner_tests.rs`; `scanner.rs` solo con el `mod tests`):

```
error[E0405]: cannot find trait `FolderScanObserver` in this scope
error[E0425]: cannot find function `scan_folders` in this scope
error[E0425]: cannot find type `FolderScanError` / `FolderScanOutcome` / `FolderScanResults` / `RootChoice` / `StoredFolder` in this scope
error[E0432]: unresolved imports `...::model::FolderCounters`, `...::model::unaccounted`
error: could not compile `pc-optimizer` (lib test) due to 28 previous errors
```
GREEN 4.1: domain::disk_usage 53 passed (14 de scanner).

### 4.2 Cancelación y progreso — RED (desvío de orden, reconocido)

Al escribir 4.1 el scanner ya incluía el progreso por conteo y las consultas de cancelación, por lo que los 6 tests de 4.2 se escribieron con el código presente (incumple "RED antes del código"). Para no dejar la tarea sin evidencia se obtuvo un RED por mutación: con `PROGRESS_EVERY_FILES = PROGRESS_EVERY_DIRS = u64::MAX` y sin la consulta de cancelación posterior al recorrido, `cargo test --lib --locked domain::disk_usage::scanner` falla:

```
test ...scan_should_cancel_right_before_building_the_result ... FAILED   (left: Ok(Completed(FolderScanResults {...})), right: Ok(Cancelled))
test ...scan_should_report_progress_every_4096_files_with_growing_counts ... FAILED
test ...scan_should_report_progress_every_512_directories ... FAILED
test ...scan_should_cancel_in_the_middle_of_the_walk ... FAILED
```

Se restauró el código real (`scanner.rs` idéntico al de 4.1) y los 21 tests de `scanner` pasan (53 + 6 = 59 en `domain::disk_usage`).

### 4.3 Refactor y contrato; cierre de la Parte 1

Doc de contrato en `domain/disk_usage/mod.rs`; `cargo fmt` aplicado; el test de arquitectura recorre `domain/` recursivamente, así que `disk_usage/` ya está cubierto. Gates de la Parte 1: cargo lib 1012 passed / 19 ignored (935 + 77 nuevos), architecture 19, manifest 5, no_network_crates 9, security_baseline 22; fmt, clippy `-D warnings`, `openspec validate --strict` verdes; `Cargo.lock` y `package-lock.json` sin cambios.

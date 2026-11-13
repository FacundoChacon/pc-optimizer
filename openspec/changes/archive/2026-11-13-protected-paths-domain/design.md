# Design

## Context

- **Estado observado**: C-02 dejó `domain/ports/fs.rs` con `FileSystem { canonicalize, metadata, read_dir }`, `FileMeta { kind, size_bytes, modified, cloud_placeholder }`, `EntryKind { File, Directory, ReparsePoint }` y `FakeFileSystem` (mapa exacto por `PathBuf`, `canonicalize` solo con mapeo explícito). **No existe adaptador real** de `FileSystem`: C-02 lo asignó a C-03 ("C-03 puede agregar campos a `FileMeta`"). `CleanupOutcome` ya tiene `SkippedProtected` y `SkippedCloud`.
- `tests/architecture.rs` prohíbe `std::fs|process|net`, `windows`, `windows_sys`, `rusqlite`, `tauri` en `src/domain`.
- El crate `windows = "=0.62.2"` está fijado **sin features**; `unused_dependencies = "allow"` lo tolera hasta su primer consumidor (este change).
- La app corre **siempre elevada** (KB 13). Amenaza central: un proceso del usuario sin privilegios planta junctions/symlinks/hard links o corre una carrera (TOCTOU) para que el proceso elevado borre algo protegido. Es la clase "arbitrary file delete → EoP".
- KB 11 §Algoritmo pide un `is_protected` "puro" que además canonicaliza (I/O): contradicción que este diseño resuelve partiendo en dos capas.

## Goals / Non-Goals

**Goals:**
- Decisión de protección fail-closed de punta a punta, con motivo explicable.
- Reglas puras, testeables con contextos falsos (incluida una unidad de sistema distinta de `C:`).
- Semántica exacta y conservadora para reparse points, 8.3, mayúsculas, ADS, UNC/dispositivo, nube, hard links.
- Primitiva anti-TOCTOU reutilizable que C-09 solo tenga que envolver alrededor de `move_to_trash`.
- Primer adaptador real de `FileSystem` + resolución del contexto con Known Folder APIs.

**Non-Goals:**
- Categorías de basura y sus raíces (C-08), recorrido de árboles (C-07/C-08), mover a la Papelera y el adaptador `Trash` (C-09).
- Detectar el usuario de la sesión interactiva (C-05): se deja la costura.
- Soportar volúmenes distintos del sistema (PA-12): fuera hasta que se resuelva.
- Comandos IPC o UI: C-03 no tiene superficie hacia el webview.

## Decisions

### D-01 — Pureza vs canonicalización: dos capas, fail-closed en ambas
- **Capa pura** (`domain/protection/{path,context,rules}.rs`): `normalize_lexical(&str) -> Result<NormalizedPath, PathRejection>`, `ProtectionContext::from_raw(RawProtectionContext) -> Result<_, ContextError>`, `evaluate_rules(&ProtectionContext, &NormalizedPath) -> RuleVerdict` y el atajo `is_protected(ctx, path) -> bool` (= `!matches!(verdict, RuleVerdict::Allowed(_))`). Sin I/O; son el "is_protected puro" de CHANGES.md.
- **Servicio** (`domain/protection/service.rs`): `PathProtection<'a, F: FileSystem>` recibe el puerto y un `Result<ProtectionContext, ContextError>`; hace la resolución con I/O y combina. También es dominio (solo usa el trait).
- **Fail-closed por construcción**: el servicio devuelve `ProtectionDecision`, nunca `Result`. Todo `Err` (parseo, contexto, `PortError`, identidad ausente, mismatch) se convierte en `Protected(reason)` en un único punto (`fn deny(reason)`), y el único constructor de `Allowed` está al final del camino feliz. Un test recorre cada `PortError` × cada llamada al puerto (inyectado con `fail_on`) y exige `Protected`.
- Alternativa descartada: `is_protected(fs, path)` monolítico. Mezcla reglas e I/O y obliga a fakes para testear reglas.

### D-02 — Algoritmo del servicio (`check`)
```
check(raw):
 1. lexical = normalize_lexical(raw)            err → Protected(Unparseable(r))
 2. ctx ok?                                     err → Protected(ContextUnavailable)
 3. evaluate_rules(ctx, lexical)                Protected → devolverlo (sin I/O)
 4. walk: para cada ancestro estricto A de lexical (sin la raíz del volumen):
        m = fs.metadata(A)                      err → Protected(Unresolvable(kind))
        m.cloud_placeholder                     → SkippedCloud
        m.kind == ReparsePoint                  → Protected(ReparsePointInPath)
        m.kind != Directory                     → Protected(Unresolvable(NotADirectory))
 5. t = fs.metadata(lexical)                    err → Protected(Unresolvable(kind))
 6. real = normalize_lexical(fs.canonicalize(lexical))   err → Protected(Unresolvable/ResolvedPathMismatch)
    same_shape(lexical, real) (D-06)            no → Protected(ResolvedPathMismatch)
 7. evaluate_rules(ctx, real)                   Protected → devolverlo
 8. t.cloud_placeholder                         → SkippedCloud
    t.kind == ReparsePoint                      → Protected(ReparsePoint)
    t.hard_links != Some(1) (archivos)          → Protected(MultipleHardLinks | Unresolvable(MissingIdentity))
    t.identity == None                          → Protected(Unresolvable(MissingIdentity))
 9. Allowed(CheckedPath { path: real, kind, size_bytes, identity, rule: exception/default })
```
- Las reglas se evalúan sobre la ruta léxica (paso 3) **y** la real (paso 7): basta que una proteja (defensa en profundidad; además el paso 3 evita I/O para lo obviamente protegido).
- Orden nube vs protegido: lo protegido gana (pasos 3/7 antes que 8); la nube solo convierte un "elegible" en `SkippedCloud`. Un ancestro de nube da `SkippedCloud` (paso 4) solo si la ruta léxica ya pasó las reglas.
- `CheckSession` (memo opcional de ancestros verificados como directorios no-reparse, clave = ruta plegada) para escaneos masivos de C-07/C-08; vive lo que dura un escaneo. **`reverify`/`prepare_delete` nunca usan memo.**

### D-03 — Reparse points: nunca atravesar, nunca elegibles (lo más conservador)
| Situación | Decisión |
|---|---|
| Algún ancestro es reparse point (junction, symlink, mount point, cualquier tag) | `Protected(ReparsePointInPath)` (o `SkippedCloud` si el tag/atributos son de nube) |
| El objetivo es reparse point no-nube | `Protected(ReparsePoint)` — **ni el enlace ni su destino** son elegibles |
| El objetivo es reparse de nube (OneDrive) | `SkippedCloud` |
| Ruta real ≠ ruta léxica (salvo 8.3/mayúsculas) | `Protected(ResolvedPathMismatch)` |
- **Qué ruta se chequea**: ambas. Se exige además que sean "la misma" (D-06), así que en la práctica la ruta elegible es la que se ve, sin redirecciones ("lo que ves es lo que se borra").
- **Por qué no permitir borrar el enlace en sí**: aunque mover un symlink/junction a la Papelera mueve el enlace y no el destino en Windows moderno, (a) depende del comportamiento del shell/`IFileOperation` y de la restauración desde la Papelera, (b) libera ~0 bytes, (c) SU-09/RN-PROT-04 ya aceptan subestimar. Beneficio nulo, riesgo no nulo. Si un consumidor lo necesitara, requiere un change que lo especifique.
- `canonicalize` del adaptador abre con `FILE_FLAG_OPEN_REPARSE_POINT` (no sigue el último componente) y devuelve `GetFinalPathNameByHandleW(FILE_NAME_NORMALIZED | VOLUME_NAME_DOS)`: los reparse intermedios quedan resueltos en la respuesta, por eso la comparación léxica/real detecta cualquier redirección que el walk (paso 4) no haya visto por carrera. Montajes sin letra o volúmenes GUID → la ruta real no tiene unidad → error → protegido.
- **Hard links**: un archivo con `hard_links > 1` es `Protected(MultipleHardLinks)` (otro nombre puede estar en System32; trashear un nombre no libera espacio). Directorios: no aplica.

### D-04 — Modelo de reglas, excepciones y precedencia
Reglas como **datos relativos a anclas del contexto** (nunca letras de unidad). Orden de evaluación (primera coincidencia gana dentro de cada nivel):

1. **No interpretable / contexto inválido** → `Unparseable(_)` / `ContextUnavailable`.
2. **Denegaciones duras** (ninguna excepción las anula):
   - `UnsupportedVolume`: unidad ∉ `ctx.supported_volumes` (por defecto solo la del sistema, PA-12).
   - `VolumeRoot`: `X:\`.
   - `VolumeSystemEntry`: primer componente bajo la raíz de **cualquier** volumen que empiece con `$` o esté en la lista cerrada (`System Volume Information`, `pagefile.sys`, `hiberfil.sys`, `swapfile.sys`, `DumpStack.log`, `DumpStack.log.tmp`, `Recovery`, `Boot`, `bootmgr`, `BOOTNXT`, `Config.Msi`, `Documents and Settings`) y todo su contenido.
   - `Anchor(kind)`: igualdad exacta con un ancla (D-05) o con la raíz de una excepción.
   - `OtherUserProfile`: dentro de `users_root` y fuera del perfil actual (incluye `Public`, `Default`, `All Users`). Sin usuario actual → todo `users_root` es ajeno.
3. **Excepciones** (lista cerrada, `enum ExceptionId`): descendiente **estricto** de la raíz → `Allowed(Exception(id))`.
4. **Raíces protegidas**, en este orden: `WindowsDir`, `ProgramFiles` (cada una), `ProgramData`, `BrowserProfile(browser)`, `AppDir`.
5. **Resto** → `Allowed(Default)`. La protección es una lista de denegación con huecos cerrados; la lista positiva de qué limpiar es de C-08 (DD-06). Ambas capas son necesarias: C-08 elige, C-03 veta.

Raíces protegidas y excepciones iniciales:
| Raíz protegida (ancla) | Excepciones (subárbol estricto) | Consumidor |
|---|---|---|
| `<windows>` | `Temp`; `SoftwareDistribution\Download`; `Minidump` | `temp_windows`, `wu_cache`, `crash_dumps` |
| `<program_files>` × N | — | — |
| `<program_data>` | `Microsoft\Windows\WER\ReportArchive`; `Microsoft\Windows\WER\ReportQueue` | `crash_dumps` |
| `<local>\Google\Chrome\User Data` | `*\Cache`; `*\Code Cache`; `*\GPUCache` | `browser_cache_chrome` |
| `<local>\Microsoft\Edge\User Data` | ídem | `browser_cache_edge` |
| `<local>\BraveSoftware\Brave-Browser\User Data` | ídem | `browser_cache_brave` |
| `<roaming>\Mozilla\Firefox\Profiles` | — | — |
| `<local>\Mozilla\Firefox\Profiles` | `*\cache2` | `browser_cache_firefox` |
| `<app_dirs>` (datos SQLite, instalación) | — | — |

- `*` = exactamente un componente cualquiera (nombre de perfil: `Default`, `Profile 3`, `ab12.default-release`). Sin `**`, sin comodines parciales.
- **Tensión perfil vs caché**: el perfil de navegador es raíz protegida; la caché es una excepción **dentro** de esa raíz. Como las excepciones solo actúan en el nivel 3 y nunca sobre denegaciones duras, una caché de otro usuario sigue protegida, y `User Data\Default\Network\Cookies`, `Login Data`, `History`, `Extensions` quedan protegidos.
- **Dueño de la lista**: C-03, como `const` en `rules.rs` + `enum ExceptionId` exhaustivo. C-08 **no** puede agregar huecos: solo produce candidatos que pasan por `check`. Ampliar la lista = delta spec sobre `path-protection` aprobada por el dueño (RN-PROT-06). `thumbnail_cache` (`<local>\Microsoft\Windows\Explorer`), `temp_user` y `%LOCALAPPDATA%\CrashDumps` no necesitan excepción (no están bajo raíces protegidas). `old_logs` bajo `C:\Windows\Logs` **no** está habilitado: si C-08 lo quiere, debe proponerlo.
- **Reglas que apuntan a `C:\`, `C:\Users`, `C:\Windows`**: `check_rule_root` (D-09) las rechaza; además `check` sobre esas rutas da `VolumeRoot`/`Anchor`.

### D-05 — Contexto inyectado (`ProtectionContext`) y costura de C-05
```rust
pub struct RawProtectionContext {           // lo llena el adaptador; strings crudos
    pub windows_dir: Option<String>,        // FOLDERID_Windows (obligatoria)
    pub program_files: Vec<String>,         // FOLDERID_ProgramFiles, ProgramFilesX86, ProgramFilesX64 (≥1 obligatoria)
    pub program_data: Option<String>,       // FOLDERID_ProgramData (obligatoria)
    pub users_root: Option<String>,         // FOLDERID_UserProfiles (obligatoria)
    pub user: Option<RawUserFolders>,       // None ⇒ todo users_root es ajeno
    pub app_dirs: Vec<String>,              // datos de la app + carpeta del ejecutable
}
pub struct RawUserFolders {
    pub profile: String, pub local_app_data: String, pub roaming_app_data: String,
    pub temp: Option<String>, pub content_folders: Vec<String>, // Desktop, Documents, Downloads, Music, Pictures, Videos
    pub source: UserSource,                 // ProcessToken (C-03) | InteractiveSession (C-05)
}
```
- `ProtectionContext::from_raw` normaliza cada ruta con `normalize_lexical`; la **unidad del sistema** es la de `windows_dir`; `supported_volumes = [unidad del sistema]`. Errores → `ContextError` → todo `Protected(ContextUnavailable)`: obligatoria ausente o no normalizable, cualquier ancla que sea raíz de volumen, `profile == users_root`. Una carpeta opcional que no normaliza se descarta (las anclas solo agregan protección, nunca la quitan; excepción: si falla `local_app_data`/`roaming_app_data`/`profile` → `user = None`).
- **Clases de ancla**: `Structural` (windows, program files, program data, users root, perfil, local/roaming app data, app dirs) y `Content` (temp, carpetas conocidas de contenido, raíces de excepción). Importa solo para `check_rule_root`.
- **Adaptador** `adapters/windows/known_folders.rs`: `resolve_protection_context(user: UserFolderSource, app_dirs) -> RawProtectionContext` con `SHGetKnownFolderPath(…, KF_FLAG_DEFAULT, token)` y `GetTempPath2W`. No usa variables de entorno para anclas (se heredan del usuario que lanzó el proceso y son triviales de falsear).
- **Costura C-05**: `UserFolderSource::ProcessToken` (C-03, `hToken = None`) vs `UserFolderSource::Token(handle)` / carpetas ya resueltas que C-05 inyecta para el usuario de la sesión interactiva. **Seguro por construcción incluso sin C-05**: en over-the-shoulder el token es del admin B; el perfil del usuario interactivo A cae en `OtherUserProfile` (protegido). El bloqueo de categorías por-usuario sigue siendo de C-05/C-08.
- El contexto se resuelve **por operación** (inicio de cada escaneo/limpieza); es barato y refleja redirecciones de carpetas conocidas.

### D-06 — Normalización léxica (pura)
`NormalizedPath { drive: char /* 'A'..='Z' */, components: Vec<Component { original: String, folded: String }> }`; `Display` = `X:\a\b` (forma Win32, sin `\\?\`).
1. Entrada `&str` (el servicio recibe `&Path` y usa `to_str()`: UTF-16 inválido → `NonUnicode`). Vacía o con `\0` → `Empty`/`InvalidCharacter`.
2. Prefijos: `\\?\X:\` → forma verbatim (se quita el prefijo; en verbatim `/` es carácter inválido y `.`/`..` → `VerbatimNotNormalized`); `\\?\UNC\`, `\\server\share` → `NetworkPath`; `\\.\`, `\??\`, `\\?\Volume{`, `\\?\GLOBALROOT` y cualquier otro `\\?\` → `DevicePath`; `\x` (relativa a raíz) y `X:x` (relativa a unidad) y relativas → `NotAbsolute`.
3. Componentes: separadores `\` y `/` (no verbatim), vacíos colapsados, `.` descartado, `..` hace pop; pop en la raíz → `EscapesRoot` (Win32 lo recortaría en silencio; se prefiere rechazar).
4. Por componente: `:` → `AlternateDataStream` (cubre `a.txt:s`, `a.txt::$DATA`); `<>"|?*` o controles `0x01–0x1F` → `InvalidCharacter`; termina en `.` o ` ` → `TrailingDotOrSpace` (Win32 los recorta según reglas sutiles —`C:\Windows.\x` abre `C:\Windows\x`— y en verbatim pueden ser nombres distintos: se elimina la ambigüedad rechazando); base `CON|PRN|AUX|NUL|COM0-9|LPT0-9` (y `COM¹²³`/`LPT¹²³`) con o sin extensión → `ReservedDeviceName`.
5. **Plegado**: por `char` del BMP (`≤ U+FFFF`), `to_uppercase()` solo si produce exactamente un `char` del BMP; si no, el carácter queda igual. Justificación: NTFS compara con su tabla `$UpCase`, mapeo **simple 1:1 por unidad UTF-16**, sin plegado completo (`ß`≠`SS`), sin normalización Unicode (`é` compuesta ≠ descompuesta) y sin mapear fuera del BMP. ASCII-lowercase sería incorrecto para `Ä/ä`. Diferencias residuales con la `$UpCase` de un volumen concreto solo afectan componentes no-ASCII (nombres de usuario); todas las raíces protegidas fijas son ASCII, y un fallo de coincidencia en un componente del perfil cae en `OtherUserProfile` (protege), así que el error residual va en dirección fail-closed.
6. **8.3**: un componente tiene "forma corta" si contiene `~` seguido de dígito y cabe en 8.3 (`^[^~.]{1,6}~[0-9]{1,6}(\.[^.]{0,3})?$`, aprox.). No se expande léxicamente: la expansión la da `canonicalize` (nombres largos). `same_shape(lexical, real)`: misma unidad, misma cantidad de componentes, y cada par igual plegado **o** el léxico con forma corta. Si un componente con forma corta no se pudo resolver, `canonicalize` falla → protegido.
7. Otras unidades / `subst`: unidad no soportada → `UnsupportedVolume`; si se soportara, `canonicalize` devolvería la unidad real distinta → `ResolvedPathMismatch`.

### D-07 — Nube (OneDrive) y extensión mínima del puerto
`FileMeta` gana dos campos opcionales; `cloud_placeholder` conserva su nombre y precisa su semántica:
```rust
pub struct FileIdentity { pub volume_serial: u64, pub file_id: u128 } // FILE_ID_INFO (128 bits por ReFS)
pub struct FileMeta {
    pub kind: EntryKind, pub size_bytes: u64, pub modified: Option<EpochMillis>,
    /// RECALL_ON_DATA_ACCESS | RECALL_ON_OPEN | OFFLINE, o tag de reparse de proveedor de nube
    /// (IO_REPARSE_TAG_CLOUD*), aunque esté hidratado.
    pub cloud_placeholder: bool,
    pub identity: Option<FileIdentity>, // Some en metadata(); read_dir puede dar None
    pub hard_links: Option<u32>,        // ídem
}
```
- Un reparse de nube se informa `kind = ReparsePoint` **y** `cloud_placeholder = true`; el dominio prioriza nube sobre reparse para elegir `SkippedCloud`.
- **Hidratados también se omiten**: borrar un archivo sincronizado se propaga a la nube y a otros dispositivos. Extiende RN-PROT-05 en dirección conservadora.
- Sin método nuevo para atributos: un solo `open` por consulta.

### D-08 — Anti-TOCTOU: qué da C-03 y qué hace C-09
Nueva operación del puerto (object-safe):
```rust
pub trait AncestorPin: Send {}            // liberación en Drop
fn pin_ancestors(&self, path: &Path) -> Result<Box<dyn AncestorPin>, PortError>;
```
Adaptador: abre de la raíz hacia abajo cada ancestro con `FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES` (un acceso que **sí** participa del control de compartición; solo `READ_ATTRIBUTES` no lo haría), `FILE_SHARE_READ | FILE_SHARE_WRITE` (**sin** `FILE_SHARE_DELETE`), `FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS`; verifica por handle que no sea reparse point. Mientras viven los handles nadie puede abrir esos directorios con `DELETE` ⇒ no se pueden renombrar ni borrar ni reemplazar por junctions; convertirlos en junction exige que estén vacíos (y contienen el objetivo). Mover el hijo no requiere abrir el padre con `DELETE`, así que el objetivo sigue movible.

API del servicio:
```rust
impl PathProtection<'_, F> {
    pub fn check(&self, raw: &Path) -> ProtectionDecision;
    pub fn session(&self) -> CheckSession<'_, F>;                        // memo para escaneos
    pub fn reverify(&self, scanned: &CheckedPath) -> ProtectionDecision;  // sin pin
    pub fn prepare_delete(&self, scanned: &CheckedPath) -> Result<DeleteTarget, ProtectedReason>;
    pub fn check_rule_root(&self, raw: &Path) -> Result<NormalizedPath, ProtectedReason>;
}
pub struct DeleteTarget { path: PathBuf, checked: CheckedPath, _pin: Box<dyn AncestorPin> }
```
`prepare_delete`: (1) `pin_ancestors(scanned.path)` — falla → `Unresolvable(PinFailed)`; (2) `check` completo sin memo; (3) `identity` y `kind` iguales a los escaneados — distinto → `IdentityChanged`; (4) devuelve `DeleteTarget` con la ruta real verificada.

**C-09 debe**: (a) llamar `prepare_delete` inmediatamente antes de cada `move_to_trash`; (b) mover `target.path()` (ruta real), nunca la ruta del frontend ni la del escaneo; (c) mantener vivo el `DeleteTarget` hasta que `move_to_trash` retorne y soltarlo después; (d) garantizar que su adaptador `Trash` no siga el último componente (si el objetivo se reemplazara por un enlace en la ventana residual, se mueve el enlace, no su destino) y verificarlo con test real; (e) mapear `Protected` → `skipped_protected`, `SkippedCloud` → `skipped_cloud`.
Ventana residual: solo el objetivo mismo, entre (2) y el movimiento; por (d) el peor caso es mover a la Papelera un enlace recién plantado (recuperable, sin tocar el destino).

### D-09 — Decisión de salida
```rust
pub enum ProtectionDecision { Allowed(CheckedPath), SkippedCloud { path: NormalizedPath }, Protected(ProtectedReason) }
pub struct CheckedPath { pub path: NormalizedPath, pub kind: EntryKind, pub size_bytes: u64,
                         pub identity: FileIdentity, pub rule: AllowRule /* Default | Exception(ExceptionId) */ }
pub enum ProtectedReason {
    Unparseable(PathRejection), ContextUnavailable,
    UnsupportedVolume, VolumeRoot, VolumeSystemEntry, Anchor(AnchorKind), OtherUserProfile,
    WindowsDir, ProgramFiles, ProgramData, BrowserProfile(Browser), AppDir,
    ReparsePointInPath, ReparsePoint, MultipleHardLinks, ResolvedPathMismatch, IdentityChanged,
    Unresolvable(ResolveFailure), // NotFound | PermissionDenied | InUse | Unsupported | Io | NotADirectory | MissingIdentity | PinFailed
}
pub enum PathRejection { Empty, NonUnicode, NotAbsolute, NetworkPath, DevicePath, AlternateDataStream,
    InvalidCharacter, ReservedDeviceName, TrailingDotOrSpace, EscapesRoot, VerbatimNotNormalized }
```
- `ProtectedReason::code() -> &'static str` en snake_case estable (`windows_dir`, `reparse_point_in_path`, `alternate_data_stream`…; para `Unparseable(r)` el código es el de `r`). Test que fija la tabla completa de códigos (son claves de i18n y de `error_detail`).
- `ProtectionDecision::skip_outcome() -> Option<CleanupOutcome>`.
- `check_rule_root(raw)`: normaliza + resuelve como `check` (pero sin exigir `hard_links`); rechaza si la decisión es `Protected` por cualquier motivo salvo `Anchor(Content)`, o si la raíz es ancestro-o-igual de una raíz protegida, raíz de excepción, del perfil actual o de un ancla `Structural`. `C:\`→`VolumeRoot`; `C:\Users`→`Anchor`; `C:\Users\ana\AppData\Local`→`Anchor`; `C:\Users\ana\Downloads`→ok.

### D-10 — Adaptador `WindowsFileSystem`
- Abre con `std::fs::OpenOptions` + `std::os::windows::fs::OpenOptionsExt` (`access_mode(FILE_READ_ATTRIBUTES)`, `share_mode(READ|WRITE|DELETE)`, `custom_flags(OPEN_REPARSE_POINT|BACKUP_SEMANTICS)`), así `std` resuelve rutas largas; luego consultas por handle con `windows`: `GetFileInformationByHandleEx` (`FileBasicInfo`, `FileStandardInfo` → tamaño y `NumberOfLinks`, `FileAttributeTagInfo` → atributos + reparse tag, `FileIdInfo` → identidad) y `GetFinalPathNameByHandleW`. Abrir solo con `FILE_READ_ATTRIBUTES` no choca con modos de compartición: un archivo en uso exclusivo sigue siendo consultable.
- `read_dir`: `FindFirstFileExW(FindExInfoBasic, FIND_FIRST_EX_LARGE_FETCH)`; reparse tag desde `dwReserved0`; `identity`/`hard_links` = `None`.
- Mapeo de errores: `FILE_NOT_FOUND|PATH_NOT_FOUND` → `NotFound`; `ACCESS_DENIED` → `PermissionDenied`; `SHARING_VIOLATION|LOCK_VIOLATION` → `InUse`; `NOT_SUPPORTED|INVALID_FUNCTION|INVALID_PARAMETER` → `Unsupported`; resto → `Io`. Detalle = ruta + código, sin `unwrap`.
- `unsafe` solo en `adapters/windows`, bloques mínimos con `// SAFETY:`; buffers con tamaño verificado (reintento si `GetFinalPathNameByHandleW` pide más).
- Features de `windows` a habilitar (sin crates nuevas): `Win32_Foundation`, `Win32_Security`, `Win32_Storage_FileSystem`, `Win32_System_Com`, `Win32_UI_Shell`; las constantes de tag de nube se definen localmente (`tag & !IO_REPARSE_TAG_CLOUD_MASK == IO_REPARSE_TAG_CLOUD`) o con `Win32_System_SystemServices`. `cargo build --locked` debe pasar sin tocar `Cargo.lock`.

### D-11 — Estrategia de tests (Strict TDD)
- **Pura, tabla** (`rules_tests.rs`, `path_tests.rs`): una fila por escenario del spec + todos los casos de CHANGES.md: `..`, mayúsculas, `\\?\`, `\\?\UNC\`, `\\.\`, ADS, `nul.txt`, punto/espacio final, comodines, Unicode/espacios en usuario, `C:\`, `C:\Users`, `C:\Windows`, cada excepción (dentro, raíz exacta, hermano), comodín de perfil (0, 1 y 2 componentes), otro usuario, `Public`, otra unidad, contexto con sistema en `D:`, contexto inválido.
- **Invariantes enumeradas** (sin `proptest`; generador determinista en el test): para cada raíz protegida × sufijos × variantes (mayúsculas aleatorias deterministas, `/` vs `\`, `.\`, `x\..\`, `\\?\`) ⇒ misma decisión; todo descendiente de raíz protegida no cubierto por excepción ⇒ protegido; ninguna excepción es raíz/ancestro de un ancla estructural; toda excepción es descendiente estricta de su raíz protegida; ningún `Allowed` tiene un componente con `:`.
- **Servicio con `FakeFileSystem`**: ancestro reparse, ancestro nube, objetivo reparse/nube/hard link, mismatch léxico/real, 8.3 vía mapeo canónico, cada `PortError` en cada llamada ⇒ protegido, `reverify` con identidad cambiada, `prepare_delete` toma el pin de la ruta real y lo libera al soltar, falla de pin ⇒ protegido.
- **Batería de contrato FS** (`domain/ports/fs_contract.rs`) contra `FakeFileSystem` y `WindowsFileSystem` en `tempfile::TempDir`.
- **Adaptador real** (`adapters/windows/fs_tests.rs`, sin `#[ignore]`: junctions y hard links no requieren privilegios; symlinks requieren privilegio o Modo Desarrollador → ese único caso detecta `ERROR_PRIVILEGE_NOT_HELD` y se marca `#[ignore]` documentado): junction hacia carpeta hermana del tempdir, symlink, hard link, identidad estable y distinta tras recrear, 8.3 (solo si `GetShortPathNameW` devuelve un nombre corto en ese volumen; si no, el test lo informa y no afirma), pin que bloquea renombrar el ancestro y permite mover el hijo, `canonicalize` sin seguir el último componente. Las junctions **nunca** apuntan fuera del tempdir.
- **Contexto real**: un test de solo lectura que resuelve las carpetas conocidas y verifica que normalizan y que `ProtectionContext::from_raw` es válido. Ningún test escribe fuera de `TempDir`.

## Risks / Trade-offs

- **Falsos positivos (se limpia menos)**: perfiles reubicados con junction, `%TEMP%` en otra unidad, nombres con punto/espacio final, archivos con varios hard links, OneDrive hidratado. Aceptado (seguridad > espacio; DD-06, SU-09). Se reportan con motivo.
- **Costo de I/O por candidato** (≈ profundidad + 2 aperturas) → `CheckSession` con memo de ancestros para escaneos; la re-verificación previa al borrado no usa memo.
- **Plegado vs `$UpCase` real**: diferencias solo en no-ASCII; fallan hacia protegido (D-06).
- **Ventana TOCTOU residual** del objetivo: mitigada por pin + identidad + (C-09) trash sin seguir el último componente.
- **Pin bloquea brevemente a terceros** que intenten abrir con `DELETE` un ancestro (milisegundos por ítem); si un ancestro ya está abierto con `DELETE` por otro proceso, el pin falla y el ítem se omite (fail-closed).
- **Lista de excepciones incompleta para C-08** (p. ej. `ShaderCache`, logs en `C:\Windows\Logs`): intencional; C-08 deberá proponer cada ampliación.
- **Convención de escenarios del spec**: el bloque "Convención de los escenarios" de `specs/path-protection/spec.md` debe pasar al `Purpose` del spec principal al archivar (si no, los escenarios pierden su contexto de referencia).
- **Contrato del puerto cambia** (`FileMeta` +2 campos, `canonicalize` precisa semántica, `pin_ancestors`): solo afecta a `fakes.rs` y sus tests; no hay otros consumidores todavía.

## Open Questions — RESOLVED (owner, 2026-11-13)

1. **RESOLVED** — Excepciones extra `Windows\Minidump`, `ProgramData\Microsoft\Windows\WER\ReportArchive` y `ReportQueue`: se incluyen, tal como se diseñó.
2. **RESOLVED** — PA-12: solo el volumen del sistema en v0.1 para limpieza, duplicados y reglas personalizadas (otros volúmenes protegidos). PA-12 marcada resuelta para v0.1 en `knowledge-base/10_preguntas_abiertas.md`.
3. **RESOLVED** — Archivos de OneDrive/nube hidratados: siempre omitidos como `skipped_cloud`. RN-PROT-05 actualizada en `knowledge-base/05_reglas_de_negocio.md`.
4. **RESOLVED** — `pin_ancestors` vive en C-03, tal como se diseñó.

## Desvíos respecto de CHANGES.md / KB

- `is_protected` se parte en reglas puras + servicio con puerto (KB 11 pide "puro" y "canonicalizar": incompatible en una sola función).
- C-03 implementa además el adaptador real `WindowsFileSystem` y la resolución de contexto (asignados a C-03 por el design de C-02, no listados en CHANGES.md).
- RN-PROT-05 se amplía: contenido de nube **hidratado** también se omite.
- Se agregan protecciones no listadas: hard links múltiples, ADS, UNC/dispositivo, nombres reservados, punto/espacio final, directorios de la app, entradas `$*` de la raíz, otros volúmenes (PA-12 conservador), anclas en sí.
- Excepciones nuevas no explícitas en KB 11: `Windows\Minidump` (KB lo nombra en `crash_dumps` sin ubicar) y `ProgramData\…\WER\Report{Archive,Queue}` (KB: "ProgramData: solo cachés whitelisted" sin listar). Requieren ratificación.
- `pin_ancestors` (anti-TOCTOU fuerte) entra en C-03 en vez de C-09 porque es una primitiva del puerto `FileSystem` cuya garantía es de protección.

## Apply notes

Desviaciones respecto del diseño, registradas durante `/opsx:apply`.

### Ratificadas por el dueño (2026-11-13), del lote 1

1. **Ancestros estrictos de un ancla o de una raíz de perfil de navegador** → `Protected(Anchor(AncestorOfProtectedRoot))`, evaluado después de la comprobación de otro usuario. Borrar un ancestro arrastraría lo protegido.
2. **Una excepción solo aplica si toda raíz protegida que cubre la ruta es la raíz padre de esa excepción.** Una ruta cubierta además por otra raíz protegida no se libera.
3. **Un veredicto léxico `OtherUserProfile` sobre una ruta con un componente de aspecto 8.3 difiere a la ruta real resuelta** (el alias corto puede ser del usuario actual).
4. **Firmas**: `prepare_delete` devuelve `Result<DeleteTarget, ProtectionDecision>` (para transportar `SkippedCloud`); `check_rule_root` devuelve `Result<NormalizedPath, ProtectedReason>`; una carpeta de nube se acepta como raíz de regla.
5. Menores: una `program_files`/`app_dirs` no normalizable es `ContextError`; `CheckedPath.path` es la ruta canónica; los códigos estables están fijados por test (todo `Anchor(_)` comparte `"anchor"`); `CONIN$`/`CONOUT$` son nombres reservados.

### Introducidas en el lote 2 (adaptador Windows e integración)

6. **`read_dir` usa `std::fs::read_dir`** (datos de la enumeración, sin seguir enlaces) en lugar de `FindFirstFileExW` directo: elimina el `unsafe` de `FindClose`. El tag de reparse (para reparse de nube) se lee por handle solo de las entradas con atributo `REPARSE_POINT`. `read_dir` de un reparse point devuelve vacío (no se expone su contenido); de un archivo, error.
7. **Carpeta temporal con `GetTempPathW`, no `GetTempPath2W`**: esta última no existe en Windows 10 (objetivo declarado) y, al enlazarse de forma estática, impediría arrancar el proceso. El resultado se expande a nombres largos con el propio puerto (si falla, se usa tal cual); solo agrega protección/excepción sobre el perfil ya resuelto por `SHGetKnownFolderPath`.
8. **`canonicalize` abre la ruta completa sin seguir el último componente** y usa `GetFinalPathNameByHandleW(VOLUME_NAME_DOS)`: resuelve en una sola operación `..`, nombres 8.3, mayúsculas del disco y reparse intermedios. Una ruta cuyo resultado no tiene letra de unidad (`\?\UNC\`, `\?\Volume{…}`) es `Unsupported`. Rutas no absolutas → `Unsupported` (no depender del directorio actual).
9. **`FILE_FLAG_BACKUP_SEMANTICS` es obligatorio para abrir directorios**; con él, un token con `SeBackupPrivilege` (la app corre elevada) ignora las ACL de denegación. El mapeo `ACCESS_DENIED → PermissionDenied` se prueba por la función de mapeo; el test con ACL real se omite (con log) si el token ignora la ACL.
10. **Test de symlink a archivo `#[ignore]`**: crear symlinks exige privilegio (`ERROR_PRIVILEGE_NOT_HELD`, 1314) y esta sesión no lo tiene; se corre a mano.
11. **Unidad `subst` (escenario "Unidad sustituida")**: `ProtectionContext::from_raw` solo soporta la unidad del sistema, así que una unidad `subst` queda `UnsupportedVolume` (más estricto que el spec); la rama de discrepancia de unidad real se prueba con una ruta de `C:` cuya ruta real cae en otra unidad (`ResolvedPathMismatch`).
12. `UserFolderSource` solo tiene `ProcessToken` (la variante de C-05 se añade en C-05).

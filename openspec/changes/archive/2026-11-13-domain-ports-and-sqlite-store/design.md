# Design

## Context

Motivación en `proposal.md`; requisitos en `specs/{domain-ports,local-store,runtime-config}/spec.md`.

Estado actual (C-01 archivado, ver `openspec/changes/archive/2026-11-13-foundation-setup/design.md`):

- `src-tauri/src/{domain,adapters}/mod.rs` solo tienen doc. `lib.rs` expone `app_builder(builder, RuntimeConfig)` (usado por el smoke IPC con `tauri::test::mock_builder`) y `run()`. `config.rs` resuelve `RuntimeConfig { data_dir_override: Option<PathBuf>, log_level }` de forma pura. `error.rs` define `AppError::Internal` con `Serialize` manual `{ kind, message }`.
- `Cargo.toml` ya fija `rusqlite =0.42.0` (`bundled`, SQLite **3.54.1**: soporta tablas `STRICT`, `json_valid`, `RAISE`), `thiserror =2.0.21`, `serde_json`, `tempfile =3.27.0` (dev). `[lints.cargo] unused_dependencies = "allow"` silencia las dependencias aún sin uso. Lints: `unwrap_used`/`expect_used`/`undocumented_unsafe_blocks` = deny (permitidos en tests vía `clippy.toml`).
- Tests de C-01 que C-02 MUST mantener verdes: `tests/architecture.rs` (prohíbe `std::fs`, `std::process`, `std::net`, `windows`, `windows_sys`, `rusqlite`, `tauri` en `src/domain/**`), `tests/no_network_crates.rs` (denylist de red; C-02 no agrega crates), `tests/security_baseline.rs` (capabilities = exactamente `APP_COMMANDS` = `["get_app_info"]`; C-02 **no** agrega comandos).
- Hasta C-05 la app corre `asInvoker` (C-01 D-07): el token del proceso **es** el del usuario interactivo, así que resolver `%LOCALAPPDATA%` con el token del proceso es correcto hoy.
- Skills aplicadas: `rust-best-practices` (thiserror en dominio, sin `unwrap` fuera de tests, static dispatch por defecto, `fn_should_x_when_y`), `rust-testing` (`#[cfg(test)]` al pie, `tempfile`, `assert_eq!`, `matches!` sobre `Result`, tests independientes), `sqlite-database-expert` (consultas parametrizadas siempre, `foreign_keys=ON` por conexión, WAL + `synchronous=NORMAL`, lotes en transacción, versionado de esquema, respaldo antes de lo destructivo, conexión reutilizada en estado gestionado, no filtrar internals en errores, test rojo primero), `tauri-v2` (`app.manage` con tipo exacto, `app.path()` en vez de rutas hardcodeadas, I/O fuera del hilo principal), `tauri-security` (deny-by-default: C-02 no toca capabilities).

## Goals / Non-Goals

**Goals:**
- Puertos mínimos, síncronos y object-safe que permitan Strict TDD del dominio en C-03..C-11 sin tocar el SO.
- Persistencia SQLite confiable: migraciones atómicas, rechazo de bases futuras, corrupción detectada sin destruir nada, append-only garantizado **por la base**, retención automática de escaneos.
- Una sola batería de contrato que pruebe que el fake de persistencia y SQLite se comportan igual.
- Costura explícita para que C-05 cambie de qué perfil sale `%LOCALAPPDATA%` sin tocar el adaptador SQLite.

**Non-Goals:**
- Adaptadores reales de `FileSystem` (C-03), `Sensors` (C-06), `Trash` (C-09): C-02 define contratos y fakes.
- Puerto `Registry` (diferido a C-16, ver D-02).
- Comandos IPC (`get_settings`/`set_setting`, estado y recuperación de la base) y UI de error: C-04.
- Modelo tipado de settings (`UiMode`, validación de claves): C-04. C-02 guarda pares clave → JSON.
- Tablas de C-07+ (`folder_snapshot`, `junk_finding`, duplicados, métricas…), purga manual del log (PA-13), cifrado.
- Backend de logging (sigue sin decidirse).

## Decisions

### D-01 — Estructura de módulos

```
src-tauri/src/
  domain/
    mod.rs            pub mod model; pub mod ports;
    model.rs          EpochMillis, ScanId, ScanKind, ScanTrigger, ScanStatus, NewScan, Scan,
                      CleanupOutcome, NewCleanupLogEntry, CleanupLogEntry, SCAN_RETENTION
    ports/
      mod.rs          re-exports; #[cfg(test)] pub(crate) mod fakes; #[cfg(test)] pub(crate) mod store_contract;
      error.rs        PortError
      fs.rs           FileSystem, FileMeta, EntryKind, DirEntry
      trash.rs        Trash
      sensors.rs      Sensors, SensorSample
      clock.rs        Clock
      store.rs        StoreError, SettingsStore, ScanStore, CleanupLogStore, Store
      fakes.rs        FakeFileSystem, FakeTrash, FakeSensors, FixedClock, InMemoryStore   (solo tests)
      store_contract.rs  batería genérica `fn check_*<S: Store>(make: impl Fn() -> S)`  (solo tests)
  adapters/
    mod.rs            pub mod clock; pub mod sqlite;
    clock.rs          SystemClock
    sqlite/
      mod.rs          SqliteStore (+ impl de los 3 traits), StoreState, open_store_state
      connection.rs   open_connection(path) + pragmas + quick_check
      migrations.rs   Migration, MIGRATIONS, migrate(conn, &[Migration])
      migrations/001_init.sql
      errors.rs       map de rusqlite::Error -> StoreError (sin SQL en el detalle)
      recovery.rs     backup_and_recreate(db_path, &impl Clock) -> Result<PathBuf, StoreError>
  config.rs           + resolve_data_dir(..), DB_FILE_NAME
  error.rs            + variantes de store y From<StoreError> for AppError
  lib.rs              run(): .setup(...) abre la base y hace manage(StoreState)
```

KB 08 dice `domain/ports.rs`; se usa un directorio `ports/` con un archivo por puerto por tamaño. Es el mismo módulo lógico (`crate::domain::ports`).

### D-02 — Puertos: superficie mínima y quién la extiende

Regla: cada método tiene un consumidor concreto en C-03..C-11 (o en C-02). Lo que solo usa v0.2+ se difiere.

```rust
// domain/ports/error.rs
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PortError {
    #[error("not found: {0}")]        NotFound(String),
    #[error("permission denied: {0}")] PermissionDenied(String),
    #[error("in use: {0}")]            InUse(String),
    #[error("unsupported: {0}")]       Unsupported(String),
    #[error("i/o error: {0}")]         Io(String),
}

// domain/ports/fs.rs
pub enum EntryKind { File, Directory, ReparsePoint }   // ReparsePoint = junction/symlink/otro; no se sigue
pub struct FileMeta {
    pub kind: EntryKind,
    pub size_bytes: u64,
    pub modified: Option<EpochMillis>,
    pub cloud_placeholder: bool,   // atributos RECALL_ON_DATA_ACCESS / RECALL_ON_OPEN / OFFLINE
}
pub struct DirEntry { pub path: PathBuf, pub meta: FileMeta }
pub trait FileSystem {
    fn canonicalize(&self, path: &Path) -> Result<PathBuf, PortError>;
    fn metadata(&self, path: &Path) -> Result<FileMeta, PortError>;          // no sigue reparse points
    fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>, PortError>;     // no sigue reparse points
}

// domain/ports/trash.rs
pub trait Trash {
    fn move_to_trash(&self, path: &Path) -> Result<(), PortError>;
}

// domain/ports/sensors.rs
pub struct SensorSample {
    pub cpu_pct: f32,                 // 0..=100
    pub ram_used_bytes: u64,
    pub ram_total_bytes: u64,
    pub disk_busy_pct: Option<f32>,   // None = no disponible (nunca 0 falso)
}
pub trait Sensors {
    fn sample(&mut self) -> Result<SensorSample, PortError>;   // &mut: sysinfo necesita refrescar estado
}

// domain/ports/clock.rs
pub trait Clock { fn now(&self) -> EpochMillis; }
```

| Puerto | Consumidor que lo implementa de verdad | Extensiones previstas |
|---|---|---|
| `FileSystem` | C-03 (canonicalize, metadata), C-07/C-08 (read_dir) | C-11 agrega lectura para hash parcial/completo (o un puerto `Hasher`, lo decide C-11). C-03 puede agregar campos a `FileMeta` si el algoritmo de 7 pasos lo pide. |
| `Trash` | C-09 | C-08: tamaño de la Papelera (`recycle_bin_usage`); C-10: vaciado (HIGH, con revisión). |
| `Sensors` | C-06 | C-14: temperaturas/SMART (`Option`s nuevos). |
| `Clock` | C-02 (`SystemClock`) | — |
| `Registry` | **diferido a C-16** | C-16 lo define (snapshot/undo), C-17 lo extiende (startup). |

**Desvío de CHANGES.md — `Registry` no se define en C-02.** Ningún change de v0.1 (C-03..C-13) lo usa; su primer consumidor es C-16 (v0.2, "Tests con fake de `Registry`"). Definirlo ahora sería adivinar la forma de valores/llaves de registro sin consumidor y luego reescribirlo. `ports/mod.rs` deja un comentario que lo asigna a C-16.

**Desvío menor — `Clock` no figura en CHANGES.md.** Se agrega porque el dominio no debe leer la hora del sistema (impuro) y C-02 la necesita (sufijo del respaldo); todos los registros posteriores con timestamp lo usan.

**`Trash` sin borrado permanente**: es un requisito de spec, no solo de alcance; la única acción permanente del producto (vaciar Papelera, RN-LIM-06) entra con C-10 bajo gobernanza HIGH.

### D-03 — Síncrono, object-safe, static dispatch

- **Síncrono**: `rusqlite`, `std::fs`, `trash` y `sysinfo` son bloqueantes; envolverlos en `async` no gana nada y `async fn` en traits no es object-safe sin `Box<dyn Future>`. El dominio no depende de un runtime. Los comandos IPC (C-04+) llaman a los puertos dentro de `tauri::async_runtime::spawn_blocking` o desde hilos propios (monitor C-06, escaneos C-07), nunca en el hilo principal.
- **Object-safe**: sin métodos genéricos ni `Self` en retorno; así `Box<dyn Trait>`/`&dyn Trait` queda disponible (p. ej. estado Tauri heterogéneo), aunque el dominio usa **generics** (`fn f(fs: &impl FileSystem)`) por defecto. Un test de compilación (`let _: &dyn FileSystem = &fake;` para cada puerto) fija la propiedad.
- **Sin supertraits `Send + Sync`**: los ponen los consumidores que los necesiten (`S: Store + Send + Sync`). `SqliteStore` es `Send + Sync` (test de compilación `fn assert_send_sync<T: Send + Sync>()`).
- **`&self` en persistencia, FS y Trash** (mutabilidad interior en el adaptador) para poder compartirlos en `State`; **`&mut self` en `Sensors`** porque su dueño es un único hilo muestreador.

### D-04 — Errores: `PortError`, `StoreError` y mapeo a `AppError`

`StoreError` vive en el dominio (no puede mencionar `rusqlite`):

```rust
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StoreError {
    #[error("database is corrupt: {0}")]                         Corrupt(String),
    #[error("database schema {found} is newer than supported {supported}")]
                                                                  SchemaTooNew { found: u32, supported: u32 },
    #[error("migration {version} failed: {detail}")]              MigrationFailed { version: u32, detail: String },
    #[error("not found: {0}")]                                    NotFound(String),
    #[error("invalid state: {0}")]                                InvalidState(String),   // p. ej. finalizar 2 veces
    #[error("constraint violated: {0}")]                          Constraint(String),     // CHECK/trigger/UNIQUE
    #[error("database unavailable: {0}")]                         Unavailable(String),    // I/O, lock, mutex envenenado, ruta
}
```

- `adapters/sqlite/errors.rs`: `fn map_sqlite_error(e: rusqlite::Error) -> StoreError` (función libre, no `From`, para que cada sitio decida el contexto). `ErrorCode::NotADatabase | DatabaseCorrupt` → `Corrupt`; `ConstraintViolation` → `Constraint`; `DatabaseBusy | DatabaseLocked | CannotOpen | SystemIoFailure | DiskFull | ReadOnly` → `Unavailable`; resto → `Unavailable`. **El detalle es el nombre del código de error y, para `Constraint`, el mensaje del `RAISE`/CHECK (que no contiene SQL de la app)**; nunca el texto de la sentencia.
- `error.rs`: nuevas variantes de `AppError` con `kind` estable: `StoreCorrupt` → `store_corrupt`, `StoreSchemaTooNew` → `store_schema_too_new`, `StoreMigrationFailed` → `store_migration_failed`, `StoreUnavailable` → `store_unavailable`. `impl From<StoreError> for AppError`: `NotFound`/`InvalidState`/`Constraint` → `Internal` (son bugs de programación o validaciones que C-04 tipará cuando exponga comandos). Tests de tabla: cada `StoreError` → `kind` esperado y `message` sin `SELECT|INSERT|UPDATE|DELETE`.
- `PortError` → `AppError` **no** se define en C-02 (lo hará el primer comando que exponga un puerto; los outcomes de C-09 mapean `PortError` a `CleanupOutcome`, no a `AppError`).

### D-05 — Modelo persistido (domain/model.rs)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EpochMillis(pub i64);                      // UTC ms; > 0 al persistir
pub struct ScanId(pub i64);
pub enum ScanKind { Full, Junk, Duplicates, Folders }         // "full" | "junk" | "duplicates" | "folders"
pub enum ScanTrigger { Manual, Scheduled }                    // "manual" | "scheduled"
pub enum ScanStatus { Running, Done, Cancelled, Error }       // "running" | ... ; is_terminal()
pub struct NewScan { pub kind: ScanKind, pub trigger: ScanTrigger, pub started_at: EpochMillis }
pub struct Scan { pub id: ScanId, pub kind, pub trigger, pub status, pub started_at, pub finished_at: Option<EpochMillis> }
pub enum CleanupOutcome { MovedToTrash, SkippedLocked, SkippedProtected, SkippedCloud, Error, RecycleBinEmptied }
pub struct NewCleanupLogEntry {
    pub ts: EpochMillis, pub category: String, pub path: String, pub size_bytes: u64,
    pub outcome: CleanupOutcome, pub scan_id: Option<ScanId>, pub error_detail: Option<String>,
}
pub struct CleanupLogEntry { pub id: i64, /* + campos de NewCleanupLogEntry */ }
pub const SCAN_RETENTION: usize = 50;
/// Seed de settings (clave, JSON). La migración 001 lo replica en SQL; la batería de contrato
/// verifica que ambas fuentes coincidan.
pub const DEFAULT_SETTINGS: &[(&str, &str)] = &[("ui_mode", "\"simple\""), ("autostart_tray", "false"), ("language", "\"es\"")];
pub fn scans_to_prune(scans: &[Scan], keep: usize) -> Vec<ScanId>;   // pura; usada por InMemoryStore
```

- Cada enum tiene `as_str()` y `parse(&str) -> Option<Self>` (round-trip testeado en tabla); las cadenas coinciden con los `CHECK` de la migración. Sin `serde` sobre el modelo por ahora (C-04 decide la forma IPC).
- `category` es `String` (no enum) porque el enum de categorías lo crea C-08 y C-11 agregará duplicados; `path` es `String` (el llamador convierte `PathBuf` con `to_string_lossy`; C-07 decide si necesita algo más estricto).
- `size_bytes: u64` en el dominio; el adaptador convierte a `i64` con `i64::try_from` → `StoreError::Constraint` si excede.
- `scan.schedule_id` (KB 04) **no** se crea: la tabla `schedule` no existe hasta C-23, que hará `ALTER TABLE scan ADD COLUMN schedule_id INTEGER REFERENCES schedule(id)` (permitido en SQLite con default `NULL`).
- La columna KB `trigger` se llama **`triggered_by`**: `TRIGGER` es palabra reservada de SQLite y obligaría a citarla en cada consulta.

### D-06 — Migraciones por `PRAGMA user_version`

```rust
pub struct Migration { pub version: u32, pub sql: &'static str }
pub const MIGRATIONS: &[Migration] = &[Migration { version: 1, sql: include_str!("migrations/001_init.sql") }];
pub fn latest_version(migrations: &[Migration]) -> u32;
pub fn migrate(conn: &mut Connection, migrations: &[Migration]) -> Result<u32, StoreError>;
```

Algoritmo de `migrate`:
1. `current = PRAGMA user_version`. Si `current > latest` → `SchemaTooNew { found, supported }` **sin escribir**.
2. Para cada migración con `version > current`, en orden: `transaction_with_behavior(Immediate)` → `execute_batch(sql)` → `pragma_update(None, "user_version", version)` → `commit`. `user_version` vive en el header de la base y es transaccional: si algo falla, el `Drop` de la transacción hace rollback y la versión queda en la anterior. Error → `MigrationFailed { version, detail }`.
3. Devuelve la versión final.

- `migrate` recibe la lista por parámetro para poder testear con migraciones de prueba (una que falla en la 2ª sentencia, una "v2" válida para el caso de pendientes parciales).
- Test puro de la lista real: versiones estrictamente crecientes, contiguas desde 1, SQL no vacío.
- Regla para C-07/C-08/C-11/…: **nunca editar una migración publicada**; cada change agrega `00N_*.sql`. Las tablas hijas de `scan` declaran `REFERENCES scan(id) ON DELETE CASCADE` para que la retención (D-08) arrastre sus hijos.

### D-07 — Migración 001 (contenido objetivo)

```sql
CREATE TABLE settings (
  key        TEXT PRIMARY KEY NOT NULL CHECK (length(key) > 0),
  value_json TEXT NOT NULL CHECK (json_valid(value_json))
) STRICT;

CREATE TABLE scan (
  id           INTEGER PRIMARY KEY,
  kind         TEXT    NOT NULL CHECK (kind IN ('full','junk','duplicates','folders')),
  triggered_by TEXT    NOT NULL CHECK (triggered_by IN ('manual','scheduled')),
  status       TEXT    NOT NULL CHECK (status IN ('running','done','cancelled','error')),
  started_at   INTEGER NOT NULL CHECK (started_at > 0),
  finished_at  INTEGER CHECK (finished_at IS NULL OR finished_at >= started_at),
  CHECK ((status = 'running') = (finished_at IS NULL))
) STRICT;
CREATE INDEX idx_scan_started_at ON scan(started_at);

CREATE TABLE cleanup_log (
  id           INTEGER PRIMARY KEY,
  ts           INTEGER NOT NULL CHECK (ts > 0),
  category     TEXT    NOT NULL CHECK (length(category) > 0),
  path         TEXT    NOT NULL CHECK (length(path) > 0),
  size_bytes   INTEGER NOT NULL CHECK (size_bytes >= 0),
  outcome      TEXT    NOT NULL CHECK (outcome IN ('moved_to_trash','skipped_locked','skipped_protected',
                                                   'skipped_cloud','error','recycle_bin_emptied')),
  scan_id      INTEGER,          -- referencia lógica a scan(id); SIN FK a propósito (ver abajo)
  error_detail TEXT
) STRICT;
CREATE INDEX idx_cleanup_log_ts ON cleanup_log(ts);

CREATE TRIGGER cleanup_log_no_update BEFORE UPDATE ON cleanup_log
BEGIN SELECT RAISE(ABORT, 'cleanup_log is append-only'); END;
CREATE TRIGGER cleanup_log_no_delete BEFORE DELETE ON cleanup_log
BEGIN SELECT RAISE(ABORT, 'cleanup_log is append-only'); END;

INSERT OR IGNORE INTO settings(key, value_json) VALUES
  ('ui_mode', '"simple"'), ('autostart_tray', 'false'), ('language', '"es"');
```

- **`cleanup_log.scan_id` sin `FOREIGN KEY`** (desvío de KB 04, que lo marca FK): con FK, borrar un escaneo por retención exigiría `ON DELETE SET NULL` (un UPDATE que el trigger aborta → la retención fallaría) o `NO ACTION` (la retención fallaría siempre que haya log). La auditoría debe sobrevivir a la retención intacta, así que es una referencia lógica histórica.
- `STRICT` hace que SQLite rechace tipos incorrectos (p. ej. texto en `size_bytes`).
- El seed vive en la migración 1 (`INSERT OR IGNORE`): se aplica una sola vez por versión y nunca pisa cambios del usuario. Futuras claves por defecto → nueva migración con `INSERT OR IGNORE`.
- Purga manual del log (KB 04, PA-13) queda fuera: cuando se implemente, deberá ser una operación explícita y auditada que no pase por el trigger (decisión de ese change).

### D-08 — `SqliteStore`, conexión y concurrencia

```rust
pub struct SqliteStore { conn: Mutex<Connection>, path: Option<PathBuf> }
impl SqliteStore {
    pub fn open(path: &Path) -> Result<Self, StoreError>;          // crea si no existe, pragmas, quick_check, migrate
    #[cfg(test)] pub(crate) fn open_in_memory() -> Result<Self, StoreError>;
}
impl SettingsStore for SqliteStore { … }  impl ScanStore … impl CleanupLogStore …
```

- **Pragmas por conexión** (`connection.rs`): `foreign_keys = ON`, `busy_timeout(5 s)`, `journal_mode = WAL` (se lee con `query_row("PRAGMA journal_mode=WAL")` porque devuelve fila; en `:memory:` devuelve `memory`, aceptado), `synchronous = NORMAL`.
- **Detección de corrupción al abrir**: tras los pragmas se ejecuta `PRAGMA quick_check`; resultado distinto de `ok` o errores `NotADatabase`/`DatabaseCorrupt` → `StoreError::Corrupt`. `Connection::open` sobre basura no falla hasta la primera lectura, por eso el chequeo es explícito. Abrir **no escribe** antes de pasar este chequeo y el de versión (el `PRAGMA journal_mode=WAL` se aplica **después** de leer `user_version` y del `quick_check`, para no tocar una base futura o corrupta).
- **`Mutex<Connection>` única** (no pool). Justificación: un solo proceso de escritorio, volumen bajo (settings, un escaneo cada tanto, log por lote); `rusqlite::Connection` es `Send` pero no `Sync`; un pool (`r2d2`) agrega una dependencia y no aporta con una sola escritora en SQLite. Las operaciones son cortas y en transacción. Mutex envenenado → `StoreError::Unavailable("lock poisoned")`, nunca `unwrap`. **Costura**: si C-07 (snapshots grandes) muestra contención con lecturas de UI, se agrega una segunda conexión de solo lectura (WAL lo permite) sin cambiar los traits.
- Escrituras multi-fila (`append_cleanup_log`, `create_scan` + retención) en **una** transacción `Immediate`. Todas las consultas parametrizadas (`?1…`), nunca `format!` con datos.

Traits de persistencia (en `domain/ports/store.rs`):

```rust
pub trait SettingsStore {
    fn get_setting(&self, key: &str) -> Result<Option<serde_json::Value>, StoreError>;
    fn list_settings(&self) -> Result<Vec<(String, serde_json::Value)>, StoreError>;   // orden por clave
    fn set_setting(&self, key: &str, value: &serde_json::Value) -> Result<(), StoreError>; // upsert
}
pub trait ScanStore {
    fn create_scan(&self, scan: &NewScan) -> Result<ScanId, StoreError>;   // + retención en la misma tx
    fn finish_scan(&self, id: ScanId, status: ScanStatus, finished_at: EpochMillis) -> Result<(), StoreError>;
    fn get_scan(&self, id: ScanId) -> Result<Option<Scan>, StoreError>;
    fn list_scans(&self) -> Result<Vec<Scan>, StoreError>;                 // más reciente primero (≤ 50 + running)
}
pub trait CleanupLogStore {
    fn append_cleanup_log(&self, entries: &[NewCleanupLogEntry]) -> Result<(), StoreError>; // atómico
    fn recent_cleanup_log(&self, limit: u32) -> Result<Vec<CleanupLogEntry>, StoreError>;   // ts DESC, id DESC
}
pub trait Store: SettingsStore + ScanStore + CleanupLogStore {}
impl<T: SettingsStore + ScanStore + CleanupLogStore> Store for T {}
```

- `serde_json::Value` en el dominio está permitido (no es I/O) y evita un tipo propio de JSON. Clave vacía → `Constraint`.
- `finish_scan`: `status` no terminal → `InvalidState` (validado en dominio antes de tocar la base); `UPDATE … WHERE id=?1 AND status='running'`; 0 filas → `NotFound` si no existe, `InvalidState` si ya terminó.
- `get_scan`/`list_scans`: un valor de enum desconocido leído de la base → `Corrupt` (no `panic`).
- No hay métodos de UPDATE/DELETE del log (spec: la persistencia MUST NOT ofrecerlos).

### D-09 — Retención de escaneos (implementada en el store)

Se implementa **ahora y dentro de `create_scan`**, en la misma transacción que el `INSERT`:

```sql
DELETE FROM scan
WHERE status <> 'running'
  AND id NOT IN (SELECT id FROM scan ORDER BY started_at DESC, id DESC LIMIT ?1);   -- ?1 = SCAN_RETENTION
```

- Por qué aquí y no diferida: es una regla de C-02 en CHANGES.md, es barata (índice por `started_at`) y hacerla atómica con la creación impide olvidarla; con `ON DELETE CASCADE` en las tablas hijas (D-06) arrastra `folder_snapshot`/`junk_finding`/duplicados de C-07+ sin código extra.
- `running` nunca se borra (un escaneo en curso seguirá insertando hijos). Escaneos huérfanos `running` tras un cierre abrupto: los reconcilia C-07 (primer change que crea escaneos); se deja anotado.
- La retención de ~90 días de `folder_snapshot` periódicos (tendencias) es de C-15/C-25.
- `InMemoryStore` implementa la misma regla con una función pura del dominio `fn scans_to_prune(scans: &[Scan], keep: usize) -> Vec<ScanId>` (testeada en tabla) y ambos pasan la batería de contrato.

### D-10 — Ubicación de la base y costura para C-05

En `config.rs` (puro):

```rust
pub const APP_DATA_SUBDIR: &str = "pc-optimizer";
pub const DB_FILE_NAME: &str = "data.db";
pub fn resolve_data_dir(override_dir: Option<&Path>, local_app_data: Result<PathBuf, String>)
    -> Result<PathBuf, String>;      // override (ya filtrado a dev + absoluto) gana; si no, local_app_data/pc-optimizer
pub fn db_path(data_dir: &Path) -> PathBuf;   // data_dir/data.db
```

- **Fuente del perfil hoy**: `app.path().local_data_dir()` de Tauri (= `FOLDERID_LocalAppData` del token del proceso). Se usa `local_data_dir()` y **no** `app_data_dir()` porque esta última anexa el identificador del bundle (`com.pcoptimizer.desktop`, provisorio hasta C-13): cambiarlo movería la base. Resultado: `%LOCALAPPDATA%\pc-optimizer\data.db` (decisión del dueño, 2026-11-13: historial y métricas son propios de la máquina y pueden crecer; no deben itinerar con los perfiles de dominio).
- **Costura C-05**: la única entrada dependiente del perfil es el argumento `local_app_data`. C-05 reemplaza su fuente por el `%LOCALAPPDATA%` del usuario de la sesión interactiva (p. ej. `SHGetKnownFolderPath` con el token de la sesión) y, si no puede determinarlo, devuelve `Err` → base no disponible (fail-closed: nunca escribir en el perfil equivocado). `SqliteStore` no sabe de perfiles.
- `PCOPT_DATA_DIR` (ya filtrado por `resolve_runtime_config`: solo debug, absoluto, no vacío) se usa tal cual, sin subcarpeta.
- `std::fs::create_dir_all(data_dir)` lo hace el adaptador (`open_store_state`) antes de abrir; no es un archivo de usuario.
- Decisión del dueño (2026-11-13): `LocalAppData` y no Roaming `%APPDATA%`; ver KB 09 DD-15.

### D-11 — Arranque, `StoreState` y respaldo/recreación

```rust
pub enum StoreState {
    Ready(Arc<SqliteStore>),
    Unavailable { error: StoreError, db_path: Option<PathBuf> },
}
pub fn open_store_state(data_dir: Result<PathBuf, String>) -> StoreState;   // nunca falla
pub fn backup_and_recreate(db_path: &Path, clock: &impl Clock) -> Result<(PathBuf, SqliteStore), StoreError>;
```

- `run()` agrega `.setup(|app| { … app.manage(open_store_state(dir)); Ok(()) })` **solo en `run()`**, no en `app_builder`, para que el smoke IPC con `mock_builder` siga igual. `setup` corre antes de mostrar la ventana; abrir una base pequeña es rápido. Si C-04 detecta demora, mueve la apertura a un hilo y gestiona el estado como `Loading`.
- **Nunca se recrea automáticamente** (KB flujo 1: "se **ofrece** respaldo y recreación"). C-02 deja `StoreState::Unavailable` con el error tipado; **C-04** expone por IPC el estado y la acción de recuperación con mensaje del catálogo (no se agregan comandos ni capabilities en C-02).
- `backup_and_recreate`: (1) intenta `open` solo para clasificar: si es `SchemaTooNew` → rechaza sin tocar nada; (2) renombra `data.db`, `data.db-wal` y `data.db-shm` (los que existan) a `data.db.corrupt-<ms>.bak`, `…-wal.bak`, `…-shm.bak`; si el nombre existe, sufijo `-1`, `-2`…; (3) `SqliteStore::open` crea una base nueva migrada y con seed. Rename (no copia) es atómico en el mismo volumen y no borra nada. Errores de rename (archivo en uso) → `Unavailable`.

### D-12 — Fakes y batería de contrato

- Ubicación: `domain/ports/fakes.rs` bajo `#[cfg(test)]` (`pub(crate)`). Todos los consumidores previstos de fakes (tests de dominio de C-03..C-11 y tests de comandos en `lib.rs`) son **tests unitarios dentro del crate**; los tests de integración en `tests/` prueban adaptadores reales sobre `tempfile`. Así los fakes no entran al binario sin features extra. **Costura**: si un test de integración llegara a necesitarlos, se mueven a un módulo `testing` detrás de una feature `test-support` habilitada solo en `[dev-dependencies]` (decisión de ese change).
- Los fakes usan `std::sync::Mutex`/`RefCell` y `BTreeMap`: no importan nada prohibido, el test de arquitectura sigue verde.
- `FakeFileSystem`: builder `with_dir`, `with_file(path, size, modified)`, `with_reparse_point(path)`, `with_cloud_placeholder(path, size)`, `with_canonical(path, canonical)`, `fail_on(path, PortError)`. `read_dir` devuelve solo hijos directos; un `ReparsePoint` nunca expone hijos. `canonicalize` sin mapeo explícito → `NotFound`.
- `FakeTrash`: registra rutas movidas (`moved()`), `fail_on(path, PortError)`; opcionalmente comparte un `FakeFileSystem` para quitar la ruta al moverla.
- `FakeSensors`: cola de muestras/errores guionadas; sin más muestras → `Unsupported`.
- `FixedClock(EpochMillis)` y `SystemClock` (adaptador, `SystemTime::now()` → ms; antes de 1970 o desborde → valor saturado, sin `unwrap`).
- `InMemoryStore`: mismo comportamiento observable que SQLite (seed inicial, upsert, transición de escaneo, retención, lote atómico con validación previa, orden de lecturas). Las restricciones de la base (CHECK, JSON válido, append-only por SQL directo) se prueban solo contra SQLite.
- `store_contract.rs`: funciones `pub(crate) fn contract_<caso><S: Store>(make: impl Fn() -> S)`; se invocan desde `#[cfg(test)]` de `fakes.rs` (con `InMemoryStore::new`) y de `adapters/sqlite/mod.rs` (con `SqliteStore` sobre `tempfile::TempDir`). Un caso nuevo agregado al contrato corre automáticamente contra ambos.

### D-13 — Tests y aislamiento del disco

- Unidad: `#[cfg(test)] mod tests` al pie de cada archivo; nombres `fn_should_x_when_y`; tablas para enums y mapeos.
- Adaptador SQLite: siempre `tempfile::TempDir` (o `:memory:` donde WAL no importe). Para probar triggers y CHECKs se abre el **mismo archivo temporal** con `rusqlite::Connection` directo (el test está en `adapters/`, donde `rusqlite` está permitido).
- Ningún test usa `PCOPT_DATA_DIR`, `app.path()` ni `%LOCALAPPDATA%`; `resolve_data_dir` se prueba con rutas inventadas. `open_store_state` se prueba con un `TempDir` y con `Err(..)`.
- Concurrencia: 4 hilos × 25 entradas sobre `Arc<SqliteStore>` en `TempDir`.

### D-14 — `Cargo.toml`

Tras C-02 se usan `rusqlite`, `thiserror`, `serde_json`, `tempfile`. Siguen sin uso `sysinfo` (C-06), `walkdir` (C-07, si lo usa el adaptador FS), `trash` (C-09), `blake3` (C-11), `windows` (C-03/C-05). El lint de cargo no admite excepciones por dependencia, así que `[lints.cargo] unused_dependencies = "allow"` **se mantiene** con el comentario actualizado listando las pendientes y su change; lo quita el último consumidor. Sin dependencias nuevas (`Cargo.lock` no cambia).

## Risks / Trade-offs

- **[`Mutex<Connection>` como cuello de botella]** Escaneos grandes (C-07) podrían bloquear lecturas de UI. → Operaciones cortas y por lote; costura de conexión de solo lectura (D-08).
- **[`setup` bloquea el arranque]** `quick_check` es O(tamaño de base). → Base acotada por retención; medir en C-04 y mover a hilo si hace falta.
- **[Triggers append-only impiden purga futura]** PA-13 prevé purga manual. → Se resolverá en ese change con una operación explícita; hoy la garantía de auditoría pesa más.
- **[`scan_id` sin FK]** El log puede apuntar a escaneos ya borrados. → Aceptado y documentado: es referencia histórica.
- **[Fakes divergentes del real]** → Batería de contrato compartida (D-12); para FS/Trash/Sensors el contrato se fija en C-03/C-09/C-06 al crear el adaptador real.
- **[Perfil equivocado tras C-05]** Si C-05 activa `requireAdministrator` sin cambiar la fuente de `local_app_data`, la base iría al perfil del admin en over-the-shoulder. → Costura única documentada (D-10) y nota explícita para C-05.
- **[Rename del respaldo con archivo bloqueado]** Otro proceso (antivirus) puede tener la base abierta. → Error `Unavailable`, nada se borra; el usuario reintenta.
- **[Perfil itinerante]** Resuelto: la base va en `%LOCALAPPDATA%` y no viaja con el perfil; ver D-10.

## Migration Plan

Primer esquema: no hay datos previos. La migración 001 se aplica en el primer arranque. Rollback del change = descartar la rama; en una máquina de desarrollo, borrar a mano el `data.db` creado en `PCOPT_DATA_DIR`.

## Open Questions

Todas RESUELTAS por el dueño (2026-11-13):

- **Recuperación de la base en C-04** — RESUELTA: C-04 incorpora el estado de la base + la acción "Respaldar y recrear" (nunca recreación automática). Registrado en el scope de C-04 en `CHANGES.md`.
- **`Registry` diferido a C-16** (D-02) — RESUELTA: aprobado el desvío; C-16 define el port `Registry`. Registrado en el scope de C-16 en `CHANGES.md`.
- **Roaming vs. Local** (D-10) — RESUELTA: la base va en `%LOCALAPPDATA%` (no itinerante). Ver KB 09 DD-15.

## Apply notes

- **`recovery.rs` clasifica por encabezado, no abriendo SQLite**: abrir una base danada con `-wal`/`-shm` presentes hizo que SQLite modificara/borrara esos archivos auxiliares (test de sidecars en rojo). `backup_and_recreate` lee `user_version` de los bytes 60..64 del encabezado (tras verificar el magic `SQLite format 3\0`) y solo rechaza si es mayor al soportado. Limitacion: en una base WAL futura con el `user_version` aun solo en el `-wal`, el encabezado puede estar desactualizado; el archivo igual se conserva como `.bak` (rename, nada se borra).
- Misma salvedad al arrancar (`open_connection`): sobre un archivo principal corrupto con `-wal` huerfano, SQLite puede tocar el `-wal`; el archivo principal queda siempre intacto (verificado por bytes).
- Nombres de respaldo: `data.db.corrupt-<ms>.bak`, `data.db-wal.corrupt-<ms>.bak`, `data.db-shm.corrupt-<ms>.bak`; con colision, `...corrupt-<ms>-N.bak` (el mismo `N` para los tres).
- `RawLogRow` (struct) en `sqlite/rows.rs` en lugar de 8 parametros sueltos (`clippy::too_many_arguments`).
- `manage_store(app, override)` en `lib.rs` encapsula el cableado de `run()` para poder probarlo con `mock_builder` y un override en `TempDir` (nunca toca el `%LOCALAPPDATA%` real).
- `open_connection` ejecuta `quick_check` ANTES de leer la version y de activar WAL; el `PRAGMA busy_timeout` se fija al abrir.
- `CHANGES.md`/KB: C-04 recibe la UI de estado/recuperacion de la base; C-16 define `Registry`; KB 09 agrega DD-15 (LocalAppData).
- Smoke 9.3 (parcial): `npm run tauri dev` con `PCOPT_DATA_DIR` en un directorio temporal creo `data.db` con `user_version = 1` y 3 ajustes sembrados. El segundo paso manual (reemplazar por texto y relanzar) no se repitio a mano: lo cubren los tests `open_should_report_corrupt_and_never_recreate_automatically` y `manage_store_should_register_unavailable_state_when_the_database_is_corrupt`.
- Los tests de cableado `manage_store_*` y de `recovery` se escribieron junto con la implementacion (sin RED separado); el de sidecars si fallo y destapo el hallazgo de arriba; ademas se hizo una mutacion manual (desactivar la deteccion de colision) que puso en rojo 2 tests.

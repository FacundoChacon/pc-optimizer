# Proposal

## Why

C-01 dejó el esqueleto hexagonal (`domain/` y `adapters/` vacíos) y un test que impide I/O en el dominio, pero todavía no hay **puertos** contra los que escribir lógica pura ni **persistencia**. Los changes que siguen en el camino crítico (C-03 protección de rutas, C-04 settings, C-05 privilegios, C-06 monitor, C-07..C-11 escaneo/limpieza) necesitan traits estables con fakes para hacer Strict TDD sin tocar el disco real, y una base SQLite con migraciones versionadas donde guardar settings, escaneos y el log de auditoría de limpieza. Es el GATE 1 del roadmap: sin C-02 no puede arrancar ningún change de fase 1–2.

## What Changes

- **Puertos del dominio** (`src-tauri/src/domain/ports/`): traits síncronos y object-safe, sin I/O, con un error de puerto común (`PortError`) y fakes en memoria para tests:
  - `FileSystem` (canonicalizar, metadata sin seguir reparse points, listar directorio) — primer consumidor C-03; C-07/C-08 lo usan; C-11 lo extiende con lectura para hashing.
  - `Trash` (solo `move_to_trash`; **sin** borrado permanente) — consumidor C-09; C-08 agrega tamaño de Papelera y C-10 el vaciado.
  - `Sensors` (muestra de CPU/RAM/disco) — consumidor C-06; C-14 lo extiende con temperaturas/SMART.
  - `Clock` (UTC epoch ms) — consumidores: este change (nombre del respaldo) y todos los que registren timestamps.
  - Persistencia segregada por agregado: `SettingsStore`, `ScanStore`, `CleanupLogStore` y el alias `Store` que los agrupa.
- **Modelo del dominio persistido** (`domain/model.rs`): `EpochMillis`, `ScanId`, `ScanKind`, `ScanTrigger`, `ScanStatus`, `CleanupOutcome`, registros de escaneo y de log; constante de retención `SCAN_RETENTION = 50`.
- **Adaptador SQLite** (`src-tauri/src/adapters/sqlite/`): `SqliteStore` con `Mutex<Connection>`, pragmas (`foreign_keys`, `journal_mode=WAL`, `synchronous=NORMAL`, `busy_timeout`), migraciones por `PRAGMA user_version` en transacción, rechazo de bases de una versión más nueva, detección de corrupción y respaldo + recreación sin pérdida del archivo original.
- **Migración 001**: tablas `settings`, `scan`, `cleanup_log` (STRICT, CHECKs de enums y rangos), índices, **triggers que abortan UPDATE/DELETE sobre `cleanup_log`** y seed `ui_mode="simple"`, `autostart_tray=false`, `language="es"`.
- **Retención**: al crear un escaneo se conservan los 50 más recientes (nunca se borra uno `running`), en la misma transacción.
- **Ubicación de la base**: `<LocalAppData>\pc-optimizer\data.db`, con `PCOPT_DATA_DIR` como override solo en desarrollo; resolución pura e inyectable (costura para C-05, que cambiará la fuente del perfil al usuario de la sesión interactiva).
- **Arranque**: `run()` abre la base en `setup` y gestiona un `StoreState` (`Ready` | `Unavailable{error}`); la app **nunca** se cae ni recrea la base por su cuenta si la apertura falla.
- **Errores**: `StoreError` (dominio) → `AppError` con `kind` estables (`store_corrupt`, `store_schema_too_new`, `store_migration_failed`, `store_unavailable`), sin filtrar SQL ni internals.
- `Cargo.toml`: comentario de `[lints.cargo] unused_dependencies` actualizado con las dependencias que siguen pendientes y el change que las consume (`rusqlite`, `thiserror`, `tempfile` pasan a usarse).
- **Sin** nuevos comandos IPC, capabilities, dependencias ni permisos: el conjunto exacto de comandos (`get_app_info`) no cambia.

## Capabilities

### New Capabilities
- `domain-ports`: contratos entre el dominio puro y el sistema operativo/persistencia (puertos, error de puerto, reloj, fakes con el mismo contrato que los adaptadores reales, ausencia de borrado permanente en el puerto de Papelera).
- `local-store`: persistencia local SQLite (apertura y pragmas, migraciones versionadas, esquema y seed iniciales, log de limpieza append-only, retención de escaneos, timestamps UTC, manejo de base corrupta/más nueva/migración fallida, concurrencia).

### Modified Capabilities
- `runtime-config`: se agrega cómo se resuelve el directorio de datos por defecto y la ruta del archivo de base (precedencia del override de desarrollo sobre el perfil del usuario).

## Impact

- **Código**: nuevo `src-tauri/src/domain/{model.rs, ports/*}`, `src-tauri/src/adapters/{clock.rs, sqlite/*}`; cambios en `domain/mod.rs`, `adapters/mod.rs`, `config.rs`, `error.rs`, `lib.rs` (`setup` + `manage(StoreState)`).
- **Dependencias**: ninguna nueva. Se empiezan a usar `rusqlite` (bundled), `thiserror`, `serde_json` y `tempfile` (dev). `sysinfo`, `walkdir`, `trash`, `blake3`, `windows` siguen sin uso hasta C-05/C-06/C-07/C-09/C-11.
- **Datos de usuario**: primer archivo persistente (`%LOCALAPPDATA%\pc-optimizer\data.db` + `-wal`/`-shm`). Mientras C-05 no exista la app corre `asInvoker`, así que el token del proceso es el del usuario interactivo y la ruta es correcta.
- **Tests**: tests unitarios del dominio y de fakes, suite de contrato compartida fake ↔ SQLite, tests del adaptador sobre `tempfile` (nunca el directorio real). Siguen verdes `architecture`, `no_network_crates` y `security_baseline`.
- **Changes posteriores**: C-04 consume `SettingsStore`/`StoreState` y expone estado/recuperación de la base por IPC; C-05 reemplaza la fuente del perfil; C-07/C-08/C-11 agregan migraciones 002–004 con `ON DELETE CASCADE` hacia `scan`; C-16 define el puerto `Registry` (diferido, ver design).

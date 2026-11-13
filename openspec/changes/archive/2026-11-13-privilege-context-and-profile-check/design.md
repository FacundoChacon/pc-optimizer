# Design

> Gobernanza **HIGH**: este diseño se revisa y aprueba antes de escribir código. Las decisiones marcadas **[DUEÑO]** tienen opciones y una recomendación; los specs y las tareas están escritos para la opción recomendada, y la sección "Decisiones del dueño" indica qué cambia si se elige otra.

## Context

Ver `proposal.md` (Why). Estado actual verificado en código (rama `feat/privilege-context-and-profile-check`, desde `4bd5897`):

- **Manifest** (`src-tauri/build.rs`, C-01): `WindowsAttributes::new_without_app_manifest()` + `cargo::rustc-link-arg=/MANIFEST:EMBED` + `/MANIFESTDEPENDENCY:…Common-Controls 6.0.0.0…` para **todos** los binarios (app, lib tests, `tests/*.rs`). El linker genera el `trustInfo` por defecto (`level='asInvoker'`). C-01 constató que `cargo::rustc-link-arg-tests` es rechazado por cargo 1.100 ("invalid instruction") y que combinar el manifest de `tauri-build` con uno del linker da CVT1100. El spec `security-baseline` exige hoy "sin elevación forzada" hasta C-05.
- **Binarios**: `[lib] crate-type = ["rlib"]` + `src/main.rs` autodescubierto (sin `[[bin]]`); `main.rs` no tiene tests pero `cargo test` igual compila y ejecuta su arnés de tests. `cargo::rustc-link-arg-bins` aplica a los targets de tipo bin, **incluido su arnés de test** (regla de cargo: aplica por tipo de target, no por modo).
- **Carpetas conocidas** (`adapters/windows/known_folders.rs`, C-03): `UserFolderSource` solo tiene `ProcessToken` (`SHGetKnownFolderPath` con `hToken = NULL`); el dominio ya tiene `UserSource::{ProcessToken, InteractiveSession}` (costura). `resolve_protection_context` todavía no tiene consumidores en runtime (llegan con C-08).
- **Directorio de datos** (`config.rs`, `lib.rs::manage_store`): `app.path().local_data_dir()` de Tauri = `SHGetKnownFolderPath(FOLDERID_LocalAppData)` con el token del proceso; `PCOPT_DATA_DIR` solo en builds con `debug_assertions`. DD-15 fija `%LOCALAPPDATA%\pc-optimizer\data.db` "del perfil real del usuario interactivo" y SU-04 supone el perfil de la sesión interactiva aun en over-the-shoulder.
- **UI** (C-04): `Shell` renderiza `StoreBanner` global; providers en `src/app/providers`; IPC tipado con decodificadores en `src/lib/ipc.ts`; catálogo `src/shared/i18n/es.ts` (voseo); guardia de literales; `src/tests/support/mockBackend.ts` simula 5 comandos.
- **Crate `windows` 0.62.2** con features `Win32_Foundation`, `Win32_Security`, `Win32_Storage_FileSystem`, `Win32_System_Com`, `Win32_UI_Shell`. Verificado en el registro local: `ProcessIdToSessionId`, `WTSQuerySessionInformationW`, `WTSQueryUserToken` están en `Win32_System_RemoteDesktop`; `OpenProcess`/`OpenProcessToken`/`GetCurrentProcess` en `Win32_System_Threading`; `GetShellWindow`/`GetWindowThreadProcessId` en `Win32_UI_WindowsAndMessaging`; `GetTokenInformation`/`LookupAccountNameW` en `Win32_Security`. Las features `Win32_*` no agregan dependencias (solo `cfg`).

## Goals / Non-Goals

**Goals:**
- Release siempre elevado; tests y desarrollo nunca elevados; verificado por test sobre el manifest embebido.
- Contexto de privilegios fail-closed, calculado una vez, consultable por IPC y probado con fakes (match / different_user / undetermined).
- Contrato puro de alcance (`machine`/`per_user`) listo para C-08/C-09/C-18, sin definir categorías.
- Aviso global en mismatch siguiendo los patrones de C-04.

**Non-Goals:**
- Definir o clasificar categorías de limpieza (C-08) o acciones HKCU (C-18): solo el contrato.
- Resolver carpetas conocidas del usuario de la sesión interactiva (`UserFolderSource::InteractiveSession`): no se necesita (D-10).
- Selector de carpeta (propuesto para su primer consumidor, D-11).
- Autostart por Task Scheduler (C-22), instalador (C-13), re-evaluación del contexto en caliente.
- Mostrar nombres de cuenta o SIDs en la UI (D-12).

## Decisions

### D-01 — Estrategia de manifest: `requireAdministrator` solo en el binario de la app y solo en release **[DUEÑO]**

`build.rs` conserva lo de C-01 (`/MANIFEST:EMBED` + Common Controls v6 para todos) y **agrega, solo si `PROFILE == "release"`**:

```
cargo::rustc-link-arg-bins=/MANIFESTUAC:level='requireAdministrator'
```

Sin espacios en el argumento (evita problemas de comillas en el response file de rustc; `uiAccess` se omite y vale `false` por defecto). Al ser `-bins`, no toca lib tests ni `tests/*.rs`; para el arnés de test del bin ver D-02. Resultado:

| Binario | debug (`cargo test`, `tauri dev`) | release (`tauri build`, `cargo build --release`) |
|---|---|---|
| `pc-optimizer.exe` | `asInvoker` | `requireAdministrator` |
| lib tests, `tests/*.rs` | `asInvoker` | `asInvoker` |

Opciones consideradas:
- **A (recomendada)**: lo de arriba. `npm run tauri dev` sigue funcionando desde una terminal normal (la app corre sin elevar → `match`, `elevated=false`); para probar UAC/over-the-shoulder se usa el `.exe` de release. Si en dev se necesita elevación (C-09 limpiando `Windows\Temp`), se lanza `tauri dev` desde una terminal elevada y el proceso hereda la elevación.
- **B**: `requireAdministrator` en todos los perfiles para el bin. `tauri dev` desde terminal normal falla con error 740 (cargo usa `CreateProcess`, que no dispara UAC); obliga a desarrollar siempre elevado (más radio de impacto ante un bug durante el desarrollo). Rechazada.
- **C**: opt-in por variable de entorno al compilar (`PCOPT_REQUIRE_ADMIN=1`). Riesgo de publicar un release sin elevación si el pipeline lo olvida. Rechazada.
- **D**: archivo versionado `src-tauri/pc-optimizer.exe.manifest` vía `/MANIFESTINPUT:<ruta>` + `/MANIFESTUAC:NO` solo en bins release. Mismo comportamiento que A y el XML queda revisable, pero depende de una ruta absoluta en el argumento del linker (comillas si hay espacios) y del merge de manifests del linker. Queda como **plan B** si `/MANIFESTUAC:level=…` no funciona; el test de D-03 valida cualquiera de las dos.

El nombre `pc-optimizer.exe.manifest` del roadmap se toma como el manifest embebido, no como un archivo obligatorio. UAC cancelado: Windows no crea el proceso (no hay código propio que escribir); se verifica en el smoke.

### D-02 — `[[bin]] test = false`

Se declara el bin explícitamente en `Cargo.toml` (`name = "pc-optimizer"`, `path = "src/main.rs"`, `test = false`). `main.rs` no tiene tests (toda la lógica vive en la lib), y así `cargo test --release` no intenta ejecutar un arnés de test del bin enlazado con `requireAdministrator` (fallaría con 740 desde una terminal normal). `cargo test` sigue compilando el bin para los tests de integración (`CARGO_BIN_EXE_pc-optimizer`), lo que D-03 aprovecha.

### D-03 — Verificación del manifest con un lector PE propio (`tests/manifest.rs`)

Un test de integración con un lector mínimo y **seguro** (sin `unsafe`, sin crates nuevos) de formato PE en `tests/support/pe_manifest.rs`: DOS header → PE header → optional header (PE32/PE32+) → data directory de recursos → árbol `RT_MANIFEST (24)` → primer recurso → bytes UTF-8 del XML. De ahí extrae el atributo `level` de `requestedExecutionLevel` (comillas simples o dobles) y si existe la dependencia `Microsoft.Windows.Common-Controls` versión `6.0.0.0`. Todo acceso por `slice::get` con `Result` descriptivo: nunca indexa ni hace pánico.

¿Por qué no buscar la cadena en el `.exe`? Porque el propio binario del test contiene los literales esperados en `.rdata` (falso positivo). Leer el recurso evita eso y cubre el escenario "texto fuera del recurso".

Tests:
- `own_test_binary_*` (siempre): `std::env::current_exe()` → `asInvoker` + Common Controls v6 (y el binario contiene el literal `requireAdministrator` fuera del recurso, que el lector ignora).
- `app_binary_*` (siempre): `env!("CARGO_BIN_EXE_pc-optimizer")` → `asInvoker` si `cfg!(debug_assertions)`, `requireAdministrator` si no. Con `cargo test --release --test manifest` se verifica el release sin elevar (el binario del test es `asInvoker`; el `.exe` de la app solo se lee).
- Errores: archivo de texto en `TempDir`, copia truncada del propio ejecutable → `Err`, sin pánico.

Supuesto a vigilar: `cargo` usa el perfil `release` (`PROFILE=release`) para `--release` y los perfiles por defecto alinean `debug_assertions` con `PROFILE`. Si alguien define un perfil release con `debug-assertions = true`, el test lo detecta como fallo (deseable).

Verificación manual opcional: `mt.exe -inputresource:target\release\pc-optimizer.exe;#1 -out:con`.

### D-04 — Estructura de módulos

```
src-tauri/
  build.rs                              # + /MANIFESTUAC solo bins release (D-01)
  Cargo.toml                            # + [[bin]] test=false; + 3 features windows (D-14)
  src/domain/privileges.rs              # UserSid, SessionEvidence, decide, detect, PrivilegeContext,
                                        # UserScope, ScopeAccess, scope_access, restrict_protection_context
  src/domain/privileges_tests.rs        # #[path] desde privileges.rs
  src/domain/ports/session.rs           # trait SessionIdentity
  src/domain/ports/fakes.rs             # + FakeSessionIdentity
  src/adapters/windows/privileges.rs    # SystemSessionIdentity (unsafe mínimo)
  src/commands/privileges.rs            # PrivilegeContextDto + get_privilege_context
  src/lib.rs                            # manage_privileges + detección en setup
  tests/manifest.rs, tests/support/pe_manifest.rs
src/
  lib/ipc.ts                            # + PrivilegeContext + getPrivilegeContext
  app/providers/PrivilegeProvider.tsx   # nuevo
  app/providers/AppProviders.tsx        # envuelve con PrivilegeProvider
  app/shell/PrivilegeBanner.tsx         # nuevo
  app/shell/Shell.tsx                   # <PrivilegeBanner/> antes de <StoreBanner/>
  shared/i18n/es.ts                     # + sección privilege
  tests/support/mockBackend.ts          # + get_privilege_context
```

### D-05 — Cómo se determina el usuario interactivo **[DUEÑO]**

Se usa la **sesión del propio proceso** (`ProcessIdToSessionId(GetCurrentProcessId())`): una elevación over-the-shoulder corre en la sesión y el escritorio del usuario que la pidió, así que esa sesión identifica a la persona frente a la pantalla (vale también para RDP y cambio rápido de usuario, donde la "sesión de consola" sería otra). Dos señales independientes de esa sesión:

1. **Shell**: `GetShellWindow()` → `GetWindowThreadProcessId` → pid → se descarta si `ProcessIdToSessionId(pid) ≠ sesión propia` → `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` → `OpenProcessToken(TOKEN_QUERY)` → `GetTokenInformation(TokenUser)` → SID. Sin ventana de shell → señal ausente.
2. **Logon de la sesión**: `WTSQuerySessionInformationW(WTS_CURRENT_SERVER_HANDLE, sesión, WTSUserName / WTSDomainName)` → `LookupAccountNameW("DOMINIO\usuario")` → SID. Usuario vacío (sesión 0, sin logon) → señal ausente.

Regla (D-06): `match` solo si el SID del token es legible, hay ≥ 1 señal y todas las disponibles coinciden con él.

Opciones consideradas:
- **A (recomendada)**: las dos señales de arriba, "al menos una y todas coinciden". La señal WTS cubre arranques sin shell (explorer caído, autostart temprano de C-22); la del shell da un SID directo sin búsqueda por nombre.
- **B**: solo WTS + `LookupAccountNameW`. Más simple, pero depende de resolver nombre→SID (cuentas de dominio/Azure AD sin conexión pueden fallar → `undetermined` falso).
- **C**: solo dueño de `explorer.exe`/shell. Falla con shells alternativos o explorer reiniciándose.
- **D**: sesión de consola (`WTSGetActiveConsoleSessionId`). Incorrecta en RDP y con varios usuarios. Rechazada.
- **E**: `WTSQueryUserToken`. Requiere `SeTcbPrivilege` (solo LocalSystem); un admin elevado no lo tiene. Inviable.
- **Variante estricta de A**: exigir **ambas** señales para `match`. Más "fail-closed" pero bloquearía lo por-usuario cada vez que el shell no esté (falsos `undetermined`). No recomendada.

Supuesto a verificar en apply (test ignorado + smoke): un admin elevado puede abrir con `TOKEN_QUERY` el token del shell de un usuario estándar de su misma sesión (el DACL por defecto del token suele permitirlo; Administrador de tareas elevado muestra el usuario de todos los procesos). Si no puede, la señal 1 queda ausente y decide la 2 (sin cambio de diseño).

### D-06 — Decisión pura en el dominio

```rust
pub struct UserSid(Vec<u8>);              // from_bytes valida: revisión 1, n ≤ 15 subautoridades, len = 8 + 4n
pub struct SessionEvidence { pub process_user: Option<UserSid>, pub shell_user: Option<UserSid>, pub logon_user: Option<UserSid> }
pub enum MismatchReason { DifferentUser, Undetermined }      // as_str: "different_user" | "undetermined"
pub enum UserMatch { Match, Mismatch(MismatchReason) }
pub struct PrivilegeContext { user_match: UserMatch, elevated: bool }   // Copy; constructores + getters
pub fn decide(evidence: &SessionEvidence) -> UserMatch
pub fn detect(identity: &impl SessionIdentity) -> PrivilegeContext
```

`decide`: token `None` → `Undetermined`; sin señales → `Undetermined`; señales distintas entre sí → `Undetermined`; señales iguales entre sí y ≠ token → `DifferentUser`; iguales al token → `Match`. `detect` convierte cada `Err` del puerto en señal ausente (y la elevación en `false`), así que nunca falla. SID por bytes (no por nombre ni por variables de entorno). Todo puro, cubierto por tabla de casos con `FakeSessionIdentity`.

### D-07 — Elevación solo informativa

`elevated` = `TokenElevation.TokenIsElevated` del proceso; error ⇒ `false`. No bloquea nada (si el release corriera sin elevar, p. ej. con `__COMPAT_LAYER=RunAsInvoker`, el SO negará lo que requiera admin y C-09 lo reportará) y nunca amplía la whitelist (RN-PRIV-05). Sirve para el smoke y para que C-09 explique fallos.

### D-08 — Directorio de datos frente a DD-15 / SU-04 **[DUEÑO]**

Conflicto detectado: SU-04 pide la base en el `%LOCALAPPDATA%` del **usuario interactivo** aun cuando el proceso corre como otra cuenta; DD-15 y el código actual la ubican en el `%LOCALAPPDATA%` del **token del proceso** (que coincide con el interactivo salvo en over-the-shoulder).

Hallazgo de seguridad: en over-the-shoulder el usuario interactivo es **estándar** y controla su perfil. Si el proceso elevado escribe en `C:\Users\ana\AppData\Local\pc-optimizer`, `ana` puede plantar una junction en esa ruta (o en `…\Local`) hacia `C:\Windows\…` y el proceso admin crearía/renombraría archivos allí (`data.db`, `-wal`, `-shm`, `.corrupt-<ms>.bak` de C-04): una **primitiva clásica de escalada de privilegios**. SQLite abre por ruta repetidas veces, así que no se puede blindar con un chequeo previo (TOCTOU). Además contradice el propio espíritu de RN-PROT ("perfiles de otros usuarios: nunca se tocan").

Opciones:
- **A — perfil del token del proceso (recomendada, sin cambio de código)**: se mantiene DD-15 literal (`app.path().local_data_dir()`). En `match` es el perfil del usuario interactivo; en over-the-shoulder, el de la cuenta admin que aprobó el UAC (perfil que el usuario estándar no controla). Se registra **DD-16** y SU-04 pasa a "refutado en C-05". Costo: en over-the-shoulder, historial y ajustes quedan en el perfil del admin.
- **B — perfil del usuario interactivo (SU-04 literal)**: resolver su `LocalAppData` (ProfileList del registro + `AppData\Local`, o `SHGetKnownFolderPath` con el token del shell abierto con `TOKEN_IMPERSONATE`, que probablemente se deniegue). Introduce la primitiva de escalada descrita. **No recomendada.**
- **C — `%ProgramData%\pc-optimizer` por máquina**: contradice DD-15; los usuarios estándar pueden crear carpetas en `ProgramData` (squatting previo), así que exigiría crear la carpeta con ACL endurecida y verificar dueño. Más alcance y riesgo.
- **D — sin persistencia en mismatch**: la base queda `Unavailable` (nuevo motivo) durante over-the-shoulder. Fail-closed máximo, pero sin `cleanup_log` C-09 no podría limpiar ni categorías de máquina, contra PA-14 ("nada más deshabilitado").

Con A no cambia `config.rs` ni `manage_store` (solo su comentario "costura para C-05" pasa a documentar la decisión); el spec `runtime-config` lo deja explícito con un escenario over-the-shoulder. `PCOPT_DATA_DIR` sigue igual (solo builds con `debug_assertions`).

### D-09 — Contrato de alcance para C-08 / C-09 / C-18

```rust
pub enum UserScope { Machine, PerUser }
pub enum ScopeAccess { Allowed, Blocked(MismatchReason) }
pub fn scope_access(ctx: &PrivilegeContext, scope: UserScope) -> ScopeAccess
```

Cada categoría (C-08) y cada mejora (C-16/C-18) declarará su `UserScope`; toda acción sobre HKCU es `PerUser`. Guía (no se codifica acá): por-usuario = `temp_user`, `browser_cache_*`, `thumbnail_cache`, `recycle_bin`, partes por-usuario de `crash_dumps`; máquina = `temp_windows`, `wu_cache`. C-08 consulta `scope_access` al escanear y C-09 de nuevo justo antes de mover a la Papelera (mismo patrón anti-TOCTOU que la protección). Se elige una función pura y no un flag en la UI para que el bloqueo viva en el backend; la UI solo explica.

### D-10 — Contexto de protección en mismatch y la costura de C-03

`UserFolderSource` **no** gana la variante de sesión interactiva: en `match` el token del proceso **es** el usuario interactivo, así que `ProcessToken` ya da las carpetas correctas; en `mismatch` daría las del admin, y en lugar de resolver las del usuario interactivo (mismo problema de D-08) se usa:

```rust
pub fn restrict_protection_context(raw: RawProtectionContext, ctx: &PrivilegeContext) -> RawProtectionContext
```

que en `mismatch` pone `raw.user = None` (resto intacto). Por la regla existente de C-03 ("usuario actual desconocido ⇒ toda ruta bajo la raíz de perfiles es `protected/other_user_profile`") eso protege **todos** los perfiles (el del admin y el del usuario interactivo) sin tocar `Windows\Temp` ni `SoftwareDistribution\Download`. Es defensa en profundidad además de D-09. C-08 debe construir su contexto como `restrict_protection_context(resolve_protection_context(ProcessToken, app_dirs), &ctx)`; se deja documentado en el doc-comment de `resolve_protection_context`. `UserSource::InteractiveSession` del dominio queda sin uso (no se borra para no tocar `path-protection` en este change; eliminarlo es un follow-up LOW si el dueño lo prefiere).

### D-11 — Selector de carpeta (UIPI) **[DUEÑO]**

Hoy ninguna pantalla recibe carpetas: el primer consumidor es C-07 (raíz de escaneo, si permite elegirla) o C-21 (reglas personalizadas). Opciones:

- **A (recomendada) — sacarlo de C-05** y entregarlo con su primer consumidor, implementado como comando Rust `pick_folder` con `IFileOpenDialog` (`FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST`) del crate `windows` ya presente (`Win32_UI_Shell` + `Win32_System_Com`, hilo STA propio, ventana dueña = ventana principal); devuelve solo la ruta elegida, que luego pasa por la validación de raíces de C-03. Sin crates ni plugins nuevos. Razones: no hay pantalla que lo use para validar la UX, agregaría un comando + permiso + `unsafe` COM sin consumidor, y no cambia la seguridad de C-05. Se actualiza `CHANGES.md` (scope de C-05 y del consumidor) tras la aprobación.
- **B — implementarlo ya en C-05** como el comando de A (≈ 1 grupo de tareas más: adapter COM, comando, permiso `allow-pick-folder`, test ignorado, botón de demo en Configuración sin efecto). Si el dueño lo elige, se agrega ese grupo a `tasks.md` antes del apply.
- **C — `tauri-plugin-dialog`**: dependencia nueva (crate + paquete npm, cambia ambos lockfiles) y capability `dialog:allow-open` expuesta al webview. Requiere aprobación explícita por regla dura; no aporta nada que A no cubra. No recomendada.

En todas: la UI nunca depende de arrastrar y soltar desde el Explorador (UIPI lo bloquea hacia ventanas elevadas, RN-PRIV-06); no se tocan `dragDropEnabled`, capabilities de plugins ni CSP.

### D-12 — Comando, DTO y estado gestionado

- Detección en `run()` → `setup`, antes de `manage_store`: `let ctx = privileges::detect(&SystemSessionIdentity); manage_privileges(app, ctx);`. Se calcula **una vez** (el usuario del token no cambia durante la vida del proceso y la sesión no cambia de usuario); re-evaluar en caliente podría desbloquear categorías a mitad de un flujo. `PrivilegeContext` es `Copy` y se gestiona directamente (`State<'_, PrivilegeContext>`).
- `get_privilege_context` (`async`, sin trabajo bloqueante) → `Result<PrivilegeContextDto, AppError>`, siempre `Ok`. Sin `kind` de error nuevo.
- DTO `#[serde(tag = "status", rename_all = "snake_case")]`: `Match { elevated }` / `Mismatch { reason: &'static str, elevated }` → `{"status":"match","elevated":true}` / `{"status":"mismatch","reason":"different_user","elevated":true}`. **Sin** SIDs, nombres ni rutas (alternativa descartada: mostrar "corre como DOMINIO\admin"; ayuda a explicar pero agrega `LookupAccountSidW` y datos de cuenta al webview; puede sumarse luego si el dueño lo pide).
- `APP_COMMANDS` += `get_privilege_context`; `capabilities/main-window.json` += `allow-get-privilege-context` (el test `security_baseline` exige el conjunto exacto).

### D-13 — Frontend

- `ipc.ts`: `PrivilegeContext` = `{status:"match"; elevated:boolean} | {status:"mismatch"; reason:"different_user"|"undetermined"; elevated:boolean}`; `getPrivilegeContext()` con decodificador estricto (forma inválida → `invalid_response`).
- `PrivilegeProvider`: consulta una vez al montar; estados `loading | ready(context)`. **Fail-closed**: error o respuesta inválida ⇒ `ready({status:"mismatch", reason:"undetermined", elevated:false})`. Durante `loading` no hay aviso (sin parpadeo); el bloqueo real vive en el backend, así que no hay ventana de riesgo.
- `PrivilegeBanner` (en `Shell`, antes de `StoreBanner`): `role="alert"`, `StatusBadge tone="warning"` con título por motivo, explicación, qué se bloquea, qué sigue disponible y cómo resolverlo; sin botón de cierre (el riesgo persiste mientras corra la app). Textos en `es.privilege` (voseo, sin jerga: "otra cuenta de Windows", no "token"/"SID").
- `mockBackend`: `get_privilege_context` con estado `privilege` (default `match`, así los tests existentes no cambian).

### D-14 — Features del crate `windows` y lockfile

Se agregan `Win32_System_Threading`, `Win32_System_RemoteDesktop`, `Win32_UI_WindowsAndMessaging` a la línea existente de `windows = "=0.62.2"`. Son `cfg` internos del crate (sin dependencias opcionales), por lo que `Cargo.lock` no cambia; se verifica con `cargo build --locked` + `git diff --exit-code src-tauri/Cargo.lock`. Antes de codear, las firmas exactas se confirman con la skill `winmd-api-search` y el código fuente del crate en el registro. Si alguna feature cambiara el lockfile: detener y consultar.

### D-15 — `unsafe` en el adaptador

Todo en `adapters/windows/privileges.rs`, un bloque por llamada Win32, cada uno con `// SAFETY:` (lint `undocumented_unsafe_blocks = "deny"` ya activo): `GetCurrentProcess`/`OpenProcessToken`, `GetTokenInformation` (patrón de dos llamadas con buffer `Vec<u8>` alineado a `TOKEN_USER` — usar `Vec<u64>` o copiar con `read_unaligned`), lectura de `TOKEN_USER.User.Sid` + `GetLengthSid`/`IsValidSid` + copia a `Vec<u8>` propio, `TokenElevation`, `ProcessIdToSessionId`, `GetShellWindow`, `GetWindowThreadProcessId`, `OpenProcess`, `WTSQuerySessionInformationW` + `WTSFreeMemory` (siempre liberado, también en error), `LookupAccountNameW` (dos llamadas), `CloseHandle` vía guard RAII (`windows::core::Owned<HANDLE>` si está disponible en 0.62, si no un `struct HandleGuard` propio con `Drop`). Lógica pura (unir `DOMINIO\usuario`, tratar vacíos, validar SID) fuera de los bloques y testeada sin SO. Riesgo de reuso de pid entre `GetWindowThreadProcessId` y `OpenProcess`: mitigado por la verificación de sesión y por la regla "señales contradictorias ⇒ undetermined".

### D-16 — Tests

- Dominio (siempre): `UserSid::from_bytes` (válidos/ inválidos), tabla de `decide` (todos los escenarios del spec), `detect` con `FakeSessionIdentity` (errores ⇒ ausente, elevación con error ⇒ `false`), `scope_access` (4 combinaciones + HKCU), `restrict_protection_context` + evaluación real con `ProtectionService`/contexto de prueba de C-03 (perfil del admin ⇒ `other_user_profile`; `Windows\Temp` igual que en match).
- Adaptador: helpers puros siempre; consultas reales `#[ignore = "consulta el token y la sesión reales de esta máquina; correr a mano con --ignored"]`.
- Comandos: DTO exacto; IPC con runtime mock y `manage_privileges(fake ctx)` (match y mismatch).
- Manifest: D-03 (debug siempre; release con `--release --test manifest`).
- Frontend (Vitest): decodificador, provider (incluido fail-closed), banner en dos pantallas y ambos modos, guardia de literales.
- Arquitectura: `domain/privileges.rs` y `domain/ports/session.rs` no importan `windows`/`tauri`/`std::fs` (test existente).

## Risks / Trade-offs

- **[`/MANIFESTUAC:level='requireAdministrator'` mal interpretado por rustc/link]** → El test D-03 en release lo detecta; plan B = opción D de D-01 (`/MANIFESTUAC:NO` + `/MANIFESTINPUT`).
- **[`rustc-link-arg-bins` también alcanza el arnés de test del bin]** → D-02 (`test = false`); verificado con `cargo test --release --locked` desde terminal no elevada.
- **[Token del shell no legible para el admin]** → La señal 1 queda ausente; decide WTS (D-05). Verificado por test ignorado y smoke over-the-shoulder.
- **[`LookupAccountNameW` lento o fallido (cuentas de dominio/Azure AD sin red)]** → Si el shell está, decide el shell; si ambas faltan ⇒ `undetermined` (fail-closed, solo bloquea lo por-usuario). La detección corre en `setup` (bloquea el arranque mientras dure): el smoke mide que no se perciba; si supera ~200 ms, mover a un hilo con estado diferido (cambio de diseño a consultar).
- **[Falsos `undetermined`]** → Solo bloquean lo por-usuario y se explican en el aviso; preferible a limpiar el perfil equivocado.
- **[Over-the-shoulder deja ajustes/historial en el perfil del admin (D-08 A)]** → Aceptado y documentado (DD-16); evita la primitiva de escalada.
- **[Desarrollo sin elevar no ejercita el camino elevado]** → Smoke con el `.exe` de release; `tauri dev` desde terminal elevada cuando haga falta.
- **[Smoke en release usa el `%LOCALAPPDATA%` real]** → `PCOPT_DATA_DIR` no aplica en release; el smoke solo lee/crea `pc-optimizer\data.db` de las cuentas de prueba; el dueño puede borrar esa carpeta después.
- **[Instalador NSIS "ejecutar al terminar"]** → Si lanza la app con `CreateProcess` fallaría con 740; se anota para C-13 (no se toca el bundle aquí).
- **[Reuso de pid del shell]** → Ventana mínima; verificación de sesión + regla de señales contradictorias.

## Migration Plan

1. Sin migración de datos: la base sigue en el mismo lugar (D-08 A).
2. Tras el merge, los ejecutables de release piden UAC; los builds de desarrollo no cambian.
3. Rollback: revertir el commit; quitar la línea `/MANIFESTUAC` devuelve el release a `asInvoker` sin efectos en datos.
4. Documentación tras aprobación: KB `09` (DD-16, SU-04 refutado, DD-02 referencia al mecanismo), `13` §Over-the-shoulder (detección por sesión propia + señales; datos en el perfil del token), `04` (ubicación de la base), `CHANGES.md` (C-05 scope y selector movido; nota para C-08/C-09/C-13/C-18/C-22).

## Decisiones del dueño (bloquean el apply)

| # | Tema | Recomendación | Si se elige otra |
|---|------|---------------|------------------|
| D-01 | Manifest en tests/dev | A: `requireAdministrator` solo bin + release; dev y tests `asInvoker` | B: cambia el escenario "Manifest del ejecutable de desarrollo" y el smoke exige terminal elevada |
| D-05 | Usuario interactivo | A: sesión propia, señales shell + WTS, "≥1 y todas coinciden" | Variante estricta: cambia el escenario "Sin shell en ejecución" a `undetermined` |
| D-08 | Directorio de datos | A: perfil del token del proceso (DD-15), SU-04 refutado | B/C/D: reescribir el delta de `runtime-config` (y `local-store` en D) y agregar tareas |
| D-11 | Selector de carpeta | A: fuera de C-05, comando Rust con `IFileOpenDialog` en su primer consumidor | B: agregar grupo de tareas; C: requiere aprobar dependencia y capability nuevas |

**Aprobado por el dueño el 2026-11-13: opción A en D-01, D-05 (variante no estricta), D-08 y D-11.**

## Open Questions

- Texto final del aviso (tono y largo): se ajusta en apply sin cambiar el spec (el spec fija qué debe comunicar, no la redacción).

## Apply notes

_(Se completa durante el apply: firmas Win32 confirmadas (1.2), plan B del manifest si aplica (2.2/2.3), revisión de `unsafe` (4.4) y resultados del smoke (8.2).)_

### 1.2 — Firmas Win32 confirmadas (`windows-0.62.2` / `windows-core-0.62.2`, fuente del registro de cargo)

Todas viven bajo `windows::Win32::…`; ninguna requiere una feature distinta de las 3 de D-14 (más `Win32_Foundation` y `Win32_Security`, ya presentes). Las features de `windows` no tienen dependencias transitivas entre sí (`Win32_X = ["Win32"]`), así que hay que declararlas todas.

| API | Módulo | Feature | Firma (resumen) |
|-----|--------|---------|-----------------|
| `GetCurrentProcess` | `System::Threading` | `Win32_System_Threading` | `unsafe fn() -> HANDLE` (seudo-handle, no se cierra) |
| `OpenProcess` | `System::Threading` | `Win32_System_Threading` | `unsafe fn(PROCESS_ACCESS_RIGHTS, bool, u32) -> Result<HANDLE>`; `PROCESS_QUERY_LIMITED_INFORMATION` |
| `OpenProcessToken` | `System::Threading` (no `Security`) | `Win32_System_Threading` + `Win32_Security` | `unsafe fn(HANDLE, TOKEN_ACCESS_MASK, *mut HANDLE) -> Result<()>`; `TOKEN_QUERY` en `Security` |
| `GetTokenInformation` | `Security` | `Win32_Security` | `unsafe fn(HANDLE, TOKEN_INFORMATION_CLASS, Option<*mut c_void>, u32, *mut u32) -> Result<()>`; `TokenUser` (1) -> `TOKEN_USER`, `TokenElevation` (20) -> `TOKEN_ELEVATION` |
| `GetLengthSid` | `Security` | `Win32_Security` | `unsafe fn(PSID) -> u32` |
| `IsValidSid` | `Security` | `Win32_Security` | `unsafe fn(PSID) -> BOOL` (windows_core) |
| `LookupAccountNameW` | `Security` | `Win32_Security` | `unsafe fn(P0 sistema, P1 cuenta, Option<PSID>, *mut u32, Option<PWSTR>, *mut u32, *mut SID_NAME_USE) -> Result<()>` |
| `ProcessIdToSessionId` | `System::RemoteDesktop` | `Win32_System_RemoteDesktop` | `unsafe fn(u32, *mut u32) -> Result<()>` (en kernel32) |
| `WTSQuerySessionInformationW` | `System::RemoteDesktop` | `Win32_System_RemoteDesktop` | `unsafe fn(Option<HANDLE>, u32, WTS_INFO_CLASS, *mut PWSTR, *mut u32) -> Result<()>`; `WTSUserName` (5), `WTSDomainName` (7) |
| `WTSFreeMemory` | `System::RemoteDesktop` | `Win32_System_RemoteDesktop` | `unsafe fn(*mut c_void)` |
| `WTS_CURRENT_SERVER_HANDLE` | `System::RemoteDesktop` | `Win32_System_RemoteDesktop` | `const HANDLE = HANDLE(0)` (se pasa como `Some(..)`) |
| `GetShellWindow` | `UI::WindowsAndMessaging` | `Win32_UI_WindowsAndMessaging` | `unsafe fn() -> HWND` |
| `GetWindowThreadProcessId` | `UI::WindowsAndMessaging` | `Win32_UI_WindowsAndMessaging` | `unsafe fn(HWND, Option<*mut u32>) -> u32` (devuelve el thread id; el pid sale por el puntero) |
| `windows::core::Owned<HANDLE>` | `windows_core::handles` | (sin feature) | `Owned<T: Free>`; `impl Free for HANDLE` llama `CloseHandle`: sirve como guard RAII de handles. Para el buffer de WTS no hay `Free`, se necesita un guard propio con `WTSFreeMemory`. |

Aclaraciones para el batch 2: `GetCurrentProcessId` no esta en la lista de 1.2; vive en `System::Threading` (misma feature). `OpenProcessToken` NO esta en `Win32::Security`, sino en `System::Threading`. `LookupAccountNameW` devuelve `Err` con `ERROR_INSUFFICIENT_BUFFER` en la primera llamada de dimensionado (esperado).

### 1.1 — Linea base (antes de tocar codigo)
Vitest 137 tests (la primera corrida fue 136/137 con 1 fallo no reproducible, concurrente con `npm run build`; 3 corridas posteriores 137/137: flaky de carga, no preexistente determinista). Cargo debug: lib 358 + architecture 5 + no_network_crates 9 + security_baseline 21 = 393 passed, 2 ignored. fmt y clippy verdes. `npm run build` verde.

### 2.2 / 2.3 — Manifest: se logro con la opcion A (sin plan B)
`build.rs` emite `cargo::rustc-link-arg-bins=/MANIFESTUAC:level='requireAdministrator'` solo con `PROFILE == "release"` en Windows; el linker lo combina con `/MANIFEST:EMBED` + `/MANIFESTDEPENDENCY` existentes (sin CVT1100). Evidencia: RED release = `left: Some("asInvoker") right: Some("requireAdministrator")`; GREEN debug y release (`asInvoker` / `requireAdministrator`, Common Controls v6 en ambos). En 2.3 el RED fue real: sin `test = false`, `cargo test --release` falla al ejecutar el arnes del bin con `os error 740`; con `[[bin]] test = false` la suite release completa corre sin elevar. Cargo.lock sin cambios.

### 4.3 / 4.4 - Adaptador real y revision de unsafe
- `GetTokenInformation` con clases de tamano fijo (`TokenElevation`) responde `ERROR_BAD_LENGTH` (24), no `ERROR_INSUFFICIENT_BUFFER`, en la llamada de dimensionado con longitud 0. Lo detecto el test real ignorado (RED: `win32 error 24`); `is_size_probe` acepta ambos codigos. `PSID` esta en `Win32::Security` (no `Foundation`).
- `unsafe`: 22 bloques, todos en `adapters/windows/privileges.rs`, cada uno con `// SAFETY:`; sin `transmute`; ninguno envuelve logica pura (nombres, codigos de error y SID se procesan fuera). Handles: `Owned<HANDLE>` (CloseHandle una vez al soltar); buffer WTS: guard `WtsString` con `WTSFreeMemory` en `Drop` (una vez); buffers de token en `Vec<u64>` (alineacion 8) y el SID se copia a `Vec<u8>` antes de soltarlos.
- Medicion real (terminal normal, sesion propia): `detect()` = ~2 ms (`Match`, `elevated=false`); logon y shell presentes y coinciden con el proceso. Muy por debajo de 200 ms: se mantiene en el hilo de `setup`.
- Lanzamiento de `lib.rs`: `manage_privileges` se llama antes de `manage_store` en `setup`; en caso de panic de la deteccion no hay fallback (`detect` no falla por contrato).

### 6 / 7 - Frontend y documentacion
- `PrivilegeProvider` (fail-closed: error o respuesta invalida -> `mismatch/undetermined`), `PrivilegeBanner` antes de `StoreBanner`, catalogo `es.privilege`. Documentado: KB 09 (DD-16, SU-04 refutado, mecanismo de DD-02), 13, 04 y CHANGES.md (C-05, C-07, C-08, C-09, C-13, C-18, C-22).

### 8.2 - Smoke manual (2026-11-13)
Exe de release `npm run tauri build -- --no-bundle` (HEAD d8e1e38) copiado a `C:\Users\Public\pcopt-smoke\`. Politica UAC de fabrica (`EnableLUA=1`, `ConsentPromptBehaviorAdmin=5`, escritorio seguro). Lo que se pudo, se verifico por API desde scripts; el dueño solo respondio los UAC y confirmo lo visual.
- **(a) PASS**: `npm run tauri dev` desde terminal no elevada con `PCOPT_DATA_DIR` temporal: sin 740, sin aviso, `data.db` en el directorio temporal.
- **(b) PASS**: lanzamiento como doble click (`Start-Process`, ShellExecute) y "No" en UAC: Windows devuelve "El usuario ha cancelado la operacion" (`ERROR_CANCELLED`, 1223) y quedan 0 procesos `pc-optimizer`. (Dos intentos previos fueron "Si" por confusion; no hubo apertura sin UAC.)
- **(c) PASS**: "Si" en UAC: ventana a ~3 s del lanzamiento (incluye el tiempo del dialogo); `TokenIsElevated=1` leido del token del proceso (control: `explorer` = 0); usa `%LOCALAPPDATA%\pc-optimizer\data.db` de la cuenta (el `-shm` se toca 1 s despues del arranque) y no existe otra carpeta `pc-optimizer` en otros perfiles; el dueño confirmo Inicio sin aviso (solo el estado vacio de Inicio). Elevado y con shell, el adaptador real da `logon` y `shell` presentes y `detect() = Match, elevated: true` en ~2 ms.
- **(d) DIFERIDO por decision del dueño (2026-11-13)**: requiere iniciar sesion con una cuenta estandar de prueba. La logica queda cubierta por los tests del dominio (`different_user` con señales en desacuerdo) y el banner por los tests de Vitest; la verificacion real over-the-shoulder queda pendiente para cuando haya una cuenta estandar disponible.
- **(e) deteccion PASS / UI no reproducible**: con `explorer.exe` detenido (requirio poner `HKLM\...\Winlogon\AutoRestartShell=0` temporalmente, aprobado por el dueño y restaurado a 1 en `finally`; explorer se reinicio desde un proceso NO elevado) y proceso elevado, el adaptador real da `logon_present = true, shell_present = false` y `detect() = Match, elevated: true` (~1.9 ms): decide la señal WTS (D-05). La app de release, en cambio, **no puede abrir sin shell**: Tauri hace panic en su propio setup (`tauri-2.12.5 app.rs:1444`, exit 101) porque WebView2 no crea el webview (`HRESULT 0x80070490`, "No se ha encontrado el elemento"). Control: la misma app lanzada igual desde el mismo proceso elevado con explorer vivo abre en ~300 ms. La ventana del config se crea antes del hook `setup` de la app, asi que la falla es independiente de C-05 (no hay deteccion involucrada). Riesgo anotado para C-26/C-27 (arranque por Task Scheduler al iniciar sesion: si la tarea corre antes de que exista el shell, la app podria morir sin mensaje): ver "Riesgo abierto" abajo.
- **(f) PASS**: sin demora perceptible segun el dueño; `detect()` ~2 ms en todas las corridas.
- **(g) PASS**: `mt.exe` muestra `requireAdministrator` y Common-Controls 6.0.0.0.

Metodo (reproducible): `Start-Process` del exe desde PowerShell no elevado para (b)/(c) (equivale a doble click); elevacion por `OpenProcessToken` + `GetTokenInformation(TokenElevation)`; para (e), un PowerShell elevado (1 UAC) que guarda `AutoRestartShell`, lo pone en 0, detiene explorer, corre los tests `#[ignore]` de `privileges` desde el binario de tests ya compilado (sin compilar elevado) y lanza la app con stderr redirigido y `RUST_BACKTRACE=1`; un `finally` restaura el valor y el lado no elevado vuelve a lanzar `explorer.exe` para que el shell no quede elevado.

**Riesgo abierto (fuera de C-05)**: sin shell, WebView2 falla y Tauri hace panic sin mensaje visible (subsistema windows, stderr perdido). Revisar en C-26/C-27 (autostart y bandeja): demorar el arranque hasta que exista el shell o capturar el error de creacion de la ventana y registrarlo en lugar de morir en silencio.

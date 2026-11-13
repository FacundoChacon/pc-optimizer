# Proposal

## Why

La app debe correr siempre elevada (RN-PRIV-01, DD-02), pero hoy el ejecutable se enlaza `asInvoker` (C-01 lo difirió a C-05) y nada detecta el escenario **over-the-shoulder**: un usuario estándar escribe en el UAC las credenciales de OTRA cuenta admin y el proceso pasa a correr como esa cuenta, de modo que `%TEMP%`, cachés de navegador, Papelera y HKCU pertenecen al perfil equivocado (RN-PRIV-03, US-023). Antes de que C-08/C-09/C-18 introduzcan categorías por-usuario y acciones HKCU, tiene que existir un contexto de privilegios confiable (fail-closed) y un contrato único para bloquearlas sin deshabilitar nada más (PA-14).

## What Changes

- **Elevación obligatoria del ejecutable de release**: el `.exe` de release embebe un manifest `requireAdministrator` (UAC en cada arranque; UAC cancelado = la app no inicia). Los binarios de test y el build de desarrollo siguen `asInvoker`, así `cargo test` y `npm run tauri dev` corren desde una terminal no elevada (decisión del dueño, ver design D-01). Un test inspecciona el manifest embebido de los binarios.
- **Detección del contexto de privilegios** (nuevo): un adaptador de Windows obtiene el usuario (SID) del token del proceso, si el token está elevado, y el usuario de la sesión interactiva **del propio proceso** por dos señales independientes (dueño del shell de la sesión y usuario de logon de la sesión vía WTS). Una función pura del dominio decide `match` o `mismatch` con motivo `different_user` o `undetermined`; sin señales, token ilegible o señales contradictorias ⇒ `mismatch/undetermined` (fail-closed). Se calcula una vez al iniciar.
- **Contrato para categorías por-usuario y acciones HKCU** (nuevo, sin inventar categorías): alcance `machine`/`per_user` y una regla pura que, en mismatch, bloquea **solo** el alcance por-usuario; C-08 (categorías), C-09 (re-verificación al ejecutar) y C-18 (acciones HKCU) lo consumen. En mismatch, el contexto de protección de rutas se construye **sin usuario actual**, de modo que todo el árbol de perfiles queda protegido (defensa en profundidad).
- **Comando IPC `get_privilege_context`** → `{ status: "match", elevated }` o `{ status: "mismatch", reason, elevated }`, sin SIDs, nombres de cuenta ni rutas; permiso `allow-get-privilege-context` (sin plugins ni cambios de CSP).
- **UI**: aviso global persistente en el shell cuando hay mismatch (o si la consulta falla: fail-closed), con icono + texto del catálogo que explica qué se bloquea, qué sigue disponible y cómo resolverlo.
- **Directorio de datos**: se **mantiene** en `%LOCALAPPDATA%\pc-optimizer` del usuario del token del proceso (DD-15) y se propone **reemplazar SU-04**: escribir la base desde un proceso elevado dentro del perfil de un usuario estándar es una primitiva de escalada (junctions plantadas por ese usuario). Decisión del dueño, ver design D-08. `PCOPT_DATA_DIR` sigue igual.
- **Selector de carpeta (UIPI)**: se propone **sacarlo de C-05** y entregarlo con su primer consumidor (C-07/C-21) como comando Rust con `IFileOpenDialog` (sin crates ni plugins nuevos). Decisión del dueño, ver design D-11.
- **Sin dependencias nuevas**: solo se habilitan features del crate `windows` ya presente; `Cargo.lock` y `package-lock.json` no cambian.

## Capabilities

### New Capabilities
- `privilege-context`: detección del usuario del proceso vs el usuario de la sesión interactiva, decisión fail-closed, elevación informada, consulta por IPC, alcance por-usuario bloqueable y restricción del contexto de protección en mismatch.

### Modified Capabilities
- `security-baseline`: "Sin elevación forzada en el entorno de desarrollo" pasa a cubrir solo build de desarrollo y tests; se agrega la elevación obligatoria del ejecutable de release con verificación del manifest embebido.
- `app-shell`: la lista exacta de comandos habilitados pasa de 5 a 6 (`get_privilege_context`); se agrega el aviso global de contexto de privilegios.
- `runtime-config`: el directorio de datos por defecto se define explícitamente como el perfil del usuario del token del proceso, también en over-the-shoulder (reemplaza SU-04).
- `domain-ports`: se agrega el puerto de identidad de la sesión (usuario del proceso, elevación, señales de la sesión interactiva) con su doble de prueba.

## Impact

- **Build**: `src-tauri/build.rs` (manifest `requireAdministrator` solo en el binario de la app y solo en perfil release), `src-tauri/Cargo.toml` (`[[bin]] test = false`; features `Win32_System_Threading`, `Win32_System_RemoteDesktop`, `Win32_UI_WindowsAndMessaging` del crate `windows`), nuevo test `src-tauri/tests/manifest.rs`.
- **Backend**: nuevo `domain/privileges.rs`, nuevo puerto `domain/ports/session.rs` + fake, nuevo adaptador `adapters/windows/privileges.rs` (con `unsafe` mínimo y `// SAFETY:`), nuevo `commands/privileges.rs`, `lib.rs` (cálculo al iniciar + `manage`), `commands/names.rs` y `capabilities/main-window.json` (+1 comando).
- **Frontend**: `src/lib/ipc.ts` (`getPrivilegeContext`), nuevo `PrivilegeProvider` y `PrivilegeBanner`, `Shell.tsx`, catálogo `es.ts`, `mockBackend.ts`, tests Vitest.
- **Docs (tras aprobación)**: KB `09` (DD-16 y SU-04 refutado), `13` §Over-the-shoulder, `04` (ubicación de la base), y `CHANGES.md` (selector de carpeta movido a su primer consumidor).
- **Changes posteriores**: C-08 declara el alcance de cada categoría y construye el contexto de protección con la restricción de privilegios; C-09 re-verifica el alcance al ejecutar; C-18 marca las acciones HKCU como por-usuario; C-13 corre el test de manifest en release en el pipeline; C-22 (autostart por Task Scheduler) hereda la detección.
- **Desarrollo**: `npm run tauri dev` sigue sin elevar; probar UAC y over-the-shoulder requiere el `.exe` de release (smoke manual del dueño).

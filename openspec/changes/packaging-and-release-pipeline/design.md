# Design

> Gobernanza **LOW** en el roadmap (no cambia comportamiento de la app), pero con tres zonas que quedan sujetas al dueño: (1) todo lo que **publica** (push de ramas, PRs, tags, releases), (2) los **permisos y el código de terceros** que corre en el CI de un repo público, y (3) el **instalador** (dónde y cómo se instala un ejecutable que siempre corre elevado, y qué pasa con los datos al desinstalar). Las decisiones marcadas **[DUEÑO]** requieren aprobación explícita antes del apply. Las tareas asumen la opción recomendada; si el dueño elige otra, se actualizan specs y tareas antes de codear (`/opsx:update`).

## Context

Motivación: ver `proposal.md`. Estado actual relevante (observado en el repo, no supuesto):

- **No existe `.github/`**: ningún workflow, ninguna regla de protección conocida. Repo público `FacundoChacon/pc-optimizer`; no hay `LICENSE`.
- **Versiones**: `tauri.conf.json` `0.1.0`, `src-tauri/Cargo.toml` `0.1.0` (`rust-version = "1.95"`, `edition = "2024"`), `package.json` `0.1.0` (`engines.node >= 22.12.0`, `.npmrc` con `save-exact` y `engine-strict`). Nada verifica que coincidan.
- **Bundle actual** (`tauri.conf.json`): `targets: ["nsis", "msi"]`, iconos; sin `bundle.windows`, sin `publisher` (Tauri usa por defecto la segunda parte del identificador, `pcoptimizer`). Identificador `com.pcoptimizer.desktop`.
- **Plantilla NSIS de `@tauri-apps/cli` 2.12.4** (leída del binario instalado en `node_modules`, no supuesta):
  - Página final con `MUI_FINISHPAGE_RUN` **siempre** presente; `RunMainBinary` llama a `nsis_tauri_utils::RunAsUser "$INSTDIR\${MAINBINARYNAME}.exe"`, es decir, lanza la app **des-elevada** como el usuario del shell cuando el instalador está elevado.
  - El desinstalador agrega la casilla "eliminar datos de la aplicación" que, con `SetShellVarContext current`, borra `$APPDATA\${BUNDLEID}` y `$LOCALAPPDATA\${BUNDLEID}` (= `com.pcoptimizer.desktop`). Hay ganchos `NSIS_HOOK_PREINSTALL/POSTINSTALL/PREUNINSTALL/POSTUNINSTALL` y plantilla propia (`bundle.windows.nsis.template`). Modos `currentUser` (por defecto), `perMachine`, `both`. Traducciones `Spanish` y `SpanishInternational` incluidas. `webviewInstallMode`: `downloadBootstrapper` (por defecto), `embedBootstrapper`, `offlineInstaller`, `skip`.
- **Elevación (C-05)**: `build.rs` agrega `/MANIFESTUAC:level='requireAdministrator'` solo al binario de la app y solo con `PROFILE=release`. `tests/manifest.rs` tiene un lector PE propio y el test `app_binary_should_match_build_profile_execution_level` (lee `CARGO_BIN_EXE_pc-optimizer`; espera `requireAdministrator` en release). Nadie corrió ese test en release fuera de la máquina del dueño, ni sobre el `.exe` que produce `tauri build`.
- **Red**: `tests/no_network_crates.rs` audita `cargo metadata --locked --filter-platform x86_64-pc-windows-msvc` contra una denylist de 31 crates. Ya corre dentro de `cargo test`.
- **Datos**: la base vive en `%LOCALAPPDATA%\pc-optimizer` del perfil del token del proceso (spec `runtime-config`, independiente del identificador del bundle). El release ignora `PCOPT_DATA_DIR`: el smoke de un build de release usa el directorio de datos real del dueño.
- **Deudas para C-13**: D-21 de C-07 sin medir en release (debug: 420 s, 1,53 M archivos, pico 127 MB; objetivo orientativo ≤ 5 min y ≤ 200 MB); nota de C-05 sobre el error 740 al terminar el instalador; notas de C-10 y C-11 (vaciado y duplicados nunca en flujos automáticos: este change no agrega ninguno).
- **Respuestas del dueño (2026-11-19)**: (1) no hay máquina con Windows 10: v0.1 se verifica solo en Windows 11 y Windows 10 queda registrado como no verificado; (2) el pipeline crea el tag `v0.1.0` y un release **borrador**; el dueño lo publica a mano.

## Goals / Non-Goals

**Goals:**
- Que ningún cambio llegue a `main` sin que un tercero (el runner) haya corrido los mismos gates que hoy corre el dueño a mano, más auditorías y el empaquetado.
- Que el ejecutable que recibe el usuario sea verificablemente el elevado, instalado donde un proceso sin privilegios no puede reemplazarlo.
- Que publicar sea siempre un acto explícito del dueño, y que todo lo publicado diga la verdad sobre sus limitaciones.

**Non-Goals:**
- Cambiar código de la app (si el smoke encuentra un defecto, se corrige con TDD en este change solo si es de empaquetado; un defecto funcional se anota y se decide con el dueño).
- Firma, auto-update, MSI, Store, Windows 10, atestaciones, bots de actualización de dependencias.
- Optimizar el escáner de carpetas (si D-21 no se cumple, se vuelve al dueño, ver D-21 de este diseño).

## Decisions

### D-01 — Formato del instalador: solo NSIS en v0.1 **[DUEÑO]**

- **A (recomendada)**: `bundle.targets: ["nsis"]`. Un solo artefacto, una sola ruta de smoke en la única máquina disponible, sin descargar WiX en el build. NSIS de Tauri ya trae español, detección de app abierta y casilla de datos.
- B: NSIS + MSI (estado actual de `tauri.conf.json`). El MSI sirve para despliegue corporativo (GPO/Intune), que no es público de v0.1; duplica smoke (instalación, desinstalación, datos) y agrega otro comportamiento al final de la instalación. Publicar un MSI sin smoke contradice "nada se publica sin verificar".
- C: solo MSI. Sin idioma español sencillo ni casilla de datos; rechazada.
- KB 02/08/09 dicen "NSIS/MSI": con A se actualizan a "NSIS (MSI reevaluado más adelante)".

### D-02 — Modo de instalación: por máquina (Program Files) **[DUEÑO]**

- **A (recomendada)**: `bundle.windows.nsis.installMode: "perMachine"`. El instalador pide UAC e instala en `%ProgramFiles%\PC Optimizer\`, con entrada de desinstalación en HKLM. Motivo de seguridad: la app **siempre** corre elevada; si su ejecutable o sus DLL vivieran en una carpeta escribible por el usuario (`%LOCALAPPDATA%\Programs`, lo que hace `currentUser`), cualquier proceso sin privilegios podría reemplazarlos y obtener privilegios de administrador en el próximo UAC que el usuario apruebe por costumbre (escalada clásica). En Program Files solo un administrador puede escribir.
- B: `currentUser` (por defecto de Tauri). Instala sin UAC, pero deja el binario elevado en una carpeta escribible: rechazada por lo anterior.
- C: `both` (el usuario elige). Reabre B; rechazada.
- Consecuencia: desinstalar también pide UAC (coherente con una app que siempre lo pide).

### D-03 — "Ejecutar PC Optimizer" al terminar y error 740 **[DUEÑO]**

Observado en la plantilla 2.12.4: la página final siempre ofrece abrir la app y lo hace con `nsis_tauri_utils::RunAsUser` (des-elevada, como el usuario del shell). Con un ejecutable `requireAdministrator` caben dos resultados, que no se pueden decidir leyendo la plantilla: si `RunAsUser` lanza a través del shell (equivalente a `ShellExecute` en el contexto del Explorador), Windows **pide UAC** y todo funciona; si crea el proceso directamente con el token del usuario, falla con **740** sin mostrar nada.

- **A (recomendada)**: mantener la plantilla por defecto y **verificarlo**: (1) en el apply, leer la fuente de `nsis_tauri_utils` de la versión que descarga el CLI 2.12.4 para anticipar el resultado; (2) en el smoke, terminar la instalación con la casilla marcada y observar. Si aparece UAC y la app abre elevada: se cumple la spec sin tocar nada.
- Contingencia si falla (se vuelve al dueño con la evidencia y se elige):
  - B1 (preferida): plantilla NSIS propia = copia exacta de la de 2.12.4 con **un solo cambio**: quitar la opción de ejecutar de la página final (`MUI_FINISHPAGE_RUN`). Costo: la plantilla queda atada a la versión del CLI; un test compara la versión anotada en la cabecera de la copia con la de `@tauri-apps/cli` en `package.json` para forzar la re-sincronización en cada actualización.
  - B2: misma copia pero con `RunMainBinary` cambiado a `ExecShell "open"` (lanza elevada heredando el UAC del instalador, sin segundo aviso). Más cómodo, pero en over-the-shoulder abre la app como la cuenta administradora: se aceptaría solo si el dueño lo prefiere.
  - C (rechazada): dejar la opción y avisar en las notas "desmarcá Ejecutar". La spec prohíbe una opción que falle en silencio.

### D-04 — Idioma, WebView2 y metadatos del instalador **[DUEÑO]**

- Idioma: `bundle.windows.nsis.languages: ["Spanish"]`, sin selector de idioma. La app solo está en español.
- WebView2: **mantener `downloadBootstrapper`** (por defecto). Windows 11 trae WebView2, así que en el sistema verificado no hay descarga; si faltara (Windows 10 recortado), el **instalador** (no la app) descarga el runtime de Microsoft. Alternativas: `embedBootstrapper` (+~2 MB, igual descarga), `offlineInstaller` (+~130 MB, sin red), `skip` (falla sin WebView2). La regla "sin red" es de la app; se documenta en las notas que el instalador puede descargar WebView2 si falta.
- Metadatos: `bundle.publisher: "PC Optimizer"` (lo que se ve en "Aplicaciones instaladas"; sin datos personales), `shortDescription` y `copyright` en español. Alternativa: el usuario de GitHub del dueño como editor; queda a su elección.

### D-05 — Datos al desinstalar: se conservan **[DUEÑO]**

- **A (recomendada)**: no agregar ganchos. La casilla del desinstalador solo borra `%LOCALAPPDATA%\com.pcoptimizer.desktop` y `%APPDATA%\com.pcoptimizer.desktop` (datos de WebView2 asociados al identificador; a confirmar en el smoke); `%LOCALAPPDATA%\pc-optimizer` (base, configuración, **log de limpiezas**) se conserva siempre. Motivos: el log es el registro de qué se movió a la Papelera (auditoría del usuario); borrarlo sería un borrado permanente fuera de la Papelera que el proyecto solo admite para vaciarla con doble confirmación; y una reinstalación recupera configuración e historial. Las notas y el README dan el comando para borrarlo a mano.
- B: gancho `NSIS_HOOK_POSTUNINSTALL` que borra `$LOCALAPPDATA\pc-optimizer` si la casilla está marcada. Más "limpio", pero es un borrado permanente; además, con el desinstalador elevado por otra cuenta (over-the-shoulder) solo alcanza el perfil de esa cuenta. Rechazada para v0.1.

### D-06 — Verificación del manifest en tres puntos

1. **Release de cargo** (escenario existente): `cargo test --release --locked --test manifest` en el job de empaquetado.
2. **Ejecutable empaquetado**: inmediatamente después de `tauri build`, se **copia** `src-tauri/target/release/pc-optimizer.exe` (el que NSIS empaqueta) a una carpeta temporal del runner, para que ninguna compilación posterior lo pise, y se inspecciona con un test nuevo **ignorado** de `tests/manifest.rs` (`packaged_app_should_require_administrator`) que lee la ruta de `PCOPT_EXE_UNDER_TEST`. Ignorado porque depende de un artefacto externo (regla del proyecto); el workflow lo corre con `--ignored --exact`. Si la variable falta o la ruta no existe, el test **falla** con un mensaje (nunca pasa vacío): la resolución de la ruta es una función pura con tests de tabla.
3. **Ejecutable instalado**: el mismo test, en el smoke, con `PCOPT_EXE_UNDER_TEST` apuntando a `%ProgramFiles%\PC Optimizer\pc-optimizer.exe` (lectura sin elevar).

Sin crates ni herramientas nuevas (`mt.exe`, `sigcheck`): reutiliza el lector PE de C-05.

### D-07 — Versión única y tag

- Se mantienen los tres campos explícitos (`tauri.conf.json`, `Cargo.toml`, `package.json`) más la raíz de `package-lock.json` (que `npm version` actualiza junto con `package.json`). Alternativa descartada: `"version": "../package.json"` en `tauri.conf.json` (Tauri lo admite), porque `Cargo.toml` seguiría aparte y el test igual haría falta.
- Test nuevo `tests/release_config.rs`: funciones puras (`parse_semver`, comparación con lista de fuentes desacordadas) con tests de tabla, más un test sobre los archivos reales; la versión de Cargo se toma de `CARGO_PKG_VERSION`.
- El tag se **deriva** de la versión (`v` + versión de `tauri.conf.json`) en el workflow de release: no hay entrada manual de versión (ni inyección por inputs) y el tag coincide por construcción. El workflow falla antes de construir si el tag ya existe.

### D-08 — Disparadores, runner y jobs del CI **[DUEÑO]**

- `ci.yml` con `on: pull_request` (cualquier base), `push: branches: [main]`, `workflow_dispatch` y `workflow_call` (para que el release reutilice exactamente los mismos gates; entrada booleana `use_cache`). Sin `pull_request_target`, sin `schedule`.
- `concurrency` por workflow + ref con `cancel-in-progress` solo en PRs.
- Runner `windows-2025` (etiqueta fija en vez de `windows-latest`, para que un cambio de imagen no cambie el entorno sin aviso). `timeout-minutes` por job.
- Jobs en paralelo:
  - `rust`: `cargo fmt --check`; clippy `--all-targets --all-features --locked -D warnings`; **primero** el paso con nombre "Auditoría de crates de red" (`cargo test --locked --test no_network_crates`), luego la suite completa `cargo test --locked`.
  - `frontend`: `npm ci`, `npm test`, `npm run build`.
  - `audit`: `cargo audit` (D-10, D-11) y `npm audit` (dos niveles, D-11).
  - `bundle`: `npm ci`, `npm run tauri build -- --ci`, copia del `.exe`, manifest (D-06 puntos 1 y 2), `SHA256SUMS.txt`, `upload-artifact` del instalador y las sumas (retención 7 días).
- Alternativa: `bundle` con `needs: [rust, frontend]` (no empaqueta si algo falla; ahorra minutos pero alarga el tiempo total). Para un repo público los minutos de Windows no cuestan; se prefiere paralelo y el release exige todos los jobs.
- PRs desde forks: GitHub corre con token de solo lectura y sin secretos; la aprobación de contribuyentes nuevos es configuración del repo (del dueño).

### D-09 — Acciones permitidas, fijado por SHA y toolchains **[DUEÑO]**

- Solo acciones de primera parte: `actions/checkout`, `actions/setup-node`, `actions/cache`, `actions/upload-artifact`, `actions/download-artifact`, cada una fijada por **SHA completo** del commit de su última versión estable al momento del apply (resuelto con `git ls-remote` del tag; comentario `# vX.Y.Z` al lado). `persist-credentials: false` en todo checkout.
- Rust: `rustup` ya instalado en el runner; el workflow instala una **versión exacta** de toolchain (la estable que el dueño usa localmente, anotada en el apply 1.1) con `clippy` y `rustfmt`, en vez de `stable` flotante: un clippy nuevo no pone el CI en rojo sin un cambio en el repo, y CI y local ven las mismas advertencias. No se agrega `rust-toolchain.toml` (cambiaría el entorno local del dueño).
- Node: `setup-node` con Node 22 LTS (cumple `engines >= 22.12.0`) y caché de npm.
- `gh` (preinstalado en el runner) para tag y release.
- Rechazadas: `dtolnay/rust-toolchain`, `Swatinem/rust-cache`, `tauri-apps/tauri-action`, `taiki-e/install-action`, `rustsec/audit-check`, `softprops/action-gh-release`: código de terceros con acceso al token, sin ganancia que no se logre con un paso `run`.

### D-10 — `cargo-audit` solo en CI **[DUEÑO]**

- **A (recomendada)**: `cargo install cargo-audit --locked --version <fija>` en el job `audit`, con el binario en caché por versión. No entra en `Cargo.toml` ni en `package.json`; no se instala en la máquina del dueño. En cada ejecución descarga la base de avisos de RustSec (red en el runner, no en la app).
- B: binario precompilado de su release de GitHub verificando la suma: más rápido, más script propio.
- C: acción `rustsec/audit-check`: tercero con token; rechazada (D-09).
- La versión exacta se elige en el apply (última estable; se anota). Archivo de excepciones versionado: `src-tauri/.cargo/audit.toml` (ubicación a confirmar con la documentación de la versión fijada); empieza vacío.

### D-11 — Política de fallos de las auditorías **[DUEÑO]**

- Cargo: falla ante **cualquier** aviso de vulnerabilidad; los informativos (`unmaintained`, `unsound`, `yanked`) se muestran sin fallar (comportamiento por defecto de `cargo audit`). `cargo audit` audita todo `Cargo.lock`, incluidos crates de otras plataformas (p. ej. la pila GTK que Tauri usa en Linux): si un aviso afecta solo a otra plataforma, la excepción se agrega al archivo con el ID, el crate y la justificación ("no está en el grafo de Windows x64; ver `no_network_crates`/`cargo metadata --filter-platform`"), **cada una aprobada por el dueño**. Nunca `--ignore` en el workflow (lo verifica el test de política).
- npm: `npm audit --audit-level=high` (todas las dependencias: el build es parte de la cadena de suministro) y `npm audit --omit=dev --audit-level=moderate` (lo que termina en el bundle: `react`, `react-dom`, `@tauri-apps/api`).
- Consecuencia aceptada: las bases de avisos cambian solas, así que el CI puede ponerse en rojo sin cambios en el repo. Es el objetivo (enterarse); la salida es actualizar la dependencia o una excepción aprobada.
- Línea base: `npm audit` se corre en el apply 1.1 (solo lectura); `cargo audit` se ve en la primera ejecución del CI. Si la línea base ya está en rojo, se **detiene** y se vuelve al dueño con la lista: ninguna excepción se agrega sin su aprobación.

### D-12 — Gate de red: el test existente como paso con nombre (desvío de `cargo tree`)

CHANGES.md dice "verificación de ausencia de crates de red (`cargo tree`)". Se usa el test `no_network_crates` (ya probado, con denylist versionada y filtro por plataforma Windows x64) como paso propio y **antes** de la suite completa, para que el rojo aparezca con su nombre. Un `cargo tree | grep` duplicaría la denylist sin tests y reportaría crates de iOS/Android. Desvío anotado en CHANGES.md al cerrar.

### D-13 — Caché

- `actions/cache` para `~/.cargo/registry/{index,cache}`, `~/.cargo/git/db` y `src-tauri/target`, con clave `SO + toolchain + job + hash(Cargo.lock)` y `restore-keys` por prefijo; `setup-node` con `cache: npm` (clave por `package-lock.json`); `cargo-audit` por versión.
- El release (vía `workflow_call` con `use_cache: false`) **no lee ni escribe caché**: build limpio, sin depender de lo que dejaron otras ejecuciones.
- Riesgo: `target` en Windows puede pasar de 2 GB por job; el límite del repo es 10 GB con desalojo automático. Si molesta, se deja de cachear `target` en `bundle` (anotarlo).

### D-14 — Política de los workflows como test (sin herramientas nuevas)

`src-tauri/tests/ci_policy.rs` lee `.github/workflows/*.yml` del repo (configuración versionada, como `security_baseline` lee `tauri.conf.json`) con análisis **por líneas** (sin crate YAML) y funciones puras con tests de tabla sobre fragmentos literales:
- toda línea `uses:` que no sea local (`./`) termina en `@` + 40 hex;
- hay `permissions:` en columna 0;
- ningún permiso `write` ni `write-all` fuera del job `publish`;
- no aparece `pull_request_target`;
- no aparecen `${{ github.event.` ni `${{ github.head_ref` (inyección en scripts);
- ningún `cargo audit` con `--ignore`;
- cada `actions/checkout` lleva `persist-credentials: false`.

Alternativa: `actionlint` (herramienta nueva, binario externo): no se agrega en v0.1; GitHub valida la sintaxis al correr y la demostración en rojo (D-15) prueba que los workflows se ejecutan.

### D-15 — Demostración del gate en rojo **[DUEÑO]**

- Requiere pushear al repo público (regla: push y PR solo cuando el dueño lo pide). Con autorización explícita:
  1. La rama del change (`feat/packaging-and-release-pipeline`) en verde en su PR.
  2. Rama descartable `ci-gate/failing-test` (desde la del change) con un test nuevo que falla a propósito → **PR borrador** contra la rama del change → anotar ejecución, job `rust`, paso y mensaje.
  3. Rama descartable `ci-gate/network-crate` con `ureq` agregado (`Cargo.lock` actualizado, para que el rojo sea del paso de red y no de `--locked`) → PR borrador → anotar que el paso "Auditoría de crates de red" falla nombrando `ureq`.
  4. Cerrar ambos PRs **sin fusionar** y borrar las ramas remotas y locales.
- Alternativa: disparar CI con `push` en ramas `ci-gate/**` sin abrir PR (menos visible, pero agrega un disparador permanente solo para esto). Se prefieren PRs borrador.

### D-16 — Workflow de release: manual, desde `main`, borrador **[DUEÑO]**

Decidido por el dueño: el pipeline crea el tag `v0.1.0` y un release **borrador**; publica el dueño.

- `release.yml` con solo `on: workflow_dispatch`, sin inputs; `permissions: contents: read` arriba.
  - `preflight`: falla si `github.ref != refs/heads/main`; lee la versión de `tauri.conf.json`; falla si `v<versión>` ya existe o si falta `.github/release-notes/v<versión>.md`.
  - `ci`: `uses: ./.github/workflows/ci.yml` con `use_cache: false` (los mismos gates, sin caché).
  - `publish` (`needs: [preflight, ci]`, único job con `permissions: contents: write`): descarga el artefacto del job `bundle`, crea el tag **por API** apuntando a `GITHUB_SHA` (sin credenciales de git en disco) y luego `gh release create v<versión> --draft --verify-tag --notes-file …` con el instalador y `SHA256SUMS.txt`.
- Nota: un release borrador de GitHub no crea el tag hasta publicarse; por eso el tag se crea explícitamente antes (lo pidió el dueño) y apunta exactamente al commit construido. Un tag creado con el `GITHUB_TOKEN` no dispara otros workflows.
- Quién lo dispara: el **dueño** (Actions → Release → Run workflow, o autoriza al orquestador a correr `gh workflow run release.yml --ref main`). El agente nunca lo dispara por su cuenta ni publica.
- Limitación: `workflow_dispatch` solo existe una vez que el archivo está en `main`, así que el release no se puede ensayar antes de fusionar. Mitigación: los gates y el empaquetado son el mismo `ci.yml` ya probado; lo nuevo es `preflight` + `publish` (pocas líneas, cubiertas por el test de política), y el resultado es un **borrador** reversible (comandos de vuelta atrás en Migration Plan).
- Alternativas: (B) disparar por push de un tag que crea el dueño a mano: el tag existe antes de saber si los gates pasan; (C) `tauri-action`: tercero con permiso de escritura. Rechazadas.
- Bytes publicados vs bytes del smoke: el release **reconstruye** desde `main` sin caché; el smoke completo se hace sobre el artefacto del CI. Por eso, antes de publicar, el dueño hace una verificación corta del borrador (suma, instalar, abrir, desinstalar). Alternativa: promover el artefacto exacto de la ejecución de CI de `main` (requiere `actions: read`, acoplar retención y run-id); no se justifica en v0.1.

### D-17 — Contenido del release **[DUEÑO]**

- Adjuntos: el instalador NSIS y `SHA256SUMS.txt` (formato `sha256sum`: `<hash>  <archivo>`).
- Notas versionadas en `.github/release-notes/v0.1.0.md`, en español, con lo que exige la spec (sin firma y cómo se ve SmartScreen: "Más información" → "Ejecutar de todos modos"; UAC en cada arranque y por qué; Windows 11 verificado y Windows 10 22H2 **no verificado**; sin red ni telemetría; el instalador puede descargar WebView2 si falta; datos en `%LOCALAPPDATA%\pc-optimizer`, que la desinstalación conserva, y cómo borrarlos; cómo verificar la suma con `Get-FileHash`; funciones de v0.1). Un test de `release_config.rs` verifica que el archivo de la versión actual existe y contiene cada punto.
- La suma solo prueba integridad de la descarga (viene del mismo origen), no autenticidad: se dice así en las notas.
- Atestación de procedencia (`actions/attest-build-provenance`, verificable con `gh attestation verify`): **diferida**; buena candidata antes de la firma (PA-09), pero exige `id-token: write` y `attestations: write`.

### D-18 — Protección de `main` **[DUEÑO]**

Recomendación: tras la primera ejecución verde en `main`, que el dueño active un ruleset que exija PR y los checks `rust`, `frontend`, `audit` y `bundle`, y otro que impida borrar o mover tags `v*`. Es configuración del repo: el agente no la toca. No bloquea el apply.

### D-19 — El runner corre elevado

Los runners Windows de GitHub corren como administrador con UAC desactivado y sin sesión interactiva. Ningún test no ignorado conocido depende de correr sin elevar (el manifest se lee, no se ejecuta; el contexto de privilegios usa dobles), pero la primera ejecución lo confirma. Si un test no ignorado falla solo en el runner por el entorno: análisis de causa raíz; se corrige el test para que no dependa del entorno o, con aprobación del dueño, se marca `#[ignore = "motivo"]`. Nunca se debilita una aserción.

### D-20 — Windows 10 no verificado (decidido por el dueño, 2026-11-19)

v0.1 se verifica solo en Windows 11 (la máquina del dueño). Windows 10 22H2 queda **no verificado**: se dice en las notas y el README; SU-05 y PA-08 siguen abiertas con la nota "v0.1 sin verificar en Windows 10 por falta de equipo". El instalador no bloquea Windows 10 (no se agrega chequeo de versión).

### D-21 — Medición de D-21 (C-07) en release **[DUEÑO]**

- Dónde: en el smoke, con la app **instalada** (release, elevada) y autorización explícita para un análisis de solo lectura de `C:` completo. Como el release ignora `PCOPT_DATA_DIR`, el resultado se guarda en el directorio de datos real; por eso el paso 1 del smoke ofrece apartarlo antes (ver D-22).
- Qué: duración (la que muestre la app si la expone; si no, cronómetro desde "Analizar" hasta el resultado), archivos recorridos, memoria pico (Administrador de tareas, columna de pico del espacio de trabajo, leída por el dueño; o `Get-Counter '\Process(pc-optimizer)\Working Set Peak'` si se puede leer sin elevar). Objetivo orientativo: ≤ 5 min y ≤ 200 MB.
- Si no se cumple: se detiene y se vuelve al dueño con los números. Recomendación: si la duración es ≤ 1,5 × el objetivo (7,5 min) y la memoria ≤ 200 MB, publicar con la limitación en las notas y abrir un seguimiento con las opciones de C-07 (`FindFirstFileExW` + `FIND_FIRST_EX_LARGE_FETCH` o listado paralelo); si es peor, el release queda en borrador hasta ese seguimiento. Optimizar el escáner no se hace en C-13.

### D-22 — Smoke guiado del instalador (manual, con el dueño) **[DUEÑO]**

Se escribe `smoke-guide.md` en el apply (patrón de C-06/C-07). Reglas: un paso por vez y el dueño confirma; **aviso y cuenta atrás real de 10 s** (`for ($i=10; $i -gt 0; $i--) { Write-Host $i; Start-Sleep 1 }`) antes de cada lanzamiento de UI o UAC (instalador, app, desinstalador); **sin capturas** (verificación por comandos de solo lectura y lo que el dueño ve); terminal del agente NO elevada; los comandos que borran o restauran algo los corre **el dueño** con el texto exacto que se le entrega; ante cualquier diferencia se detiene y se anota.

Pasos (Windows 11):
1. Preparación: el dueño descarga el instalador del artefacto del CI verde **con el navegador** (para que tenga la marca de internet y SmartScreen actúe como para un usuario real) a una carpeta que elija; el agente verifica la suma contra `SHA256SUMS.txt`. Se comprueba que no hay instalación previa y si existe `%LOCALAPPDATA%\pc-optimizer`; si existe, se entrega al dueño el comando para renombrarlo a `pc-optimizer.bak-c13`. El agente crea un fixture pequeño de duplicados en `%TEMP%\pcopt-smoke-c13\`.
2. Instalar (cuenta atrás): SmartScreen (texto y pasos coinciden con las notas), UAC del instalador con "Editor desconocido", pantallas en español, carpeta en Program Files. En la página final, dejar marcada "Ejecutar" y observar (D-03): UAC y app elevada, o fallo/nada.
3. Sin UI: ruta instalada, entrada de desinstalación en HKLM, acceso directo del menú Inicio, permisos de la carpeta (`icacls`: usuarios sin escritura), manifest del ejecutable instalado (test ignorado de D-06 con la ruta instalada).
4. Abrir desde el menú Inicio y **cancelar** el UAC (cuenta atrás): no queda ningún proceso `pc-optimizer`.
5. Abrir aprobando el UAC (cuenta atrás): Inicio muestra `0.1.0`; aviso de privilegios coherente con la cuenta; alternar simple/avanzado.
6. Red: con la app abierta, `Get-NetTCPConnection` y `Get-NetUDPEndpoint` filtrados por el PID de la app → vacíos.
7. Recorrido v0.1: dashboard (C-12), Monitor, análisis de basura y preview (confirmar limpieza solo con autorización), duplicados sobre el fixture y limpieza del fixture a la Papelera, log de limpieza, Papelera (solo ver tamaño; vaciar solo con autorización explícita), Configuración persiste tras cerrar y abrir.
8. D-21 (con autorización): análisis de `C:` completo; anotar duración, archivos y memoria pico.
9. Desinstalar desde Configuración → Aplicaciones (cuenta atrás, UAC) marcando la casilla de datos: Program Files, accesos directos y entrada borrados; `%LOCALAPPDATA%\com.pcoptimizer.desktop` borrado; `%LOCALAPPDATA%\pc-optimizer` **conservado**.
10. Cierre: el dueño corre los comandos entregados para borrar `%TEMP%\pcopt-smoke-c13\`, el directorio de datos del smoke y restaurar `pc-optimizer.bak-c13` si lo apartó; los archivos del fixture que quedaron en la Papelera los elimina o restaura él.

Verificación corta del borrador (después del release, antes de publicar): descargar el adjunto del borrador, comparar la suma, instalar, abrir (UAC), ver `0.1.0` y desinstalar.

### D-23 — Tests

- `tests/release_config.rs` (nuevo): versión semántica y sincronía de `tauri.conf.json`, `package.json`, `package-lock.json` (raíz y `packages[""]`) y `CARGO_PKG_VERSION`; configuración del bundle (`targets == ["nsis"]`, `installMode == "perMachine"`, `languages == ["Spanish"]`, `publisher` presente); notas del release de la versión actual con todos los puntos de D-17. Funciones puras + tests de tabla (RED con fuentes desacordadas, versión no semántica, nota sin un punto).
- `tests/ci_policy.rs` (nuevo): D-14, tabla de fragmentos buenos y malos por regla + test sobre los workflows reales.
- `tests/manifest.rs`: resolución pura de `PCOPT_EXE_UNDER_TEST` (ausente, inexistente, válida) + test ignorado `packaged_app_should_require_administrator`.
- Si D-03 termina en B1: test que compara la versión anotada en la plantilla NSIS propia con la de `@tauri-apps/cli`.
- `security_baseline` sigue verde sin cambios (CSP, capabilities y `withGlobalTauri` intactos).
- Sin tests de Vitest nuevos (no cambia el frontend).

### D-24 — Archivos

```
.github/
  workflows/ci.yml            # gates + bundle; reutilizable (workflow_call)
  workflows/release.yml       # workflow_dispatch: preflight → ci → publish (borrador)
  release-notes/v0.1.0.md
src-tauri/
  .cargo/audit.toml           # excepciones de cargo-audit (vacío o justificadas)
  tauri.conf.json             # bundle: nsis, perMachine, Spanish, publisher
  tests/ci_policy.rs          # nuevo
  tests/release_config.rs     # nuevo
  tests/manifest.rs           # + ejecutable empaquetado/instalado
README.md                     # instalación, SmartScreen, UAC, datos, desinstalación
```

### D-25 — Orden y ramas

- Rama `feat/packaging-and-release-pipeline` desde `main` **con C-12 ya fusionado** (si C-12 no está fusionado al empezar, partir de `main` y rebasar antes del smoke: el CI no depende de C-12, el smoke de v0.1 sí).
- Partes: **1** tests y configuración (local, TDD); **2** workflows y notas (local); **3** push, CI verde y demostración en rojo (autorización del dueño); **4** smoke con el artefacto del CI; **5** PR y merge (dueño), release borrador (dueño), verificación corta, publicación (dueño), cierre.

## Risks / Trade-offs

- [El botón final del instalador falla con 740] → verificación en el smoke y contingencia B1 (D-03).
- [Instalador sin firma: SmartScreen y desconfianza] → aceptado por DD-11; notas con los pasos y la suma SHA-256.
- [Windows 10 sin verificar] → dicho en notas y README; PA-08/SU-05 abiertas.
- [Las bases de avisos ponen el CI en rojo sin cambios] → intencional; excepciones solo versionadas, justificadas y aprobadas (D-11).
- [Avisos de crates solo de Linux en `Cargo.lock`] → excepción por ID con justificación de plataforma, aprobada por el dueño.
- [El release no se puede ensayar antes de `main`] → gates compartidos con `ci.yml`, test de política, resultado en borrador y vuelta atrás documentada.
- [Bytes publicados ≠ bytes del smoke completo] → verificación corta del borrador antes de publicar (D-16).
- [Las SHA de acciones envejecen] → actualización manual con el mismo test de política; bots de actualización fuera de alcance.
- [El empaquetador descarga NSIS y utilidades de GitHub durante el build] → red solo en el runner; si falla, se reintenta la ejecución.
- [El runner corre elevado y sin sesión] → D-19.
- [Caché grande o desalojada] → solo afecta tiempos (D-13).
- [Un test fallido a propósito o `ureq` llegan a `main`] → solo en ramas `ci-gate/*` con PR borrador contra la rama del change, cerrados sin fusionar y borrados (D-15); `git log main` se revisa al cerrar.
- [D-21 no se cumple en release] → se vuelve al dueño (D-21 de este diseño).
- [Sin `LICENSE` en un repo público con binarios] → pregunta abierta para el dueño antes de publicar (no cambia este diseño).

## Migration Plan

1. Implementar partes 1–2 en local; partes 3–5 con autorización del dueño en cada paso que publica.
2. Primer release: el dueño dispara `release.yml` desde `main` → borrador `v0.1.0` → verificación corta → el dueño publica.
3. Vuelta atrás (la ejecuta el dueño): `gh release delete v0.1.0 --yes` y `git push origin --delete v0.1.0` (borrador y tag); desactivar un workflow con `gh workflow disable <archivo>`; revertir `tauri.conf.json` restaura el bundle anterior.

## Decisiones del dueño (bloquean el apply)

| ID | Decisión | Recomendación |
|---|---|---|
| D-01 | Formato | Solo NSIS en v0.1; MSI fuera |
| D-02 | Modo de instalación | Por máquina (Program Files, instalador con UAC) |
| D-03 | "Ejecutar" al terminar / 740 | Plantilla por defecto + verificación; si falla, plantilla propia sin la opción (B1) |
| D-04 | Idioma, WebView2, metadatos | Español; `downloadBootstrapper`; editor "PC Optimizer" |
| D-05 | Datos al desinstalar | Se conservan siempre; borrado manual documentado |
| D-08 | Disparadores y jobs | PR + push a `main` + manual + `workflow_call`; `windows-2025`; 4 jobs en paralelo |
| D-09 | Acciones y toolchains | Solo acciones de GitHub fijadas por SHA; Rust exacto vía `rustup`; Node 22; `gh` |
| D-10 | `cargo-audit` | `cargo install --locked --version` fija, solo en CI, en caché |
| D-11 | Política de auditoría | Cargo: toda vulnerabilidad; npm: alta en todo, moderada en producción; excepciones versionadas y aprobadas |
| D-15 | Demostración en rojo | Dos PRs borrador desde `ci-gate/*`, cerrados sin fusionar y borrados |
| D-16 | Release | `workflow_dispatch` desde `main`: preflight → mismos gates sin caché → tag por API + borrador; lo dispara el dueño |
| D-17 | Contenido | Instalador + `SHA256SUMS.txt` + notas versionadas; atestación diferida |
| D-18 | Protección de `main` | Ruleset con checks requeridos y tags `v*` protegidos (lo configura el dueño; no bloquea) |
| D-21 | Medición D-21 | En el smoke con la app instalada; contingencia con umbral 1,5× |
| D-22 | Smoke | 10 pasos en Windows 11, cuenta atrás de 10 s, sin capturas, limpieza por el dueño |

## Desvíos respecto de CHANGES.md / KB

- "Bundle NSIS/MSI" → solo NSIS en v0.1 (D-01).
- "Verificación de ausencia de crates de red (`cargo tree`)" → test `no_network_crates` como paso con nombre (D-12).
- "Verificación manual guiada en Windows 10 22H2 y 11" → solo Windows 11; Windows 10 no verificado (D-20, decidido por el dueño).
- "Release en GitHub (tag v0.1.0)" → tag + release **borrador**; publica el dueño (D-16, decidido por el dueño).
- KB 02 dice que el aviso de SmartScreen se documenta "en README": se documenta en README **y** en las notas del release.

## Preguntas abiertas de KB 10 que toca este change

- **PA-08** (profundidad de Windows 10): sigue abierta; se agrega la nota de v0.1 sin verificar.
- **PA-09** (firma): sigue abierta; la atestación queda como paso intermedio sugerido.
- **PA-11** (auto-update): sin cambios; no hay mecanismo de actualización.

## Open Questions

- **Licencia**: el repo público no tiene `LICENSE`; sin ella, los binarios publicados quedan "todos los derechos reservados". Conviene que el dueño la decida antes de **publicar** el borrador; no cambia specs, diseño ni tareas.
- Versión exacta de `cargo-audit` y SHA de cada acción: se fijan en el apply con la última estable de ese momento.

## Apply notes

<!-- Se completa durante el apply: línea base, evidencia RED, ejecuciones de CI (URLs), demostración en rojo, smoke y mediciones. -->

### Apply 1 (grupos 1 a 7, local) — 2026-11-19

Contexto: un apply anterior se interrumpió dejando trabajo sin commitear y sin anotar. Este apply lo revisó pieza por pieza, completó lo que faltaba (4.3, 5.x, 6.x, 7.x) y reprodujo RED donde fue honesto y barato. Donde no, queda escrito "RED no observado".

#### 1.1 Línea base (tomada sobre el árbol con trabajo parcial: desvío; no hay línea base previa pura)

| Comando | Resultado |
|---|---|
| `npm test` | 54 archivos, 838 tests verdes |
| `npm run build` | verde |
| `cargo test --locked` | lib 1198 ok / 25 ignorados; `architecture` 24; `manifest` 5 preexistentes (9 con los 4 nuevos de 3.1; 1 ignorado nuevo de 3.2); `no_network_crates` 9; `security_baseline` 22 (`ci_policy` y `release_config` del apply anterior ya presentes) |
| `cargo fmt --check` / `cargo clippy --all-targets --all-features --locked -D warnings` | verdes |
| `rustc --version` | rustc 1.100.0 (0ef1bc386 2026-11-09), fijada en CI como `RUST_TOOLCHAIN` |
| `node --version` | v24.22.1 local (CI usa Node 22, cumple `engines >= 22.12.0`) |
| `npm audit --audit-level=high` y `npm audit --omit=dev --audit-level=moderate` | 0 vulnerabilidades en ambos |

#### 1.2 Hallazgos de fuente (solo lectura)

| Tema | Fuente | Hallazgo |
|---|---|---|
| CLI | `node_modules/@tauri-apps/cli` | 2.12.4 |
| Plantilla NSIS generada | `src-tauri/target/release/nsis/x64/installer.nsi` | `INSTALLMODE perMachine`, `MULTIUSER_EXECUTIONLEVEL Highest`, `INSTALLWEBVIEW2MODE downloadBootstrapper` (por defecto), `DISPLAYLANGUAGESELECTOR false`, idioma Spanish (`Spanish.nsh`), `COPYRIGHT` y `MANUFACTURER` tomados de `bundle` |
| Nombre del instalador | build real | `PC Optimizer_0.1.0_x64-setup.exe` (con ESPACIO). GitHub cambia el espacio por un punto al adjuntar; el workflow lo copia como `PC-Optimizer_<versión>_x64-setup.exe` antes de calcular `SHA256SUMS.txt` |
| `RunAsUser` (D-03) | `nsis_tauri_utils.dll` que descarga el CLI (`%LOCALAPPDATA%\tauri\NSIS\Plugins\x86-unicode\additional`), importaciones y símbolos | Usa `GetShellWindow` + `OpenProcessToken` + `DuplicateTokenEx` + `CreateProcessWithTokenW` (crea el proceso con el token del shell, SIN pasar por el shell) y también importa `ShellExecuteW`. La plantilla lo usa en el botón "Ejecutar" final (`RunMainBinary`). Con un manifest `requireAdministrator`, `CreateProcessWithTokenW` con token no elevado suele devolver 740 (`ERROR_ELEVATION_REQUIRED`). **D-03 probablemente termine en B1** (plantilla propia sin la opción "Ejecutar"). El análisis estático no confirma si `ShellExecuteW` es un respaldo; se decide en el smoke 9.2, paso 2 |
| `cargo-audit` | crates.io (`max_stable_version`) | 0.23.0 (fijada en `CARGO_AUDIT_VERSION`). Lee `.cargo/audit.toml` relativo al directorio de ejecución; el job corre `cargo audit` con `working-directory: src-tauri`, donde están `Cargo.lock` y `.cargo/audit.toml` |
| `actions/checkout` | `git ls-remote --tags` (etiqueta ligera, sin `^{}`: el SHA es el del commit) | v7.0.1 = `3d3c42e5aac5ba805825da76410c181273ba90b1` |
| `actions/setup-node` | idem | v7.1.0 = `949feb2413d6458794dcd2491c4babbbce0c15c1` |
| `actions/cache` | idem | v6.1.0 = `55cc8345863c7cc4c66a329aec7e433d2d1c52a9` |
| `actions/upload-artifact` | idem | v7.0.2 = `cf430e030ddbb5b0abf93d22962f4752f3646cd9` |
| `actions/download-artifact` | idem | v8.0.2 = `9000827ccba6bdab643e8b6fd33ac0654aef8333` |

Aviso para el orquestador: D-03 probable B1 (ver `RunAsUser`). No bloquea los grupos 1 a 7.

#### 1.3 Decisiones registradas

Diff de `CHANGES.md` y `knowledge-base/{02,08,09,10}` revisado: C-13 en curso con D-01/D-02/D-12/D-16/D-20; KB 02 distribución NSIS por máquina, borrador, SmartScreen en README y notas; KB 08 CI/Build con jobs, gates y release; KB 09 DD-11 con detalles y SU-05 "v0.1 sin verificar en Windows 10"; KB 10 PA-08 abierta con la nota. `git diff --stat` solo lista esos cinco archivos.

#### Evidencia por tarea (RED / GREEN)

- **2.1 y 2.2** (`release_config.rs`, versión): RED no observado en este apply (código escrito por la ejecución interrumpida). GREEN actual: `--test release_config` verde; cubre `parse_semver` (válidas e inválidas), dos fuentes desacordadas a la vez, las cuatro iguales, lectura sobre `tempdir` nombrando `package.json`, lock sin `packages[""]` y el repo real (todo 0.1.0). No se editaron los archivos reales de versión.
- **2.3** (bundle): RED **reproducido** con el `tauri.conf.json` de `HEAD` restaurado temporalmente (se repuso después; `git diff` confirma la sección `bundle` nueva):
  `cargo test --manifest-path src-tauri/Cargo.toml --locked --test release_config real_tauri_conf` ->
  `real_tauri_conf_should_ship_only_the_nsis_per_machine_spanish_installer ... FAILED` con
  `left: ["bundle.targets debe ser exactamente [\"nsis\"]", "bundle.windows.nsis.installMode debe ser \"perMachine\"", "bundle.windows.nsis.languages debe ser [\"Spanish\"]", "bundle.publisher debe ser un texto no vacio"]`.
  GREEN con la configuración actual; TRIANGULATE: tabla con `currentUser`, `["nsis","msi"]` y config vacía. `--test security_baseline` verde (22).
- **2.4** `npm run tauri build` (terminal no elevada): un solo instalador `src-tauri/target/release/bundle/nsis/PC Optimizer_0.1.0_x64-setup.exe`, 3 290 350 bytes (3,14 MiB); no existe `bundle/msi`. No se ejecutó.
- **3.1** (`resolve_exe_under_test`): RED no observado (ejecución interrumpida). GREEN: ausente/vacía/blancos, inexistente en `tempdir`, carpeta, archivo existente (4 tests).
- **3.2** (`packaged_app_should_require_administrator`, `#[ignore]`): RED **observado**: (a) sin la variable -> `falta PCOPT_EXE_UNDER_TEST: indica la ruta del ejecutable a inspeccionar`; (b) con la variable apuntando al binario de test `asInvoker` -> `left: Some("asInvoker") right: Some("requireAdministrator")`. GREEN con una copia (carpeta temporal) de `target/release/pc-optimizer.exe` -> `test packaged_app_should_require_administrator ... ok`. `--test manifest` sin `--ignored`: 9 ok / 1 ignorado.
- **3.3** `--release --test manifest`: 9 ok / 1 ignorado.
- **4.1, 4.2** (`ci_policy.rs`: SHA, permisos, `pull_request_target`): RED no observado (ejecución interrumpida). GREEN con tablas de rechazo/aceptación (tags, 39/41 caracteres, mayúsculas, `docker://`, locales `./`, comentario `# vX`, `permissions` en columna 0 frente a dentro de un job, `contents: write` en `publish`, `write-all`, `pull_request_target` en comentario y `pull_request`).
- **4.3** (expresiones no confiables, excepciones de auditoría, `persist-credentials`): RED **observado**: `cargo test ... --test ci_policy` -> `error[E0425]: cannot find function untrusted_expressions in this scope` (y `audit_exceptions`, `checkout_with_persisted_credentials`). GREEN. TRIANGULATE (el workflow real destapó un falso positivo): `--ignored` del arnés de tests se tomaba por `--ignore`; se agregó el caso (aceptado `-- --ignored --exact`, rechazado `--ignore=RUSTSEC`) y se corrigió con comparación por palabra.
- **5.1** (workflows reales): RED **observado** sin `.github/`: `no se pudo leer ...\.github\workflows: El sistema no puede encontrar la ruta especificada`. GREEN: `ci.yml` creado; `real_workflows_should_satisfy_every_policy_rule` y `ci_workflow_should_declare_the_four_jobs_on_a_pinned_windows_runner` verdes. El job `rust` crea una carpeta `dist` vacía (`tauri::generate_context!` exige que exista `frontendDist`); verificado localmente: clippy compila con `dist` vacía.
- **5.2** (`src-tauri/.cargo/audit.toml`): RED **observado** (`no se pudo leer ...\.cargo\audit.toml`); GREEN con el archivo sin excepciones (`ignore = []`) y el comentario que exige ID, crate, justificación y aprobación del dueño. Revisión del YAML contra D-10/D-11: `cargo install --locked --version 0.23.0` con caché del binario por versión, `cargo audit` sin `--ignore`, `npm audit` en los dos niveles de D-11.
- **5.3** Ensayo local de los pasos (terminal no elevada, sin `cargo-audit`): fmt ok; clippy ok (también con `dist` vacía); `--test no_network_crates` 9 ok; suite completa ok; `npm test` 838 ok; `npm run build` ok; `npm run tauri build -- --ci` ok; copia del `.exe`, `--release --test manifest` 9 ok / 1 ignorado y el test ignorado sobre la copia ok; `sha256sum` -> `1f885d3652c22e0cda857de65ad799a9b59f1a8208eb3b20702f44fceb00e743  PC-Optimizer_0.1.0_x64-setup.exe`, igual a `Get-FileHash` (mayúsculas). `package-lock.json` y `Cargo.lock` sin cambios. Desvío: NO se ejecutó `npm ci` (el clasificador de permisos denegó el comando que lo incluía); `node_modules` ya coincide con el lock porque `npm test` y `npm run build` pasan; `npm ci` queda para el CI. Se agregó `release-assets/` a `.gitignore` (salida local del ensayo).
- **6.1** (notas): RED **observado**: `error[E0425]: cannot find function missing_note_points`. GREEN: `.github/release-notes/v0.1.0.md` cubre los 10 puntos (SmartScreen sin firma y pasos, UAC, Windows 11, Windows 10 no verificado, sin red ni telemetría, WebView2, carpeta de datos, borrado, suma). TRIANGULATE: cada punto removido se nombra como faltante; nota vacía -> 10 faltantes. Desvío de texto: se usa "Ejecutar de todas formas" (texto de Windows 11 en español; D-17 decía "todos modos"); se confirma en el smoke (9.4).
- **6.2** (`release.yml`): RED **observado**: `falta .github/workflows/release.yml`. GREEN: solo `workflow_dispatch`; `preflight` (ref `refs/heads/main`, versión desde `tauri.conf.json`, tag inexistente por API, notas presentes), `ci` vía `uses: ./.github/workflows/ci.yml` con `use_cache: "false"`, `publish` con `needs: [preflight, ci]` y único `contents: write`, tag por API sobre `GITHUB_SHA`, `gh release create --draft --verify-tag --notes-file`. Suma con `sha256sum --text` (formato `hash  archivo` de D-17). Revisión contra los escenarios: "Gates en verde" (tag + borrador con instalador, sumas y notas, sin publicar), "Gate en rojo" (`publish` depende de `ci`; sin gate verde no hay tag), "Disparo desde otra rama" (primer paso de `preflight` falla antes de construir), "Tag existente" (preflight falla antes de `ci`), "Tag nuevo". No ejecutable antes de estar en `main`.
- **7.1** README: secciones de instalación, SmartScreen, UAC, sistemas, desinstalación/datos, CI y licencia, coherentes punto por punto con `v0.1.0.md`; el único enlace externo es el propio repositorio. Nota: la sección "Elevación" del README (preexistente) sigue diciendo `asInvoker`; está desactualizada desde C-05 y no se tocó (fuera de alcance).
- **7.2** Verificación integral: fmt ok, clippy ok, `cargo test --locked`: lib 1198 / 25 ign (igual a línea base), architecture 24, ci_policy 16, manifest 9 / 1 ign, no_network_crates 9, release_config 16, security_baseline 22; Vitest 838 (igual a línea base); `npm run build` ok; `openspec validate packaging-and-release-pipeline --strict` válido; `package-lock.json` y `Cargo.lock` sin cambios.

Pendiente (requiere al dueño): grupos 8 a 10.

#### Correcciones de la revisión independiente

Seis hallazgos corregidos tras la revisión independiente (el dueño decidió NO agregar `cargo audit --deny warnings`; la severidad del paso de auditoría queda igual). Conteos de `ci_policy`: 16 -> 27 tests; `release_config` 16 (sin cambios). Archivos: `src-tauri/tests/ci_policy.rs`, `.github/workflows/ci.yml`, `.github/workflows/release.yml`, `README.md`. Los YAML se verifican con los tests de `ci_policy` sobre los workflows reales más lectura cuidadosa.

1. **Solo se escaneaba `*.yml`.** `policy_files(root)` acepta `.yml` y `.yaml` en `.github/workflows` y los `action.yml|yaml` de `.github/actions/**` (la carpeta puede no existir; `workflows/` sigue exigiéndose no vacía). RED: `error[E0425]: cannot find function policy_files` / `all_violations`. Tests con `tempdir`: extensiones, acciones compuestas anidadas, sin `actions/`, y una violación en `.yaml` y en `action.yml` se reporta.
2. **Reglas eludibles con sintaxis YAML alternativa.**
   - a. `write_permissions_outside_publish` normaliza comillas y llaves (`permissions: { contents: write }`, `"write"`, `'write'`, `"write-all"`, claves entre comillas); las claves `run`/`name`/`if` no cuentan. `contents: write` en `publish` sigue aceptado; `write-all` se rechaza en todos lados. RED: `debia rechazar "      permissions: { contents: write }"` y `debia aceptar rust: "      - name: write"`.
   - b. `untrusted_expressions` pasó a lista de permitidos dentro de `run:` (línea, bloque `|`/`>` con modificadores, continuación de escalar plano; los comentarios del script también se miran porque GitHub los expande). Permitidas: `runner.temp`, `runner.os`, `github.sha`, `github.workspace`, `github.run_id`, `github.run_number`; todo lo demás (`github.ref_name`, `head_ref`, `event.*`, `inputs.*`, `steps.*.outputs.*`, `needs.*.outputs.*`, `secrets.*`, `env.*`, combinaciones con `||`, `${{` sin cerrar) se rechaza. `if:`, `with:` y `env:` siguen libres. RED: `debia rechazar "      - run: echo ${{ github.ref_name }}"` y bloque multilínea `left: [] right: [8, 9]`. TRIANGULATE: tabla de rechazo (15) y de aceptación (13), bloque con línea en blanco, comentario interno, fin de bloque por dedentación y los seis marcadores de bloque. Los workflows reales ya cumplían (todo va por `env:`), sin cambios en ellos.
3. **Caché de `target/` dejaba instaladores viejos.** El paso "Instalador NSIS" hace `rm -rf src-tauri/target/release/bundle` antes de `tauri build`; se mantiene la comprobación de un único instalador. Test `bundle_job_should_remove_stale_installers_before_building` (RED: `el job bundle debe borrar src-tauri/target/release/bundle`).
4. **README "Elevación" desactualizada.** Ahora dice: release embebe `requireAdministrator` (aviso de UAC en cada arranque), desarrollo y tests usan `asInvoker` (confirmado en `build.rs::emit_app_elevation`).
5. **Preflight tomaba cualquier error de `gh api` por "tag inexistente".** Se captura salida y estado: éxito = el tag existe (falla); solo `HTTP 404` = libre; cualquier otro error falla con mensaje. Test `release_preflight_should_tell_a_free_tag_from_an_api_failure` (RED: `solo un 404 significa tag libre`).
6. **Tag huérfano si falla `gh release create`.** El paso del tag tiene `id: tag` y se agregó, dentro de `publish`, el paso final `if: failure() && steps.tag.outcome == 'success'` que borra `refs/tags/v$VERSION` con `gh api -X DELETE` (valores por `env:`). Test `release_publish_should_delete_the_tag_it_created_when_a_later_step_fails` (RED: `el paso del tag necesita id`). Límite conocido: si `gh release create` llega a crear el borrador y falla al subir un adjunto, el borrador queda sin tag; lo borra el dueño (`gh release delete`).

Compuerta final (terminal no elevada): `cargo fmt --check` ok; clippy `-D warnings` ok; `cargo test --locked`: lib 1198 / 25 ign, architecture 24, ci_policy 27, manifest 9 / 1 ign, no_network_crates 9, release_config 16, security_baseline 22; Vitest 838; `openspec validate packaging-and-release-pipeline --strict` válido; `Cargo.lock` y `package-lock.json` sin cambios.

### Primera ejecución del CI (8.1) — 2026-11-19

Run: https://github.com/FacundoChacon/pc-optimizer/actions/runs/44565882974 (PR #13). Resultado por job: `audit` pass (5m14s), `bundle` pass (8m29s), `rust` fail (6m29s), `frontend` fail (1m10s).

**Causa raíz 1 (rust).** Los runners de Windows tienen `core.autocrlf=true`: los `.github/workflows/*.yml` se bajan con CRLF y 4 tests de `ci_policy.rs` buscan subcadenas como `"\n  {job}:\n"` (`bundle_job_should_remove_stale_installers_before_building`, `ci_workflow_should_declare_the_four_jobs_on_a_pinned_windows_runner`, `release_publish_should_delete_the_tag_it_created_when_a_later_step_fails`, `release_workflow_should_be_manual_draft_only_and_the_only_one_with_write_access`). En local los archivos están en LF y pasaban.
- Decisión del dueño: normalizar dentro del test; sin `.gitattributes`.
- Fix: helper único `src-tauri/tests/support/text.rs` (`normalize_newlines` / `read_text`, `\r\n` a `\n`), incluido con `#[path]` en `ci_policy.rs` y `release_config.rs`; todas las lecturas de archivos del repo en esos tests pasan por él (`policy_files`, `audit.toml`, JSON y notas de release).
- RED: 3 tests nuevos; con el helper creado pero sin usarlo en `policy_files`, `policy_files_should_read_a_crlf_workflow_exactly_like_its_lf_version` y `policy_files_should_normalize_mixed_line_endings_and_leave_lf_untouched` fallan (2 failed, 1 passed); GREEN al usar `read_text`. Triangulación: CRLF puro, mezcla de finales y LF intacto. Verificación adicional: con copias CRLF temporales de `.github/` (no commiteadas, restauradas con `git checkout`), `ci_policy` (30) y `release_config` (16) pasan.

**Causa raíz 2 (frontend).** `CleanupPreviewDialog.tsx` cierra el `<dialog>` nativo en un `useEffect` pasivo, mientras el `<p role="alert">` se renderiza en el mismo commit; `findByRole("alert")` puede resolverse antes del efecto en un runner lento y el `queryByRole("alertdialog")` síncrono aún ve el diálogo abierto (falló el caso `nothing_to_clean` de `cleanupFlow.test.tsx`).
- Decisión del dueño: corregir solo el test, sin tocar producción.
- Fix: `await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull())` en el `it.each` y en los otros 3 puntos con la misma carrera (diálogo abierto antes, cierre asertado justo tras un `await` que no espera el cierre): `cleanupFlow.test.tsx` (tras confirmar, ~l.219) y `emptyRecycleBin.test.tsx` (~l.267 y ~l.437). Los demás asserts síncronos verifican un diálogo que nunca se abrió y quedan igual.
- RED: no se puede forzar de forma fiable (es una carrera de tiempos del runner); es un fix de test sin RED reproducible. Verificación: `cleanupFlow.test.tsx` 6 corridas seguidas verdes, carpeta `cleanup` y `npm test` completo repetidos en verde.

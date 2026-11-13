# Design

## Context

Motivación en `proposal.md`; requisitos en `specs/{app-shell,local-store,settings,ui-modes}/spec.md`.

Estado actual (C-01, C-02 y C-03 archivados):

- **Frontend**: `src/app/App.tsx` es una única pantalla que llama `getAppInfo()` y muestra título + versión; `src/lib/ipc.ts` tiene `call<T>(command)` sin argumentos, `toAppError` y `getAppInfo`; `src/shared/i18n/es.ts` es un objeto `as const` con 4 strings; `src/styles.css` define 5 tokens Tailwind 4 (`@theme`) y `color-scheme: dark`; `src/features/` está vacío. Tests en `src/tests/**` (Vitest + jsdom + Testing Library, `mockIPC` de `@tauri-apps/api/mocks`). `tsconfig.app.json` es `strict` + `noUncheckedIndexedAccess` + `exactOptionalPropertyTypes` con `types: ["vite/client"]` (sin tipos de Node en `src/`).
- **Backend**: `lib.rs::app_builder` registra solo `get_app_info`; `manage_store` hace `app.manage(open_store_state(..))` con un `StoreState` **inmutable** (`Ready(Arc<SqliteStore>)` | `Unavailable { error, db_path }`). `recovery.rs::backup_and_recreate(db_path, &impl Clock) -> Result<(PathBuf, SqliteStore), StoreError>` ya existe y está probado, pero nada lo invoca. `error.rs::AppError` serializa `{ kind, message }`; `NotFound`/`InvalidState`/`Constraint` caen en `internal` ("C-04 los tipará al exponer comandos"). `SettingsStore` guarda pares clave → `serde_json::Value`; seed `ui_mode="simple"`, `autostart_tray=false`, `language="es"` (`DEFAULT_SETTINGS`).
- **Seguridad**: `commands/names.rs::APP_COMMANDS` es la fuente única que `build.rs` usa para generar los permisos `allow-<cmd>` y que `tests/security_baseline.rs` compara contra `capabilities/main-window.json` (conjunto exacto). CSP de producción sin `unsafe-inline`: `style-src 'self'` bloquea atributos `style`, así que **no se usan estilos inline** (ni `style=` de React); todo estilo va por clases Tailwind.
- **Sin dependencias nuevas**: el lockfile npm y `Cargo.lock` no cambian (Node 24, mismas majors). `typescript 6.0.3` ya está en `devDependencies` y expone la API del compilador.
- Skills aplicadas: `frontend-dev` (cambios mínimos, convenciones del repo, verificar con `npm run build`), `vitest` (jsdom, `mockIPC`, `vitest run`), `tauri-v2` (registro en `generate_handler!`, `Result<T, AppError>`, args owned en camelCase, `State<T>` con tipo exacto, comandos `async` con `State<'_>` devuelven `Result`), `tauri-security` (deny-by-default: solo 4 `allow-*` nuevos), `sqlite-database-expert` (conexión gestionada reutilizada, sin internals en errores), `ux-ui-revisor` (WCAG 2.2 AA como guía de diseño), `rust-best-practices`/`rust-testing` (errores tipados, `fn_should_x_when_y`, `tempfile`).

## Goals / Non-Goals

**Goals:**
- Shell y kit de UI reutilizables por C-06..C-12 sin tocar el shell: una pantalla nueva = una entrada en el registro + su componente.
- Contrato IPC de configuración y de estado/recuperación de la base estrecho, validado en el dominio y espejado 1:1 en `ipc.ts`.
- Recuperación de la base **sin reinicio** y segura ante invocaciones concurrentes.
- Regla "sin literales de UI" verificada por test, no solo por revisión.

**Non-Goals:**
- Contenido real de Inicio (health score, tarjetas, acción primaria): C-06/C-12. Inicio queda como estado vacío.
- Autostart real (Task Scheduler, HIGH): C-22. Aquí el control es visible y deshabilitado; el backend rechaza la escritura.
- Umbrales y demás ajustes (`scan_roots`, `report_prefs`): sus changes.
- Librería de gráficos (PA-01), multi-idioma (PA-10), ESLint (pregunta abierta de C-01).
- Mover la apertura de la base fuera de `setup` (ver Risks).
- Revisión de usabilidad IN-01 (tras v0.1).

## Decisions

### D-01 — Estructura de módulos

```
src/
  app/
    App.tsx                      <AppProviders><Shell/></AppProviders>
    navigation.ts                ScreenId, SCREENS (registro tipado), DEFAULT_SCREEN
    providers/
      AppProviders.tsx           compone los providers en orden
      AppInfoProvider.tsx        useAppInfo(): carga get_app_info una vez
      SettingsProvider.tsx       useSettings(): settings, phase, setUiMode()
      StoreStatusProvider.tsx    useStoreStatus(): status, refresh(), recover()
      NavigationProvider.tsx     useNavigation(): current, navigate()
      Announcer.tsx              useAnnounce(): región aria-live única
    shell/
      Shell.tsx                  gate de carga + DensityProvider + layout por modo
      SimpleLayout.tsx           cabecera + nav horizontal
      AdvancedLayout.tsx         sidebar densa
      NavList.tsx                entradas del registro (compartida por ambos layouts)
      SkipLink.tsx               "Saltar al contenido"
      StoreBanner.tsx            aviso global de base no disponible
  features/
    dashboard/HomeScreen.tsx     encabezado + estado vacío
    settings/
      SettingsScreen.tsx         4 secciones
      AppearanceSection.tsx      RadioGroup de modo
      AutostartSection.tsx       Switch aria-disabled
      DatabaseSection.tsx        StatusBadge + acción + ConfirmDialog
      AboutSection.tsx           nombre + versión
      storeStatusText.ts         motivo → textos del catálogo (puro)
  shared/
    i18n/es.ts                   catálogo ampliado
    i18n/format.ts               format(template, params)
    i18n/errors.ts               errorMessage(AppError)
    ui/                          density.tsx, focus.ts, Button, Card, StatusBadge, Icon,
                                 RadioGroup, Switch, ConfirmDialog, index.ts
  lib/ipc.ts                     + getSettings, setSetting, getStoreStatus, recoverStore
  tests/                         (Vitest; ver D-15)

src-tauri/src/
  domain/settings.rs             UiMode, Language, Settings, SettingUpdate, SettingsError,
                                 settings_from_entries, parse_update, update_setting
  domain/store_status.rs         StoreStatus, UnavailableReason, status_of(&StoreError, has_path)
  adapters/sqlite/state.rs       + StoreHandle, RecoveryError
  commands/mod.rs                + run_blocking
  commands/settings.rs           SettingsDto, get_settings, set_setting
  commands/store.rs              StoreStatusDto, RecoveryDto, get_store_status, recover_store
  commands/names.rs              APP_COMMANDS (5)
  error.rs                       + SettingInvalid, SettingReadOnly, StoreNotRecoverable
  lib.rs                         manage(StoreHandle), generate_handler! (5)
capabilities/main-window.json    + allow-get-settings, allow-set-setting,
                                   allow-get-store-status, allow-recover-store
```

`features/settings` agrupa también la UI de la base (la acción vive en Configuración, KB 12 y scope de C-04); el aviso global es del shell. No se crea `features/database`.

### D-02 — Capabilities de spec: dónde vive cada requisito

- `settings` (nueva): modelo, lectura tolerante, escritura validada y pantalla Configuración, incluida la sección Base de datos (es UI de configuración).
- `ui-modes` (nueva): modo por defecto, cambio persistido, layouts, densidad, tema invariable. Es un eje transversal que usarán todas las pantallas; no cabe en `app-shell` (arranque/IPC) ni en `settings` (datos).
- `app-shell` (modificada): conjunto exacto de comandos, catálogo + guardia de literales + errores por `kind`, navegación, aviso global y accesibilidad base (transversal a todas las pantallas).
- `local-store` (modificada): estado consultable y recuperación por IPC, porque extienden "Base corrupta" / "Respaldo y recreación" ya definidos ahí.
- **No** se crea `i18n`: el catálogo ya es requisito de `app-shell` y PA-10 (multi-idioma) está abierto; crear la capability ahora sería especular. `security-baseline` y `domain-ports` no cambian de comportamiento (el test de conjunto exacto ya cubre los permisos nuevos; `SettingsStore` no cambia).

### D-03 — Navegación sin dependencia de router

Registro tipado + estado en contexto:

```ts
export type ScreenId = "home" | "settings";
export interface ScreenDef { id: ScreenId; label: CatalogText; icon: IconName; Component: ComponentType }
export const SCREENS: readonly ScreenDef[] = [ { id: "home", … }, { id: "settings", … } ];
```

`NavigationProvider` guarda `current: ScreenId` en `useState` (default `"home"`); `navigate(id)` lo cambia y mueve el foco al `<main>` (`tabIndex={-1}`) para que lectores de pantalla y teclado sigan el cambio. Las entradas son `<button>` dentro de `<nav aria-label>` con `aria-current="page"` en la actual. El cambio de modo no toca `current` (requisito "conserva la pantalla actual").

Alternativas: `react-router` 7 (dependencia ~60 kB, URLs e historial sin valor en una ventana de escritorio sin deep links), `wouter` (pequeña, pero sigue siendo dependencia nueva para 2 pantallas), TanStack Router (pesada). Si un change futuro necesita deep-link (p. ej. abrir una pantalla desde la bandeja, C-23), `navigate(id)` ya es la API; se reevalúa entonces. **Decisión a confirmar por el dueño** (sin dependencia nueva; recomendado).

Solo se registran pantallas implementadas (spec `app-shell`): sin entradas "próximamente". Cada change de v0.1 agrega su entrada.

### D-04 — Modelo de ajustes en el dominio puro (`domain/settings.rs`)

```rust
pub enum UiMode { Simple, Advanced }            // as_str / parse, Default = Simple
pub enum Language { Es }                        // Default = Es
pub struct Settings { pub ui_mode: UiMode, pub autostart_tray: bool, pub language: Language } // Default
pub enum SettingUpdate { UiMode(UiMode) }       // key(), to_json()
#[derive(thiserror::Error)] pub enum SettingsError {
    #[error("invalid setting {key}: {reason}")] Invalid { key: String, reason: &'static str },
    #[error("setting {0} is read-only")]         ReadOnly(String),
}
pub fn settings_from_entries(entries: &[(String, serde_json::Value)]) -> Settings;
pub fn parse_update(key: &str, value: &serde_json::Value) -> Result<SettingUpdate, SettingsError>;
pub fn update_setting<S: SettingsStore + ?Sized>(store: &S, key: &str, value: &Value)
    -> Result<Settings, SettingsServiceError>;   // Settings(SettingsError) | Store(StoreError)
```

- Constantes de clave (`KEY_UI_MODE = "ui_mode"`, …) en un solo lugar; `DEFAULT_SETTINGS` de `model.rs` se deriva/verifica contra `Settings::default()` en un test (no se duplica la verdad).
- **Lectura tolerante**: valor ausente, de otro tipo o fuera del enum → default; nunca escribe (spec `settings`). Claves desconocidas se ignoran.
- **Escritura**: allowlist explícita por versión. `ui_mode` → escribible (string `"simple"`|`"advanced"`); `autostart_tray`, `language` → `ReadOnly`; cualquier otra (incluida vacía) → `Invalid`. `update_setting` valida **antes** de tocar el store y devuelve la configuración releída (no la supuesta).
- `serde_json` ya se usa en los puertos del dominio; no hay imports prohibidos (test `architecture` sigue verde). Sin `serde` derive sobre el dominio: los DTOs viven en `commands/` (D-07), como anticipó C-02.

Alternativa descartada: `set_setting` con un enum etiquetado deserializado por Tauri (`#[serde(tag = "key", content = "value")]`). Un valor inválido fallaría en la deserialización de argumentos de Tauri, que rechaza con un **string** y no con `AppError` → el frontend recibiría `kind = "unknown"`. Recibir `key: String, value: serde_json::Value` y validar en el dominio da `setting_invalid` tipado.

### D-05 — Catálogo, formateo y errores por `kind` (sin librería i18n)

- `es.ts` sigue siendo un objeto `as const` (tipo `Catalog = typeof es`), ampliado con `nav`, `shell`, `home`, `settings.{appearance,autostart,database,about}`, `store.reasons`, `errors`, `a11y`.
- `format(template, params)` reemplaza `{nombre}`; un parámetro faltante deja el marcador visible (falla evidente en revisión, nunca `undefined`). Puro y testeado.
- `errorMessage(error: AppError): string` busca `es.errors[kind]` con `Object.hasOwn` (un `kind` como `"toString"` no debe resolver a una propiedad del prototipo) y cae en `es.errors.unknown`. Kinds mapeados: `internal`, `unknown`, `invalid_response`, `store_corrupt`, `store_schema_too_new`, `store_migration_failed`, `store_unavailable`, `store_not_recoverable`, `setting_invalid`, `setting_read_only`. El `message` del backend (inglés, técnico) **nunca** se muestra.
- Alternativa: i18next/react-intl (dependencia nueva, runtime extra; PA-10 abierta y de prioridad baja). El catálogo tipado permite migrar después sin tocar componentes si todos pasan por `es.*`/`format`.

### D-06 — Guardia automática de literales con la API del compilador de TypeScript

Test `src/tests/uiLiterals.test.ts` con un detector puro en `src/tests/support/uiLiterals.ts`:

- Fuentes: `import.meta.glob("/src/{app,features,shared/ui}/**/*.tsx", { query: "?raw", import: "default", eager: true })` (Vite/Vitest, sin APIs de Node ni `@types/node` en `src/`).
- `ts.createSourceFile(..., ScriptKind.TSX)` y recorrido del AST: reporta `JsxText` con contenido no blanco, `StringLiteral`/`NoSubstitutionTemplateLiteral` como hijo directo de `JsxExpression` en posición de hijo, y literales en los atributos `aria-label`, `aria-description`, `aria-roledescription`, `aria-valuetext`, `title`, `alt`, `placeholder`, `label`. Mensaje: `archivo:línea atributo/texto`.
- El detector se prueba primero con fuentes inline (casos positivos y negativos: `className="..."`, `role="switch"`, `{es.app.title}` no se reportan).
- Alternativa: ESLint + `eslint-plugin-react` (`jsx-no-literals`) + `@typescript-eslint` (también cubriría `no-explicit-any`). Más estándar, pero agrega 3+ dependencias y configuración; queda como **opción para el dueño** (recomendación: test propio ahora; ESLint, si se decide, en C-13 junto con CI).

### D-07 — Comandos IPC y DTOs

| Comando | Args | Ok | Errores |
|---|---|---|---|
| `get_settings` | — | `SettingsDto` | `store_*` |
| `set_setting` | `key: String`, `value: serde_json::Value` | `SettingsDto` (releída) | `setting_invalid`, `setting_read_only`, `store_*` |
| `get_store_status` | — | `StoreStatusDto` | — (infalible salvo `internal`) |
| `recover_store` | — | `RecoveryDto` | `store_not_recoverable`, `store_*` |

```rust
#[derive(Serialize)] #[serde(rename_all = "camelCase")]
pub struct SettingsDto { ui_mode: &'static str, autostart_tray: bool, language: &'static str }
#[derive(Serialize)] #[serde(tag = "status", rename_all = "snake_case")]
pub enum StoreStatusDto { Ready, Unavailable { reason: &'static str, recoverable: bool } }
#[derive(Serialize)] #[serde(rename_all = "camelCase")]
pub struct RecoveryDto { backup_file_name: String }   // solo el nombre, sin directorio
```

- Todos `async fn` que clonan el `StoreHandle` de `State<'_, StoreHandle>` y ejecutan el trabajo en `tauri::async_runtime::spawn_blocking` vía `commands::run_blocking` (un `JoinError` → `AppError::Internal`). Motivo: en Tauri 2 los comandos síncronos corren en el hilo principal; `rusqlite` y los `rename` bloquean (lo anticipó C-02 D-03).
- Cada comando es una envoltura de una función pura de lógica (`read_settings(&StoreHandle)`, `write_setting(&StoreHandle, &str, &Value)`, `store_status(&StoreHandle)`, `recover(&StoreHandle, &impl Clock)`) testeable sin Tauri; además un smoke IPC por comando con `tauri::test::mock_builder` + `get_ipc_response` sobre un `TempDir`.
- `AppError` agrega `SettingInvalid(String)` → `setting_invalid`, `SettingReadOnly(String)` → `setting_read_only`, `StoreNotRecoverable(String)` → `store_not_recoverable`; `From<SettingsServiceError>` y `From<RecoveryError>`. El mapeo existente de `StoreError` no cambia.
- `APP_COMMANDS` pasa a `["get_app_info", "get_settings", "set_setting", "get_store_status", "recover_store"]`; `build.rs` genera los permisos y `main-window.json` lista exactamente los 5 `allow-*`. Sin plugins, sin cambios de CSP.

### D-08 — `StoreHandle`: estado intercambiable para recuperar sin reiniciar

```rust
#[derive(Clone)]
pub struct StoreHandle(Arc<RwLock<StoreState>>);
impl StoreHandle {
    pub fn new(state: StoreState) -> Self;
    pub fn status(&self) -> StoreStatus;                          // lectura
    pub fn store(&self) -> Result<Arc<SqliteStore>, StoreError>;  // clona el Arc o el error
    pub fn recover(&self, clock: &impl Clock) -> Result<PathBuf, RecoveryError>; // escritura
}
pub enum RecoveryError { NotRecoverable(&'static str), Failed(StoreError) }
```

- `app.manage(StoreHandle)` reemplaza `app.manage(StoreState)`. Los comandos toman el `Arc<SqliteStore>` bajo lock de lectura y lo **sueltan** antes de operar (el `Mutex<Connection>` interno serializa la base).
- `recover` toma el lock de **escritura** durante toda la operación: dos invocaciones simultáneas se serializan; la segunda ve `Ready` → `NotRecoverable` (spec "Invocaciones simultáneas"). Recuperable = `Unavailable` con motivo `corrupt` o `migration_failed` **y** `db_path` conocido.
- Éxito → el estado pasa a `Ready(Arc::new(store))`. Error → **se conserva el estado previo** (sigue recuperable; el botón sigue disponible). Reintentar es seguro: `backup_and_recreate` solo renombra y nunca pisa un respaldo existente.
- Lock envenenado → `StoreError::Unavailable("lock poisoned")`, nunca `panic`.
- El nombre devuelto al frontend es `file_name()` del respaldo (sin directorio, spec `local-store`).
- `domain/store_status.rs` define `StoreStatus`/`UnavailableReason` y `status_of(&StoreError, has_path)` como función pura (tabla de tests); `NotFound`/`InvalidState`/`Constraint` (no se producen al abrir) se clasifican defensivamente como `unavailable`, no recuperable.
- Alternativa: `Mutex<Option<Arc<SqliteStore>>>` o reiniciar la app tras recrear. Reiniciar rompe el requisito "sin reinicio" y en C-05 implicaría otro UAC.

### D-09 — Providers y flujo de datos del frontend (sin librería de estado)

- `SettingsProvider`: `phase: "loading" | "ready" | "failed"`, `settings: Settings` (defaults en `failed`), `setUiMode(mode)`. **No optimista**: llama `setSetting("ui_mode", mode)` y aplica la configuración devuelta; mientras tanto `pending = true` deshabilita el selector; si falla, el modo no cambia y expone `lastError` (spec `ui-modes` "Cambio de modo fallido"). La latencia local es de milisegundos, así que esperar evita el parpadeo de un rollback.
- `StoreStatusProvider`: `status`, `refresh()`, `recover()`. Tras un `recover()` exitoso refresca su estado **y** pide a `SettingsProvider` recargar (callback compartido en `AppProviders`).
- `Shell` muestra solo `es.app.loading` hasta que `phase !== "loading"` (spec "no mostrar el layout de ningún modo antes de conocer el modo"); en `failed` usa `simple`.
- Context + `useReducer`/`useState`; sin Redux/Zustand/React Query: 3 recursos, sin caché ni reintentos complejos.
- Cada hook lanza un error claro si se usa fuera de su provider (test).

### D-10 — Densidad por contexto en el UI kit

`shared/ui/density.tsx` exporta `DensityProvider` y `useDensity(): "simple" | "advanced"`; el `Shell` lo provee con el modo efectivo. `Card`, `Button`, `RadioGroup`, `NavList` eligen clases (padding, tamaño de texto, gap) según la densidad; el contenido y la estructura no cambian (spec "mismo componente, otra densidad"; `data-density` en el nodo raíz para testearlo). El kit **no** importa nada de `app/` (dependencia de capas `app → features → shared`, nunca al revés).

Componentes: `Button` (`variant: "primary" | "secondary" | "danger"`, `data-variant`), `Card` (sección con encabezado), `StatusBadge` (`tone: "success" | "warning" | "danger" | "neutral"`, siempre `Icon` `aria-hidden` + texto), `Icon` (SVG inline propios: home, settings, check, warning, error, info; sin librería), `RadioGroup` (`fieldset`/`legend` + `input type="radio"` nativos: flechas y foco del navegador), `Switch` (D-12), `ConfirmDialog` (D-11).

"Una acción primaria por pantalla" (US-001) se verifica con un test que renderiza cada pantalla registrada y cuenta `[data-variant="primary"]` ≤ 1.

### D-11 — Diálogo de confirmación con `<dialog>` nativo

`ConfirmDialog` usa `<dialog>` + `showModal()`: WebView2 (Chromium) da gratis fondo inerte, `Escape` (evento `cancel`), foco atrapado y restauración del foco al cerrar. `role="alertdialog"`, `aria-labelledby`/`aria-describedby` a título y explicación del catálogo; `autoFocus` en **Cancelar** (acción segura por defecto); el botón de confirmación usa `variant="danger"` y se deshabilita mientras la operación está en curso (evita doble invocación además del lock de D-08).

jsdom no implementa de forma fiable `showModal`/`close`: `src/tests/setup.ts` agrega un shim mínimo (solo si faltan) que alterna el atributo `open` y despacha `cancel`/`close`. Los tests verifican lo que controla el código (qué botón confirma, que `Escape`/Cancelar no invoca IPC, foco devuelto al disparador vía `ref`), no el comportamiento nativo del navegador.

Alternativa: diálogo propio con `div role="alertdialog"` + trampa de foco + `inert` en hermanos: más código y más superficie de error de accesibilidad.

### D-12 — Autostart visible y deshabilitado

`Switch` = `<button role="switch" aria-checked>`; para autostart se usa `aria-disabled="true"` (no el atributo `disabled`) para que siga siendo **enfocable** y su explicación (`aria-describedby` → "Disponible en una próxima versión") se lea con lector de pantalla; el `onClick` es un no-op y no hay ninguna llamada IPC (spec `settings`). Estilo atenuado por `aria-disabled:` de Tailwind. El backend igual rechaza `autostart_tray` con `setting_read_only` (defensa en profundidad). C-22 lo habilita.

Alternativa: ocultar el control hasta C-22 (KB 12 lo lista en Configuración v0.1 y la US-024 pide verlo). **UX a confirmar por el dueño** (recomendado: visible y deshabilitado).

### D-13 — Accesibilidad base y tokens del tema

- **Tokens** (`@theme` en `styles.css`), mismos en ambos modos: se agregan `--color-focus`, `--color-success`, `--color-warning`, `--color-danger`, `--color-on-accent`, `--color-border-strong` (valores iniciales propuestos: `#7cb8ff`, `#4ade80`, `#fbbf24`, `#f87171`, `#0f1115`, `#6b7385`; se ajustan si el test de contraste falla).
- **Test de contraste** (`src/tests/contrast.test.ts`): importa `../styles.css?raw`, extrae `--color-*` y calcula la relación WCAG; pares de texto (`content`, `content-muted`, `success`, `warning`, `danger` sobre `surface` y `surface-raised`; `on-accent` sobre `accent` y `danger`) ≥ 4.5:1; `focus` y `border-strong` sobre ambos fondos ≥ 3:1. La función de contraste se prueba con pares conocidos (blanco/negro = 21).
- **Foco**: constante `focusRing` en `shared/ui/focus.ts` (`focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus`) aplicada por todos los controles del kit, `NavList` y `SkipLink`; test de clases.
- **Teclado**: solo elementos nativos (`button`, `input`, `dialog`); `SkipLink` es el primer enfocable y lleva a `<main id="main" tabIndex={-1}>`.
- **Movimiento reducido**: los componentes solo usan `motion-safe:transition-*`; además `styles.css` agrega `@media (prefers-reduced-motion: reduce)` que anula transiciones/animaciones. Test: la guardia de D-06 también reporta clases `transition`/`animate-` sin prefijo `motion-safe:`.
- **Anuncios**: `Announcer` renderiza una única región `role="status" aria-live="polite"` en el shell; cambio de modo y recuperación anuncian textos del catálogo. Errores en `role="alert"`.
- **Estado**: `StatusBadge` siempre icono + texto.
- **Viewport**: la ventana tiene `minWidth 960`, pero con zoom 200 % el ancho CSS baja a ~480 px: el layout avanzado usa `flex-col md:flex-row` (la sidebar pasa arriba bajo `md`) y el contenido usa anchos fluidos; sin scroll horizontal.
- **CSP**: nada de atributos `style` ni estilos en runtime (CSP `style-src 'self'`); todo por clases.

### D-14 — Cliente IPC tipado con validación en el borde

`ipc.ts` agrega `call<T>(command, args?: InvokeArgs)` y decodificadores por DTO (`decodeSettings`, `decodeStoreStatus`, `decodeRecovery`: `unknown` → tipo, por narrowing). Una respuesta con forma inesperada rechaza con `{ kind: "invalid_response", message }` en vez de propagar datos mal tipados a la UI (TypeScript no puede garantizar lo que devuelve Rust). Tipos:

```ts
export type UiMode = "simple" | "advanced";
export interface Settings { uiMode: UiMode; autostartTray: boolean; language: "es" }
export interface WritableSettings { ui_mode: UiMode }
export function setSetting<K extends keyof WritableSettings>(key: K, value: WritableSettings[K]): Promise<Settings>;
export type StoreUnavailableReason = "corrupt" | "schema_too_new" | "migration_failed" | "unavailable";
export type StoreStatus = { status: "ready" } | { status: "unavailable"; reason: StoreUnavailableReason; recoverable: boolean };
export interface RecoveryResult { backupFileName: string }
```

`WritableSettings` refleja la allowlist de D-04: intentar `setSetting("autostart_tray", true)` no compila (además el backend lo rechaza).

### D-15 — Tests

- **Rust**: tabla para `parse_update`/`settings_from_entries`/`status_of`; `update_setting` contra `InMemoryStore` (fake de C-02); `StoreHandle` sobre `TempDir` (ready, corrupt → recover, migration_failed, schema_too_new, sin path, recuperación concurrente con `Barrier` de 2 hilos, rename bloqueado abriendo el archivo con `OpenOptionsExt::share_mode(0)`); serialización exacta de los DTOs con `serde_json::json!`; smoke IPC por comando con runtime mock (`InvokeBody::Json` para args). Ningún test usa `%LOCALAPPDATA%` ni `PCOPT_DATA_DIR`.
- **Frontend** (`src/tests/`, subcarpetas `ui/`, `app/`, `settings/`, `support/`): `ipc`, `format`, `errors`, `uiLiterals`, `contrast`, componentes del kit, providers, `Shell` (gate de carga, layouts, nav, skip link, banner), `SettingsScreen` (modo, autostart, base, diálogo, recuperación). IPC siempre por `mockIPC`; helper `support/mockBackend.ts` que simula los 5 comandos con estado en memoria y registra llamadas. `@testing-library/user-event` no está instalado: se usa `fireEvent` + `focus()`/`keyDown` (sin dependencia nueva). Consecuencia: la navegación real con Tab (orden de foco) se verifica por estructura (primer enfocable del DOM, elementos nativos) y en el smoke manual.

## Risks / Trade-offs

- **[Smoke IPC de comandos `async` con runtime mock]** `get_ipc_response` debería esperar la respuesta de un comando async; si no lo hace de forma fiable → la lógica se cubre con las funciones puras (D-07) y el smoke se limita a un comando, documentándolo en Apply notes.
- **[`?raw` de CSS en Vitest]** Vitest podría procesar `styles.css?raw` con el pipeline de CSS. → Si devuelve vacío, el test de contraste lee los tokens desde un módulo `shared/ui/tokens.ts` que el CSS replica, con un test de igualdad entre ambos.
- **[Guardia de literales con falsos positivos]** p. ej. un `·` decorativo. → Se resuelve moviendo el texto al catálogo; no hay lista de excepciones (mantiene la regla simple).
- **[Shim de `<dialog>` en jsdom]** puede ocultar diferencias con Chromium. → Smoke manual en `npm run tauri dev` (tarea final) verificando Escape, foco inicial y restauración.
- **[Apertura de la base en `setup` bloquea el arranque]** (C-02). → Se mantiene; la base está acotada por retención. Si el smoke manual muestra demora perceptible, se registra para C-05/C-06.
- **[Allowlist de escritura crece por change]** → Cada change que agregue un ajuste escribible extiende `parse_update`, `WritableSettings` y sus tests; el test de "clave desconocida" impide escrituras arbitrarias.
- **[`aria-disabled` permite enfocar un control inerte]** → Es intencional (descubribilidad); el `onClick` es no-op y hay test de que no se invoca IPC.
- **[Recuperar con otro proceso usando la base]** → `rename` falla, error `store_unavailable` con texto del catálogo "cerrá otros programas y reintentá"; nada se borra y el estado sigue recuperable.

## Migration Plan

Sin migraciones de esquema: `settings` ya existe con su seed. Despliegue normal del binario. Rollback = descartar la rama; los respaldos `.corrupt-*.bak` creados por pruebas manuales se borran a mano del directorio temporal de `PCOPT_DATA_DIR`.

## Open Questions

Ninguna que cambie specs o tareas. Decisiones tomadas con recomendación y pendientes de **confirmación del dueño** (si se rechazan, cambian solo las tareas indicadas):

- D-03 navegación sin router (alternativa: `wouter`/`react-router` → tareas 7.1 y 8.2).
- D-06 guardia de literales con test propio (alternativa: ESLint + plugins → tarea 4.3).
- D-12 autostart visible y deshabilitado (alternativa: ocultarlo → tarea 9.3).

## Apply notes

- D-03, D-06, D-12 confirmados por el dueño (navegación sin router, guardia por test Vitest, autostart visible con `aria-disabled`).
- `styles.css?raw` devolvía cadena vacía en Vitest: se resolvió con `test.css.include: [/styles.css/]` en `vite.config.ts` (no hizo falta el plan B de tokens.ts).
- `Switch` no maneja `keydown`: un `<button>` nativo activa `click` con Enter/Espacio; los tests verifican `click` y que `keyDown` no dispare IPC.
- El banner global se renderiza en `Shell` (no dentro de cada pantalla) para evitar un ciclo de imports `navigation.ts` <-> pantallas.
- Al cambiar de modo el layout se remonta; `AppearanceSection` devuelve el foco a la opción elegida (`setTimeout 0`).
- `SettingsProvider.setUiMode` devuelve `Promise<boolean>` (éxito) para anunciar solo si se guardó.
- Tarea 10.2 (smoke manual con `npm run tauri dev`) pendiente del dueño.

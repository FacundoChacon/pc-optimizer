# Proposal

## Why

Tras C-01/C-02 la interfaz es una sola pantalla (`App.tsx`) que muestra título y versión: no hay shell, navegación, modos de UI, kit de componentes ni forma de leer o cambiar la configuración persistida, y una base corrupta deja la app en `StoreState::Unavailable` sin que el usuario se entere ni pueda recuperarla (C-02 difirió esa UI a C-04). Todas las pantallas de v0.1 (C-06 monitor, C-07/C-08 espacio y limpieza, C-11 duplicados, C-12 dashboard) necesitan un shell común con modo simple/avanzado, un catálogo de textos y un cliente IPC tipado sobre el que montarse; sin C-04 cada una reinventaría layout, navegación y manejo de errores.

## What Changes

- **Shell de la app** (`src/app`): providers (configuración, estado de la base, navegación), registro tipado de pantallas sin dependencia de router, layout **simple** (cabecera con navegación, tarjetas grandes) y layout **avanzado** (sidebar densa), mismo tema oscuro; enlace "Saltar al contenido"; aviso global cuando la base no está disponible.
- **Modos de UI** (US-001, US-002): la app arranca en modo simple; el modo se cambia en Configuración, se persiste en `settings.ui_mode` vía IPC y se aplica sin reiniciar, conservando la pantalla actual. Si la persistencia falla, el modo no cambia y se muestra el error del catálogo.
- **Pantalla Configuración v0.1** (US-024): selector de modo, autostart visible pero **deshabilitado** hasta C-22 (con explicación), sección "Base de datos" con estado (icono + texto) y acción **"Respaldar y recrear"** con diálogo de confirmación, y sección "Acerca de" con la versión.
- **Pantalla Inicio** provisoria: encabezado y estado vacío del catálogo; las tarjetas y la acción primaria reales llegan con C-06..C-12.
- **Backend** (`src-tauri`): modelo tipado de ajustes en el dominio puro (`UiMode`, lectura tolerante con valores por defecto, validación con allowlist de claves escribibles: en v0.1 solo `ui_mode`); `StoreHandle` con estado intercambiable para recuperar la base **sin reiniciar**; 4 comandos IPC nuevos: `get_settings`, `set_setting`, `get_store_status`, `recover_store`; nuevos `kind` de error `setting_invalid`, `setting_read_only`, `store_not_recoverable`.
- **Capabilities**: se agregan exactamente los 4 permisos `allow-*` de los comandos nuevos (sin plugins, sin cambios de CSP).
- **Cliente IPC** (`src/lib/ipc.ts`): espejo tipado de los 4 comandos y sus DTOs.
- **Catálogo e i18n** (`src/shared/i18n`): nuevas secciones del catálogo, formateo con parámetros, traducción de cada `kind` de error a un mensaje del catálogo (nunca se muestra el `message` técnico del backend), y una **guardia automática** (test con la API del compilador de TypeScript, sin dependencias nuevas) que falla si un componente tiene texto visible o atributos accesibles literales.
- **UI kit** (`src/shared/ui`): `Button`, `Card`, `StatusBadge`, `Icon` (SVG inline), `RadioGroup`, `Switch`, `ConfirmDialog`, con densidad `simple`/`advanced` provista por contexto (componentes compartidos, no pantallas duplicadas).
- **Accesibilidad base** (WCAG 2.2 AA): foco visible, navegación completa por teclado, `prefers-reduced-motion`, estado con icono + texto, anuncios `aria-live`, diálogo modal accesible, y un test de contraste de los tokens del tema.
- **Sin dependencias nuevas** (npm ni cargo): `package-lock.json` y `Cargo.lock` no cambian. No se decide PA-01 (gráficos).

## Capabilities

### New Capabilities
- `settings`: configuración de usuario persistida (modelo tipado, lectura tolerante, claves escribibles y de solo lectura, comandos `get_settings`/`set_setting`) y pantalla Configuración v0.1 (modo, autostart deshabilitado, base de datos, acerca de).
- `ui-modes`: modo simple por defecto y modo avanzado opt-in, mismo tema, cambio sin reinicio, layouts del shell por modo y componentes compartidos con densidad por modo.

### Modified Capabilities
- `app-shell`: la lista exacta de comandos habilitados pasa de 1 a 5; el catálogo de textos se extiende a nombres accesibles y mensajes de error por `kind` y queda custodiado por un test; se agregan el shell con navegación entre pantallas, el aviso global de base no disponible y la accesibilidad base.
- `local-store`: se agrega el estado de la persistencia consultable por IPC (sin detalles internos), la recuperación iniciada por el usuario que reemplaza el estado sin reiniciar, y el rechazo de la recuperación fuera de los casos recuperables.

## Impact

- **Código frontend**: reescritura de `src/app/App.tsx`; nuevos `src/app/{providers,shell,navigation}`, `src/features/{settings,dashboard}`, `src/shared/{ui,i18n}`; extensión de `src/lib/ipc.ts` y `src/styles.css` (tokens de estado y foco); tests nuevos bajo `src/tests/`. Se adaptan `App.test.tsx` e `ipc.test.ts` sin perder sus casos.
- **Código backend**: nuevo `domain/settings.rs`; `commands/{settings.rs, store.rs}`; `adapters/sqlite/state.rs` (`StoreHandle`); `error.rs` (3 variantes); `lib.rs` (`manage(StoreHandle)` y `generate_handler!`); `commands/names.rs` y `capabilities/main-window.json` (+4 comandos, mismo conjunto exacto).
- **APIs IPC**: 4 comandos nuevos, todos `async` con el trabajo bloqueante en `spawn_blocking`. Contrato de error `{ kind, message }` sin cambios.
- **Dependencias**: ninguna. Router, i18n, estado global e iconos se resuelven con código propio (ver design D-03/D-05/D-08).
- **Datos de usuario**: `recover_store` renombra la base dañada a `data.db.corrupt-<ms>.bak` (y `-wal`/`-shm`) solo tras confirmación explícita; nunca se recrea automáticamente ni se borra nada.
- **Tests**: Vitest (cambio de modo, render desde catálogo, persistencia vía IPC mockeado, recuperación, accesibilidad, guardia de literales, contraste) y `cargo test` (dominio de ajustes, `StoreHandle`, comandos por IPC con runtime mock sobre `TempDir`). Siguen verdes `architecture`, `no_network_crates` y `security_baseline`.
- **Changes posteriores**: C-06..C-12 registran su pantalla en el registro y usan el UI kit; C-22 vuelve escribible `autostart_tray` (HIGH) y habilita el switch; PA-10 (i18n multi-idioma) puede reemplazar el catálogo propio sin tocar componentes.

# Proposal

## Why

US-003 (v0.1, gobernanza MEDIUM): el usuario quiere ver **en vivo** cuánto usan el procesador, la memoria y el disco **para detectar qué satura su equipo**, sin que mirar empeore el problema (RN-MON-06). Hoy la app no muestra ninguna métrica de rendimiento: el puerto `Sensors` de C-02 existe pero no tiene adaptador ni consumidor, y `sysinfo` está fijada desde C-01 sin uso. Es la pantalla "Monitor" del mapa de KB 12, y C-12 (dashboard), C-14 (sensores de hardware) y C-15 (historial) dependen de ella. Además obliga a decidir **PA-01** (librería de gráficos), que hoy bloquea también el treemap diferido de C-07.

## What Changes

- **Muestreo en vivo en un hilo propio** cada 1 s (RN-MON-01) de: uso total de CPU, memoria usada/total, espacio usado/total del volumen del sistema, actividad del disco del sistema (porcentaje de tiempo activo) y lectura/escritura en bytes por segundo. El hilo arranca con la app, es independiente de los trabajos de escaneo y limpieza (`ActiveJob`): ni los bloquea ni lo bloquean.
- **Honestidad de métricas**: una métrica que no se puede medir (o que todavía no tiene dos lecturas para calcular una tasa) viaja como **ausente**, nunca como 0.
- **Ventana en memoria de ~5 min** (ring buffer de 300 muestras, nada se persiste); **huecos** tras suspensión, hibernación o un bloqueo del hilo se marcan como ausencia de datos (la línea del gráfico se corta) y las tasas de la primera muestra posterior se descartan.
- **Agregación en el dominio**: valor actual, promedio de 60 s, pico de la ventana y un **nivel llano** por métrica (normal / alto / muy alto; disco "casi lleno") para el modo simple; sirve después a C-12, C-15 y C-24.
- **Flujo por suscripción, sin polling**: `start_monitor_stream` devuelve la ventana actual y entrega cada muestra nueva por un `Channel` de la propia invocación (mismo mecanismo que el progreso de los escaneos); `stop_monitor_stream` corta el flujo. **Sin eventos globales ni permisos `core:*`.**
- **PA-01 decidida (recomendación)**: componentes de gráficos **propios en SVG**, sin librería (medidor y serie temporal), con tema oscuro por tokens, sin estilos en línea (CSP `style-src 'self'`), testeables en jsdom; uPlot queda como alternativa preferida si C-15 la necesita.
- **UI**: entrada nueva **Monitor** en la navegación. Simple: tres tarjetas (Procesador, Memoria, Disco C:) con valor y nivel en palabras, sin acción primaria. Avanzado: lista de medidores, gráficos en vivo de 5 min por métrica (con cursor, lectura por teclado y vista de tabla) y panel de detalles (intervalo, huecos, costo de lectura, métricas no disponibles con motivo).
- **Puerto `Sensors` rediseñado** (la muestra de C-02 asumía porcentajes ya calculados): lecturas crudas con contadores acumulados y un reloj monotónico, para que tasas y huecos se calculen y prueben en el dominio.
- Decisiones que requieren aprobación del dueño **[DUEÑO]** en `design.md` (PA-01, mecanismo de flujo y permisos, ciclo de vida del muestreo, intervalo, métrica de actividad de disco y feature de `windows`, procesos fuera de alcance, umbrales de nivel, UI, presupuesto de rendimiento, smoke).

Fuera de alcance: temperaturas, SMART y GPU (C-14); persistencia y gráficos de 24 h (C-15); alertas (C-24); lista de procesos (seguimiento propio, ver D-08); otros volúmenes y discos (PA-12); intervalo relajado en bandeja (C-22); uso por núcleo; tarjetas del dashboard de Inicio (C-12 consume lo que entrega este change); el treemap de C-07 (queda habilitado por PA-01, no se hace aquí).

## Capabilities

### New Capabilities
- `live-monitor`: muestreo en vivo de CPU, memoria y disco del sistema en segundo plano (intervalo, métricas y su disponibilidad, ventana en memoria, huecos, agregación y niveles, independencia de los trabajos, presupuesto de rendimiento, flujo por suscripción, comandos IPC y pantalla Monitor en ambos modos).
- `ui-charts`: componentes de visualización compartidos (medidor y serie temporal en vivo) con tema oscuro, compatibles con la CSP estricta y accesibles (valores en texto, lectura por teclado, vista de tabla, sin depender solo del color).

### Modified Capabilities
- `domain-ports`: la muestra de sensores pasa a lecturas crudas (contadores acumulados de disco, CPU calculada por la fuente, espacio del volumen) con métricas ausentes por separado; se agrega un reloj monotónico para medir intervalos.
- `app-shell`: lista exacta de comandos IPC habilitados (2 nuevos), escenario de flujos sin permisos de eventos, y pantalla Monitor en la navegación.

## Impact

- **Rust**: nuevo `src-tauri/src/domain/monitor/` (modelo, ring buffer, tasas y huecos, agregación y niveles, ejecución de un ciclo de muestreo, `*_tests.rs`); `domain/ports/sensors.rs` (lectura cruda) y `domain/ports/clock.rs` (reloj monotónico) con dobles en `fakes.rs`; nuevo `adapters/windows/sensors.rs` (`sysinfo` para CPU, memoria y contadores de disco; `VolumeSpace` de C-07 para el espacio) y `adapters/windows/pdh.rs` (tiempo activo del disco, `unsafe` mínimo con `// SAFETY:`); nuevo `commands/monitor.rs` (`MonitorHub`, hilo de muestreo, 2 comandos, DTOs, tests); `commands/names.rs`, `lib.rs` (estado y arranque del hilo en `setup`), `error.rs` (`monitor_unavailable`).
- **Capabilities**: `capabilities/main-window.json` + `allow-start-monitor-stream`, `allow-stop-monitor-stream` (29 permisos en total, 0 `core:*`, 0 plugins). Sin cambios de CSP.
- **Frontend**: `src/lib/ipc.ts` (tipos, decodificadores, `startMonitorStream`/`stopMonitorStream`), nuevo `src/shared/charts/` (medidor, serie temporal, escalas puras), nuevo `src/features/monitor/`, `src/app/navigation.ts`, `src/app/providers/AppProviders.tsx` (proveedor del flujo, reutilizable por C-12), `src/shared/i18n/{es,errors}.ts`, `src/tests/support/mockBackend.ts` y tests.
- **Dependencias**: ninguna nueva en npm ni en Cargo. `sysinfo =0.40.3` (ya fijada, features `system`/`disk`) pasa a tener consumidor; se declara la feature `Win32_System_Performance` de `windows` 0.62.2 (ya compilada en el grafo por `sysinfo`: `Cargo.lock` no cambia). Ver D-06.
- **Esquema**: sin migraciones (nada se persiste).
- **Docs** (en el apply): `CHANGES.md` (C-06 y notas para C-07, C-12, C-14, C-15, C-22, C-24), KB 02 (comandos y flujo), KB 05 (RN-MON-01/06 con valores reales), KB 06 US-003, KB 09 (decisión de gráficos), KB 10 (PA-01 resuelta), KB 12 (pantalla Monitor y componentes de gráficos), KB 13 (fuentes reales, huecos, ciclo de vida del muestreo).

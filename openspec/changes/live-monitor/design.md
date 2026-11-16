# Design

> Gobernanza **MEDIUM** (monitor de solo lectura, sin tocar archivos del usuario ni persistir nada). Toca dos puntos sensibles de la línea base: la superficie IPC y las capabilities (flujo de backend a frontend sin permisos `core:*`) y `unsafe` nuevo en un adaptador de Windows (contadores PDH). Las decisiones marcadas **[DUEÑO]** requieren aprobación explícita antes del apply. Las tareas asumen la opción recomendada; si el dueño elige otra, se actualizan specs y tasks antes de codear (`/opsx:update`).

## Context

Motivación: ver `proposal.md`. Estado actual relevante (observado en el código, no supuesto):

- **Puerto de sensores (C-02)**: `domain/ports/sensors.rs` declara `Sensors { fn sample(&mut self) -> Result<SensorSample, PortError> }` con `SensorSample { cpu_pct: f32, ram_used_bytes, ram_total_bytes, disk_busy_pct: Option<f32> }` y un doble guionado `FakeSensors` en `fakes.rs`. No hay adaptador ni consumidor. La forma asume porcentajes ya calculados y una CPU siempre presente: no permite calcular tasas ni detectar huecos en el dominio, y obliga a inventar un 0 de CPU en la primera lectura (prohibido por la spec actual "ausente, no cero").
- **Reloj (C-02)**: `Clock::now() -> EpochMillis` (pared, UTC). No hay reloj monotónico; la pared puede saltar (cambio de hora, sincronización) y no sirve sola para medir intervalos.
- **`sysinfo =0.40.3`** fijada desde C-01 con `default-features = false, features = ["system", "disk", "component"]` y sin consumidor (comentario de `[lints.cargo]`). Fuente revisada en `~/.cargo/registry`: en Windows la CPU sale de PDH (`PdhAddEnglishCounterW`, intervalo mínimo `MINIMUM_CPU_UPDATE_INTERVAL = 200 ms`), la memoria de `GlobalMemoryStatusEx`, y `Disk::usage()` expone contadores acumulados de lectura/escritura por volumen vía `IOCTL_DISK_PERFORMANCE`. La feature `system` de `sysinfo` ya activa `windows/Win32_System_Performance` sobre el mismo `windows 0.62.2` del proyecto (un solo `windows` en `Cargo.lock`).
- **Espacio del volumen (C-07)**: puerto `VolumeSpace` y adaptador `WindowsVolumeSpace` (`GetDiskFreeSpaceExW`) ya existen y se prueban sobre un `tempdir`.
- **Trabajos (C-08..C-11, C-07)**: `ActiveJob` da exclusividad a escaneos, limpieza y vaciado de Papelera; el progreso viaja por un `tauri::ipc::Channel` recibido como argumento del comando (`onProgress`), sin `core:*`. `Channel<T>` de Tauri 2.12.5 es `Clone` y puede guardarse y usarse después de que el comando responda; mensajes de menos de 8 KiB se entregan por ejecución directa en el webview.
- **Seguridad**: 27 permisos `allow-*`, 0 `core:*`, 0 plugins (`security_baseline` lo verifica); CSP de producción `style-src 'self'` (sin `unsafe-inline`), `img-src 'self'`; `withGlobalTauri: false`; `UNSAFE_ALLOWED_FILES` en `tests/architecture.rs` lista los 9 archivos con `unsafe`.
- **Privilegios (C-05)**: el contexto (`match`/`mismatch`) se calcula al arrancar; el banner global ya avisa en mismatch.
- **Frontend**: navegación Inicio / Espacio en disco / Limpieza / Duplicados / Configuración (`navigation.ts`); `density.tsx` para simple/avanzado; `Card`, `StatusBadge`, `Icon`; tokens de tema en `styles.css` (`--color-surface #0f1115`, `--color-accent #4f9cf9`, `success`, `warning`, `danger`…); `ProportionBar` de C-07 (SVG con atributos, sin `style`, testeado). No hay librería de gráficos (PA-01 abierta). `mockBackend.ts` simula `Channel` para los escaneos.

## Goals / Non-Goals

**Goals:**
- Muestreo barato y continuo que no compite con los trabajos ni los bloquea, y cuyo costo se mide (RN-MON-06).
- Todo lo calculable (tasas, huecos, promedios, niveles) en el dominio puro, probado con dobles deterministas (sin reloj real ni `sleep` en tests).
- Flujo de backend a frontend con el mismo mecanismo ya auditado (`Channel` de la invocación): sin permisos `core:*`, sin plugins, sin cambios de CSP.
- Sin dependencias nuevas; `Cargo.lock` y `package-lock.json` sin cambios.
- Dejar a C-12, C-14, C-15, C-22 y C-24 puntos de extensión claros sin implementarlos.

**Non-Goals:**
- Persistir muestras o agregados (C-15). Uso por núcleo, procesos, temperaturas, GPU, SMART, red.
- Otros volúmenes o discos físicos (PA-12): solo el volumen del sistema.
- Ajustar el intervalo según visibilidad o bandeja (C-22); exactitud de "actividad" idéntica al Administrador de tareas (se documenta la fuente).

## Decisions

### D-01 — Ciclo de vida del muestreo: siempre activo desde el arranque **[DUEÑO]**

- **A (recomendada)**: el hilo de muestreo arranca en `setup` (después de la base y del contexto de privilegios) y corre mientras la app vive, haya o no alguien mirando. La ventana de 5 min ya tiene datos al abrir Monitor (o el dashboard de C-12), y C-15 (agregado por minuto) y C-24 (RAM > 90 % durante ≥ 60 s) pueden colgarse del mismo flujo sin otro hilo. Costo acotado por D-18 (lecturas de microsegundos a pocos milisegundos por segundo).
- B: muestreo solo mientras hay un suscriptor. Menos trabajo con la ventana cerrada, pero la pantalla arranca vacía, el "pico de 5 min" no significa nada y C-15/C-24 obligarían a reabrir esta decisión. Alternativa si la medición de D-18 sale mal.
- C: B con una ventana precargada de baja frecuencia. Más estados y más pruebas sin beneficio claro. Rechazada.
- El hilo es de prioridad normal: bajarla haría que, justo con la CPU saturada (lo que el usuario quiere ver), el monitor deje de muestrear y muestre huecos falsos.
- Cierre: el hilo consulta una señal de parada en cada espera (`recv_timeout` sobre un canal de `std::sync::mpsc`); al soltarse el `MonitorHub` (fin del proceso) la espera termina. No se hace `join` en la salida (no debe demorar el cierre).

### D-02 — Intervalo y ventana: 1 s y 300 muestras (5 min) **[DUEÑO]**

- **A (recomendada)**: intervalo fijo de 1 000 ms (RN-MON-01: 1–2 s) y ventana de 300 muestras con desalojo por cantidad **y** por antigüedad (> 300 s respecto de la última muestra), para que tras una suspensión larga no queden muestras viejas en el gráfico. El intervalo es un parámetro del bucle (no una constante esparcida) para que C-22 lo relaje en bandeja.
- B: 2 000 ms y 150 muestras. La mitad de lecturas y de mensajes; los gráficos se ven menos "en vivo" y un pico de 1 s puede no verse. Contingencia de D-18.
- Programación por plazo (`next = prev + interval`); si el hilo se atrasó más de un intervalo (bloqueo, suspensión), el próximo plazo se recalcula desde "ahora" (sin ráfaga de muestras para "ponerse al día"). Es una función pura (`next_deadline`) con tests.

### D-03 — Coexistencia con `ActiveJob`: el monitor no es un trabajo

- El muestreo **no** toma `ActiveJob` ni lo consulta: es una lectura de contadores del sistema, no recorre el disco, no escribe nada y no comparte estado con escaneos o limpiezas. Bloquearlo durante un escaneo ocultaría justo la actividad que el usuario quiere ver (el propio escaneo de C-07 en el gráfico de disco); bloquear un escaneo por el monitor no tiene sentido porque no hay recurso compartido.
- `start_monitor_stream`/`stop_monitor_stream` tampoco toman `ActiveJob`: funcionan con cualquier trabajo activo, y ningún trabajo se rechaza por el monitor. Tests: suscripción con un escaneo en curso; escaneo iniciado con una suscripción activa.
- El monitor tampoco depende de la base (D-16): nada se guarda.

### D-04 — Mecanismo de flujo: `Channel` por suscripción, sin eventos globales ni `core:*` **[DUEÑO]**

- **A (recomendada)**: `start_monitor_stream({ onSample: Channel })` registra el canal como **único suscriptor** y responde con la ventana actual y el resumen; desde ahí el hilo entrega cada muestra por ese canal. `stop_monitor_stream({ subscriptionId })` lo quita. Es el mecanismo que Tauri recomienda para flujos ordenados y el que ya usan los escaneos; solo agrega 2 permisos `allow-*` propios, sin `core:*`, sin plugins, sin cambios de CSP (`connect-src ipc:` ya cubre la ruta de datos grandes, aunque cada muestra pesa < 1 KiB).
  - **Sin duplicados ni pérdidas**: copiar la ventana y registrar el suscriptor ocurre bajo el mismo candado que agrega muestras; el flujo empieza en la muestra siguiente a la última de la ventana (`seq` estrictamente creciente).
  - **Un solo suscriptor**: una suscripción nueva reemplaza a la anterior (recarga del webview, remontaje); un `stop` con un id viejo no afecta a la vigente (idempotente). Si `Channel::send` falla, el suscriptor se descarta. El frontend comparte una sola suscripción entre todos sus consumidores (D-10).
- B: eventos de la app (`emit("monitor://sample")` + `listen` en JS). Exige `core:event:allow-listen` y `core:event:allow-unlisten` (o `core:event:default`, que además habilita `emit`/`emit_to` desde el webview): rompe "0 `core:*`" (spec `app-shell` y `security_baseline`), difunde a cualquier destino que escuche ese nombre y saca el contrato del tipado de `ipc.ts`. Rechazada.
- C: polling (`get_monitor_samples(sinceSeq)` cada segundo). Contradice el alcance de C-06 ("sin polling desde la UI"), multiplica invocaciones IPC y desalinea los relojes de UI y muestreo. Rechazada (fallback si A fallara en el smoke).

### D-05 — Puerto `Sensors` rediseñado y reloj monotónico (delta de `domain-ports`)

- `Sensors { fn read(&mut self) -> SensorReading }` (infalible como un todo; cada métrica falla por separado):

```
SensorReading {
  cpu_pct:         Reading<f32>,              // 0..=100, calculado por la fuente entre lecturas
  memory:          Reading<MemoryReading>,    // used_bytes, total_bytes (used <= total)
  volume:          Reading<VolumeReading>,    // total_bytes, free_bytes del volumen del sistema
  disk_io:         Reading<DiskIoCounters>,   // bytes leídos/escritos ACUMULADOS del volumen del sistema
  disk_active_pct: Reading<f32>,              // 0..=100, tiempo activo calculado por la fuente
}
Reading<T> = Result<T, Unavailable>;  Unavailable = WarmingUp | SourceError | NotSupported
```

- Las tasas (bytes/s) las calcula el dominio a partir de contadores acumulados y del reloj monotónico: así los huecos y los contadores que retroceden se prueban con dobles. CPU y tiempo activo los entrega la fuente ya calculados (PDH necesita dos colecciones; la primera es `WarmingUp`).
- Puerto nuevo `MonotonicClock { fn now_ms(&self) -> u64 }` (adaptador sobre `std::time::Instant`), con doble manual que avanza a pedido. El reloj de pared sigue siendo `Clock`.
- `FakeSensors` pasa a guionar `SensorReading`; se adaptan sus tests existentes (mecánico).

### D-06 — Fuentes y dependencias: `sysinfo` + `VolumeSpace` + PDH para el tiempo activo **[DUEÑO]**

| Métrica | Fuente | Notas |
|---|---|---|
| CPU total % | `sysinfo::System::refresh_cpu_usage` + `global_cpu_usage` | primera lectura `WarmingUp` |
| Memoria usada/total | `sysinfo::System::refresh_memory_specifics(ram)` | usada = total − disponible |
| Espacio del volumen | `WindowsVolumeSpace` (C-07) sobre la raíz del volumen del sistema | misma raíz que resuelve C-07 |
| Lectura/escritura acumuladas | `sysinfo::Disks` (solo el disco cuyo punto de montaje es la raíz del sistema), `refresh_specifics(io_usage)` + `usage().total_*` | `IOCTL_DISK_PERFORMANCE` |
| Tiempo activo % | PDH `\LogicalDisk(C:)\% Idle Time` con `PdhAddEnglishCounterW` → `100 − idle` acotado a 0..=100 | primera colección `WarmingUp` |

- **A (recomendada)**: lo anterior. Sin dependencias nuevas. Se declara la feature `Win32_System_Performance` de `windows` en nuestro `Cargo.toml` (hoy llega solo por unificación desde `sysinfo`; declararla evita depender de eso). Verificado en la fuente: `Cargo.lock` no cambia; si cambiara, DETENER y consultar. `PdhAddEnglishCounterW` evita nombres de contadores traducidos (Windows en español). `% Idle Time` se usa porque `% Disk Time` puede pasar de 100.
  - `unsafe` nuevo solo en `adapters/windows/pdh.rs` (abrir consulta, agregar contador, colectar, leer valor formateado, cerrar consulta en `Drop`), mínimo y con `// SAFETY:`; se agrega a `UNSAFE_ALLOWED_FILES`.
- B: solo `sysinfo` (sin tiempo activo; disco = lectura/escritura en MB/s). Cero `unsafe` nuevo, pero pierde la métrica que el usuario reconoce ("disco al 100 %") y la que la spec actual de `domain-ports` ya nombra. Alternativa si el dueño no quiere `unsafe` nuevo.
- C: todo por PDH (CPU, memoria, disco). Más `unsafe` y duplica lo que `sysinfo` ya resuelve. Rechazada.
- D: crate nuevo (p. ej. wrappers de PDH o WMI). Dependencia nueva sin necesidad. Rechazada.
- La feature `component` de `sysinfo` (WMI de temperaturas) no se usa aquí (C-14).

### D-07 — Tasas, calentamiento y huecos (dominio)

- `MonitorEngine` (estado: `seq`, lectura anterior, instantes monotónico y de pared anteriores) convierte cada `SensorReading` en una `MonitorSample { seq, at: EpochMillis, gap_before, cpu_pct, mem_used, mem_total, volume_total, volume_free, disk_active_pct, disk_read_bps, disk_write_bps }`, cada campo `Option`.
- **Hueco**: entre dos muestras hay hueco si el delta monotónico supera 2,5 × intervalo, o si el delta de pared difiere del monotónico en más de 2 000 ms, o si la pared retrocede. Así se detecta la suspensión tanto si `Instant` cuenta el tiempo suspendido como si no (comportamiento a confirmar en el smoke).
- **Tras un hueco**: la muestra lleva `gap_before = true`; CPU, tiempo activo y tasas se descartan (`after_gap`) porque promedian el período suspendido; memoria y espacio se conservan (son instantáneos). Si la pared retrocedió, la ventana se vacía (no se puede ordenar por tiempo).
- **Tasas**: `(contador − anterior) × 1000 / Δmonotónico`; contador que retrocede (reinicio del contador) o Δ = 0 → ausente (`source_error`); primera lectura → `warming_up`.
- Nada se interpola ni se rellena con 0.

### D-08 — Lista de procesos: fuera de alcance **[DUEÑO]**

- **A (recomendada)**: no se muestran procesos en C-06 (el alcance de `CHANGES.md` son CPU, RAM y disco). KB 12 menciona "procesos" en el Monitor avanzado: queda como seguimiento propio (`/opsx:propose` aparte, v0.2) porque (1) refrescar procesos con `sysinfo` cuesta bastante más que las métricas globales y pone en riesgo RN-MON-06; (2) en over-the-shoulder la lista expondría nombres y líneas de comando de la sesión de otra cuenta y hay que decidir su privacidad; (3) amplía la superficie IPC.
- B: top 5 procesos por CPU y memoria (nombre, sin línea de comando), refrescados cada 5 s. Responde mejor "qué satura mi equipo", con los costos de (1)–(3). Posible si el dueño lo prioriza; requiere reabrir specs.

### D-09 — Agregación y niveles llanos **[DUEÑO]**

- `summarize(window, now)` en el dominio, por métrica: `current` (última muestra disponible), `avg60` (promedio de las muestras disponibles de los últimos 60 s), `peak` (máximo de la ventana) y `level`. Ausentes no cuentan; sin muestras disponibles → `level = unknown` y valores `null`.
- Umbrales (**Suposición**, se ajustan con el dueño; alineados con los defaults de C-24 donde existen):

| Métrica | Base del nivel | Normal | Alto | Muy alto |
|---|---|---|---|---|
| CPU | `avg60` | < 70 % | 70–89,9 % | ≥ 90 % |
| Memoria | `avg60` de usada/total | < 80 % | 80–89,9 % | ≥ 90 % |
| Disco (tiempo activo) | `avg60` | < 60 % | 60–89,9 % | ≥ 90 % |
| Espacio del volumen | `current` usado/total | < 90 % | — | "casi lleno" ≥ 90 % |

- El promedio de 60 s evita que un pico de un segundo cambie el texto del modo simple (sin histéresis extra). Lectura/escritura en bytes/s no tienen nivel (no hay un límite natural).
- El resumen también cuenta los huecos de la ventana y el costo medio de lectura (`avg_read_micros`, D-18), y da la disponibilidad de cada métrica en la última muestra (`ok`, `warming_up`, `after_gap`, `source_error`, `not_supported`).

### D-10 — `MonitorHub` y bucle de muestreo

- `commands/monitor.rs` gestiona `MonitorHub` como estado de Tauri: `Arc<Mutex<HubState>>` con la ventana (`RingBuffer<MonitorSample>` de capacidad 300), el último resumen, el suscriptor (`id` + función de envío) y el estado del muestreo (`running` / `failed`). El dominio no conoce `Channel`: el hub envuelve el canal en una función `FnMut(&MonitorEventDto) -> Result<(), ()>`.
- El bucle (`run_sampler`) es genérico sobre `Sensors`, `Clock`, `MonotonicClock` y una espera inyectable (`Ticker`: real con `recv_timeout`, doble que avanza un reloj falso), así que se prueba sin hilos ni `sleep`. Cada ciclo: esperar el plazo → leer sensores (midiendo el costo) → `MonitorEngine` → `RingBuffer` → `summarize` → enviar al suscriptor.
- Frontend: un solo `MonitorProvider` en `AppProviders`; `useMonitor()` cuenta consumidores: la primera pantalla que lo usa abre la suscripción y la última que se desmonta la cierra (Monitor hoy; el dashboard de C-12 después). Con `document.hidden` las muestras se siguen guardando pero no se re-renderiza hasta volver a ser visible.

### D-11 — Fallos del muestreo

- Un sensor que falla solo vuelve ausente su métrica (D-05). Un pánico dentro de un ciclo (p. ej. en `sysinfo`) se captura con `catch_unwind`: el hub pasa a `failed`, envía `{ kind: "stopped", reason: "sampler_failed" }` al suscriptor y el hilo termina. No hay reinicio automático (evita bucles de pánico). Si no se puede crear el hilo, el hub queda `failed` y la app sigue.
- Con el hub `failed`, `start_monitor_stream` responde `monitor_unavailable`. La UI muestra "El monitor no está disponible en esta sesión. Reiniciá la app para volver a intentarlo." en lugar de los valores.
- Sin muestras durante 3 × intervalo (hilo trabado), la UI marca los valores como "sin datos recientes" (temporizador del frontend, probado con `vi.useFakeTimers`); no inventa valores.

### D-12 — PA-01: gráficos propios en SVG, sin librería **[DUEÑO]**

Criterios: actualización cada 1 s de 4–5 series de 300 puntos, bajo costo de render, tema oscuro por tokens, peso del bundle, accesibilidad, CSP `style-src 'self'` sin `unsafe-inline` ni `eval`, testeable en jsdom (la spec pide tests de render de medidores) y dependencia nueva (requiere aprobación).

| Opción | En vivo 1 Hz | Bundle (gzip, aprox.) | Tema oscuro | Accesibilidad | CSP | Tests jsdom | Dependencia |
|---|---|---|---|---|---|---|---|
| **SVG propio** | Sobrado para ≤ 1 500 puntos (un `path` por serie) | ~0 (código propio, estimado < 6 KB) | Clases y tokens del tema | A cargo nuestro (nombre, teclado, tabla) | Por construcción: atributos, sin `style` (patrón de `ProportionBar`) | Completo (DOM SVG) | Ninguna |
| uPlot | Excelente (canvas, pensado para series en vivo) | ~20–25 KB | Por opciones (colores leídos de tokens) | Canvas: requiere tabla/alternativa propia | Sin `eval`; CSS en archivo propio; estilos por CSSOM (a verificar en release) | Requiere doble (sin canvas en jsdom) | Nueva |
| Recharts | Correcto pero re-renderiza todo el SVG en React; hay que apagar animaciones | ~100–150 KB (incluye d3) | Por props | Capa de accesibilidad de teclado | Estilos por props React (CSSOM); verificar | Parcial (`ResponsiveContainer` necesita tamaño) | Nueva |
| visx | Bueno (primitivas SVG) | ~30–40 KB por los paquetes usados | Por props | A cargo nuestro | SVG; sin `eval` | Bueno | Nueva (varias) |
| ECharts | Bueno (canvas), muy completo | ~150–300 KB según tree-shaking | Temas propios | `aria` con descripción automática | Estilos en `cssText`; tooltips con DOM propio; verificar | Requiere doble | Nueva |

- **A (recomendada)**: SVG propio en `src/shared/charts/` (`Meter`, `TimeSeriesChart`, escalas puras). Cumple todos los criterios sin dependencia nueva ni riesgo de CSP, sigue el patrón ya aprobado en C-07 y es totalmente testeable. El costo es escribir ejes, cursor y vista de tabla (acotado: ~300–400 líneas con tests).
- B: uPlot detrás de la misma interfaz `TimeSeriesChart`. Es la opción preferida **si** un consumidor necesita lo que el SVG no da bien (C-15: 24 h con 1 440+ puntos por serie y zoom). Se decide en ese change; el cambio queda local al componente.
- C/D/E: Recharts, visx o ECharts. Más peso o más riesgo de CSP sin ventaja para este caso. Rechazadas para v0.1.
- Efecto en C-07: el treemap diferido puede hacerse en SVG propio (algoritmo squarified puro) cuando se retome; PA-01 deja de bloquearlo.
- Pesos aproximados: se confirman con `npm view`/el bundle real solo si el dueño elige una librería.

### D-13 — Forma visual (skill `dataviz`): medidores y series, no gauges circulares **[DUEÑO]**

- "Gauges" del alcance se interpretan como **lista de medidores** (nombre, barra horizontal y cifra por fila, barras alineadas): la skill `dataviz` desaconseja un gauge o dona por fila para varias proporciones contra su límite. El relleno lleva la severidad (acento → advertencia → peligro) con el nivel también en icono + texto; la pista es un paso más tenue del mismo tono.
- Modo simple: **tarjetas de cifra** (etiqueta, valor, nivel en palabras), sin gráfico.
- Modo avanzado: una **serie temporal por métrica** (pequeños múltiplos, un solo eje Y cada uno; nunca doble eje): CPU %, memoria %, tiempo activo del disco % (eje fijo 0–100) y lectura/escritura en bytes/s (eje con máximo "redondo" sobre el pico de la ventana; dos series → leyenda + etiqueta directa en el extremo). Eje X fijo "hace 5 min → ahora". Huecos = la línea se corta.
- Interacción: cursor vertical + lectura (dibujada dentro del SVG con `transform`, sin `style`) con puntero y con flechas del teclado sobre el gráfico enfocable; botón "Ver tabla" con las últimas 60 muestras. Sin animaciones (respeta `prefers-reduced-motion` por defecto).
- Colores: una serie = `accent`; la segunda serie (escritura) usa un token nuevo `--color-series-2` validado con `scripts/validate_palette.js` de la skill contra la superficie oscura `#0f1115`, más el test `contrast.test.ts` existente. Los textos usan tokens de texto, nunca el color de la serie.

### D-14 — Superficie IPC **[DUEÑO]**

| Comando | Args | Respuesta |
|---|---|---|
| `start_monitor_stream` | `{ onSample: Channel<MonitorEventDto> }` | `MonitorSnapshotDto` |
| `stop_monitor_stream` | `{ subscriptionId }` | `null` |

```
MonitorSnapshotDto = { subscriptionId, intervalMs, windowMs,
  samples: MonitorSampleDto[],               // <= 300, orden por seq creciente
  summary: MonitorSummaryDto | null }        // null si todavía no hay muestras
MonitorEventDto = { kind: "sample", sample: MonitorSampleDto, summary: MonitorSummaryDto }
                | { kind: "stopped", reason: "sampler_failed" }
MonitorSampleDto = { seq, at, gapBefore,
  cpuPct, memUsedBytes, memTotalBytes, volumeTotalBytes, volumeFreeBytes,
  diskActivePct, diskReadBps, diskWriteBps }  // cada métrica: number | null
MonitorSummaryDto = { cpu: MetricSummaryDto, memory: MetricSummaryDto, diskActive: MetricSummaryDto,
  volume: { usedPct: number | null, level: "normal" | "almost_full" | "unknown" },
  availability: { cpu, memory, volume, diskActive, diskIo },   // "ok"|"warming_up"|"after_gap"|"source_error"|"not_supported"
  gapsInWindow, avgReadMicros: number | null }
MetricSummaryDto = { current, avg60, peak: number | null, level: "normal" | "high" | "very_high" | "unknown" }
```

- camelCase; enteros JS seguros (`seq`, bytes, ids `<= 2^53 − 1`); porcentajes con un decimal. `subscriptionId` no entero seguro → `invalid_argument`. Ningún DTO lleva rutas ni datos de usuario. Los campos que agregue C-14 (temperaturas) serán **aditivos**: los decodificadores de `ipc.ts` ignoran campos desconocidos y fallan con faltantes o tipos inválidos.
- Se agregan exactamente 2 permisos `allow-*`: 29 en total, 0 `core:*`, 0 plugins.

### D-15 — Errores nuevos

`AppError` suma `MonitorUnavailable` (`monitor_unavailable`: el muestreo no arrancó o falló). Reutiliza `invalid_argument` e `internal`. Texto en `es.errors`, `message` sin detalles internos. Un sensor que falla nunca es un `AppError` (es una métrica ausente).

### D-16 — Over-the-shoulder y base no disponible

- Las métricas son de la máquina, no de un perfil: el monitor funciona igual en `match` y en `mismatch` (sin bloqueo; el banner global de C-05 ya informa). No se expone ningún dato por usuario (por eso los procesos quedan fuera, D-08).
- El monitor no usa la base: funciona con la base `unavailable`/corrupta (test de comando con el `StoreHandle` caído).

### D-17 — UI: pantalla Monitor **[DUEÑO]**

- Navegación: Inicio, Espacio en disco, Limpieza, Duplicados, **Monitor**, Configuración (orden del mapa de KB 12; icono nuevo). Delta de `app-shell`.
- **Simple** (sin acción primaria: es una pantalla de lectura):
  - Encabezado "Monitor" y una línea: "Así está trabajando tu equipo ahora. Se actualiza cada segundo."
  - Tres tarjetas: **Procesador** ("23 %" + "Normal"), **Memoria** ("62 % · 9,9 de 16 GB" + "Normal"), **Disco C:** ("Activo 12 %" + "Libre 120 GB de 476 GB" + nivel de espacio). Nivel con icono + texto; en alto/muy alto una frase llana ("Tu procesador está muy ocupado desde hace un minuto.").
  - Estados: "Midiendo…" mientras una métrica está en calentamiento; "No disponible" con una explicación breve por motivo; "Sin datos recientes"; monitor no disponible (D-11).
- **Avanzado**: lista de medidores (CPU, memoria, tiempo activo, espacio) con valor actual, promedio 60 s y pico 5 min; los cuatro gráficos de D-13; panel de detalles con intervalo, muestras en la ventana, huecos, costo medio de lectura y disponibilidad por métrica con su código crudo.
- Textos en `es.monitor.*` (sin literales en componentes, `uiLiterals.test.ts`); tamaños y tasas con `format.ts` (B/s → KB/s → MB/s).
- Valores con actualización de 1 Hz **sin** `aria-live` (sería ruido); un anunciador `polite` solo cuando cambia un nivel ("Uso de memoria: alto").

### D-18 — Presupuesto de rendimiento (RN-MON-06, se mide en el smoke) **[DUEÑO]**

- Objetivo: el proceso `pc-optimizer` con el muestreo activo y la pantalla cerrada promedia **< 1 %** de la CPU total durante 5 min; con Monitor avanzado visible, el renderizador de WebView2 promedia **≤ 2 %** (orientativo). Costo medio de lectura de sensores (`avgReadMicros`) **≤ 5 ms** por ciclo.
- Medición: `Get-Counter '\Process(pc-optimizer)\% Processor Time'` (dividido por la cantidad de procesadores lógicos) y el de `msedgewebview2` del renderizador durante 5 min, en debug y, si el dueño lo autoriza, en release; además el `avgReadMicros` del panel de detalles.
- Contingencia si se excede: intervalo de 2 s (D-02 B), menos puntos por gráfico (submuestreo a 150), pausar el muestreo sin suscriptores (D-01 B). Se vuelve al dueño antes de seguir.

### D-19 — Estructura de módulos

```
src-tauri/src/domain/monitor/
  mod.rs          // re-exports + contrato (solo lectura; nada se persiste; ausente nunca es 0)
  model.rs        // MonitorSample, MetricSummary, Level, Availability, límites (300 muestras, 300 s, 60 s)
  ring.rs         // RingBuffer<T> genérico con capacidad
  engine.rs       // MonitorEngine: tasas, calentamiento, huecos, seq
  schedule.rs     // next_deadline (sin ráfagas)
  summary.rs      // summarize + niveles (D-09)
  runner.rs       // run_cycle / run_sampler genéricos sobre puertos y Ticker
  *_tests.rs
src-tauri/src/domain/ports/sensors.rs     // SensorReading, Reading<T>, Unavailable, Sensors (rediseño)
src-tauri/src/domain/ports/clock.rs       // + MonotonicClock
src-tauri/src/domain/ports/fakes.rs       // FakeSensors (nueva forma), FakeMonotonicClock, FakeTicker
src-tauri/src/adapters/clock.rs           // + SystemMonotonicClock (Instant)
src-tauri/src/adapters/windows/pdh.rs     // PdhIdleTime (unsafe mínimo) + mapeo puro de estados
src-tauri/src/adapters/windows/sensors.rs // WindowsSensors (sysinfo + VolumeSpace + PdhIdleTime)
src-tauri/src/commands/monitor.rs         // MonitorHub, spawn_sampler, 2 comandos, DTOs (+ monitor_tests.rs)
src/shared/charts/                        // Meter, TimeSeriesChart, scales.ts, chartText (si hace falta)
src/features/monitor/                     // MonitorProvider, useMonitor, MonitorScreen, MonitorTiles,
                                          // MeterList, LiveCharts, MonitorDetails, monitorText.ts
```

### D-20 — Tests

- Dominio (dobles, sin reloj real ni `sleep`): `RingBuffer` (capacidad, desalojo del más viejo, orden, vacía, capacidad 0 rechazada); `MonitorEngine` (primera lectura en calentamiento; tasas con contadores conocidos; contador que retrocede; Δ = 0; hueco por delta monotónico, por desacuerdo pared/monotónico y por pared hacia atrás con vaciado; tras hueco se descartan CPU, actividad y tasas y se conservan memoria y espacio; `seq` creciente); `next_deadline` (a tiempo, atrasado < 1 intervalo, atrasado > 1 intervalo sin ráfaga); `summarize` (tabla de niveles en los bordes de cada umbral; ausentes ignorados; sin datos → `unknown`; pico de la ventana; huecos contados; desalojo por antigüedad tras suspensión); `run_cycle`/`run_sampler` con `FakeSensors` + `FakeTicker` (N ciclos → N muestras; sensor que falla → métrica ausente; costo medido con el reloj falso; parada).
- Adaptadores: mapeo puro de PDH (`PDH_CSTATUS_VALID_DATA` → valor; `PDH_INVALID_DATA`/primera colección → `WarmingUp`; error de apertura → `NotSupported`; `idle` > 100 → actividad 0); selección del disco del sistema en una lista de discos simulada; `SystemMonotonicClock` no decrece. Lectura real de sensores con `#[ignore = "lee sensores reales de la máquina"]` (se corre a mano con `--ignored`).
- Arquitectura: dominio puro también en `monitor/`; `unsafe` nuevo solo en `adapters/windows/pdh.rs`.
- Comandos: DTOs exactos (snapshot y evento), suscriptor único y reemplazo, `stop` idempotente con id viejo, fallo de envío descarta el suscriptor, sin duplicados ni pérdidas entre ventana y flujo, `monitor_unavailable` con hub `failed`, evento `stopped` ante pánico, suscripción con `ActiveJob` ocupado, escaneo iniciado con suscripción activa, base no disponible, `mismatch`, `subscriptionId` inválido; baseline de capabilities (29 permisos, 0 `core:*`).
- Frontend: decodificadores (formas válidas, nulos permitidos, `kind`/`level`/`availability` desconocidos y números inseguros → error, campos extra ignorados); escalas puras (camino cortado en nulos y huecos, máximo redondo, eje de tiempo fijo); `Meter` (sin `style`, acotado, severidad con icono + texto fuera del SVG); `TimeSeriesChart` (sin `style`, nombre accesible, cursor por teclado, vista de tabla, leyenda con 2 series); `MonitorProvider` (una suscripción compartida, cierre al desmontar el último, ventana inicial + flujo, `stopped`, "sin datos recientes" con temporizadores falsos, sin re-render con `document.hidden`); pantalla simple y avanzada; navegación con 6 entradas; `uiLiterals`; `contrast`.

### D-21 — Smoke guiado (manual, con el dueño) **[DUEÑO]**

Se escribe `smoke-guide.md` en el apply (patrón de C-07). Reglas:

1. Un paso por vez; el dueño confirma antes del siguiente.
2. **Aviso y cuenta atrás real de 10 s** (`for ($i=10; $i -gt 0; $i--) { Write-Host $i; Start-Sleep 1 }`) antes de cada lanzamiento de la UI o de UAC.
3. Sin capturas: verificación por el DOM por CDP solo si el dueño lo delega (texto, roles, atributos), por el panel de detalles y por contadores de Windows (`Get-Counter`). Comparación de valores con el Administrador de tareas la hace el dueño a ojo (tolerancia amplia: fuentes y ventanas distintas).
4. Base temporal con `PCOPT_DATA_DIR` (el monitor no la usa, pero el resto de la app sí); terminal NO elevada salvo la variante elevada.
5. Cargas de prueba acotadas y creadas por el agente en una carpeta scratch: CPU (un bucle de PowerShell en un núcleo, 30 s), disco (escribir y leer un archivo temporal de 512 MiB en `%TEMP%\pcopt-smoke-c06\`), memoria (opcional). Nunca sobre carpetas del dueño.
6. Si algo no coincide, se detiene y se anota.

- Pasos: build y arranque en desarrollo; Monitor en simple (tarjetas, "Midiendo…" al inicio, valores plausibles); avanzado (medidores, 4 gráficos que avanzan, cursor por teclado, "Ver tabla", detalles); carga de CPU → nivel sube tras ~60 s y el anunciador lo dice; carga de disco → lectura/escritura y tiempo activo visibles; un análisis de C-07 sobre una carpeta pequeña mientras el monitor corre (ambos funcionan); salir de Monitor y volver (sin duplicados en el gráfico); **suspensión** (opcional, con autorización: suspender 1 min → hueco visible, sin ceros, la CPU de la primera muestra posterior ausente); medición de D-18 durante 5 min (pantalla cerrada y Monitor avanzado). Opcional con autorización: build de release elevado (`match`) para verificar que los gráficos se dibujan con la CSP de producción.
- Cierre: el agente detiene las cargas; el borrado de `%TEMP%\pcopt-smoke-c06\` y de la base temporal lo hace el dueño con el comando exacto que le entrega el agente (`Remove-Item -LiteralPath … -Recurse -Force`), porque su regla global bloquea `rm -rf`.

### D-22 — Puntos de extensión para otros changes (sin implementarlos)

- **C-12**: consume `MonitorProvider`/`useMonitor()` (misma suscripción) y los componentes `Meter`/tarjeta de cifra; no necesita comandos nuevos.
- **C-14**: sensores lentos (WMI, NVML, ADLX) en su propio hilo y cadencia, publicando la última lectura que el ciclo rápido anexa a la muestra como campos **opcionales nuevos** (aditivo en DTO y decodificadores); nunca bloquean el ciclo de 1 s.
- **C-15**: un segundo consumidor del bucle (agregador por minuto) junto al hub; los huecos ya vienen marcados (`gap_before`) para registrarlos como ausencia.
- **C-22**: el intervalo ya es un parámetro del bucle; relajarlo con la ventana oculta es su decisión.
- **C-24**: `avg60` de memoria y el espacio "casi lleno" ya existen en el resumen; las reglas, el cooldown y las notificaciones son suyos.

## Risks / Trade-offs

- [El muestreo continuo consume CPU aunque nadie mire] → lecturas baratas, costo medido y visible (`avgReadMicros`), presupuesto con medición en el smoke y contingencias (D-18, D-01 B).
- [Re-render de React cada segundo con varios gráficos] → un `path` por serie, sin animaciones, sin re-render con la ventana oculta; contingencia de submuestreo.
- [`unsafe` nuevo de PDH en un proceso elevado] → archivo único en la lista permitida, bloques mínimos con `// SAFETY:`, consulta cerrada en `Drop`, mapeo de estados probado como función pura; alternativa sin PDH (D-06 B).
- [Contadores de rendimiento deshabilitados o corruptos en la máquina] → la métrica queda `not_supported` con su explicación; el resto sigue.
- [`IOCTL_DISK_PERFORMANCE` sin datos en algunos controladores] → `source_error`/`not_supported`, sin ceros.
- [Comportamiento de `Instant` durante la suspensión] → la regla de hueco combina monotónico y pared (D-07); se verifica en el smoke.
- [Un `Channel` viejo tras recargar el webview] → suscriptor único reemplazado por la nueva suscripción; fallo de envío lo descarta; el frontend cierra al desmontar.
- [Pánico en `sysinfo` dentro del hilo] → `catch_unwind`, estado `failed`, aviso en la UI, sin reinicio en bucle.
- [Diferencias con el Administrador de tareas] → fuentes documentadas en KB 13; el smoke compara con tolerancia amplia.
- [Valores que cambian bajo el cursor del gráfico] → el cursor se ancla a la muestra (`seq`), no a la posición en píxeles.

## Migration Plan

- Sin migración de esquema: nada se persiste.
- Capabilities: +2 permisos `allow-*`; sin `core:*`, sin plugins, sin cambios de CSP.
- `Cargo.toml`: feature `Win32_System_Performance` declarada; comentario de `[lints.cargo]` actualizado (`sysinfo` ya tiene consumidor). `Cargo.lock` y `package-lock.json` sin cambios.
- Rollback: revertir la rama; no deja datos ni configuración en la máquina.
- Docs a actualizar en el apply (tarea 1.3): `CHANGES.md` (C-06 en curso + decisiones + notas para C-07, C-12, C-14, C-15, C-22, C-24), KB 02 (comandos y flujo por `Channel`), KB 05 (RN-MON-01 = 1 s, RN-MON-06 con el presupuesto y su medición), KB 06 US-003 (criterios reales), KB 09 (decisión de gráficos), KB 10 (PA-01 resuelta), KB 12 (pantalla Monitor, medidores en vez de gauges, procesos diferidos), KB 13 (fuentes reales, ciclo de vida, huecos).

## Decisiones del dueño (bloquean el apply)

| ID | Decisión | Recomendación |
|---|---|---|
| D-01 | Ciclo de vida | Muestreo siempre activo desde el arranque, prioridad normal |
| D-02 | Intervalo y ventana | 1 s; 300 muestras / 300 s; sin ráfagas tras atrasos |
| D-04 | Flujo y permisos | `Channel` por suscripción (único suscriptor), 2 comandos; sin `emit`/`listen` ni `core:*`; sin polling |
| D-06 | Fuentes y dependencias | `sysinfo` (CPU, memoria, contadores de disco) + `VolumeSpace` + PDH `% Idle Time` con `unsafe` mínimo; feature `Win32_System_Performance` declarada; sin crates nuevos |
| D-08 | Procesos | Fuera de C-06; seguimiento propio |
| D-09 | Niveles | Umbrales de la tabla (CPU 70/90, memoria 80/90, disco activo 60/90, espacio 90) sobre promedio de 60 s |
| D-12 | PA-01 | SVG propio sin librería; uPlot como alternativa preferida si C-15 la necesita |
| D-13 | Forma visual | Medidores lineales y series por métrica en vez de gauges circulares |
| D-14 | IPC | `start_monitor_stream` + `stop_monitor_stream`; 29 permisos |
| D-17 | UI | Entrada "Monitor" antes de Configuración; simple = 3 tarjetas sin acción primaria; avanzado = medidores, 4 gráficos, detalles |
| D-18 | Rendimiento | < 1 % CPU del proceso con pantalla cerrada; ≤ 5 ms por lectura; medición en el smoke |
| D-21 | Smoke | Cargas acotadas en scratch, suspensión y release solo con autorización, cierre con comando para el dueño |

## Desvíos respecto de CHANGES.md / KB

- `CHANGES.md` C-06 "Eventos Tauri" → `Channel` de la invocación (el mecanismo de Tauri para flujos), no `emit`/`listen` (D-04).
- `CHANGES.md` C-06 "`sysinfo`/PDH" → `sysinfo` para casi todo; PDH solo para el tiempo activo del disco (D-06). KB 13 "Disco actividad/throughput: PDH (**Suposición**)" → lectura/escritura por `IOCTL_DISK_PERFORMANCE` vía `sysinfo`.
- `CHANGES.md` C-06 / KB 12 "gauges" → lista de medidores lineales (D-13).
- KB 12 Monitor avanzado "procesos" → diferido (D-08).
- KB 13 "ring buffer ~5 min (**Suposición**)" → 300 muestras de 1 s con desalojo por antigüedad (D-02).
- Spec actual de `domain-ports` "Muestra de sensores" → lecturas crudas con contadores y disponibilidad por métrica (D-05).

## Preguntas abiertas de KB 10 que toca este change

- **PA-01** (gráficos): se decide aquí (D-12); al aprobarse se marca resuelta en KB 10 y se actualiza la nota de C-07 (treemap ya no bloqueado).
- **PA-12** (otros volúmenes): solo el volumen del sistema.
- **PA-08** (Windows 10): PDH, `IOCTL_DISK_PERFORMANCE` y `GlobalMemoryStatusEx` existen en Windows 10; sin diferencias esperadas. Se anota en el smoke si hay una máquina Windows 10 disponible.
- **PA-14** (resuelta): sin bloqueo en mismatch; sin datos por usuario (D-16).
- **IN-01** (simple vs avanzado): pantalla con ambos modos; revisión UX en la tarea 7.7.
- **IN-03** (siempre admin): el monitor no necesita elevación; corre igual elevado.

## Open Questions

- Textos exactos de las frases llanas y de las explicaciones de "no disponible": se ajustan en la revisión UX (tarea 7.7) sin cambiar specs.
- Valor exacto del token `--color-series-2`: lo fija el validador de la skill `dataviz` en la tarea 6.4.
- Cantidad de filas de la vista de tabla (60 por defecto): detalle de UI.

## Apply notes

**Aprobación del dueño (2026-11-16)**: el dueño aprobó TODAS las opciones recomendadas de las decisiones **[DUEÑO]** (D-01, D-02, D-04, D-06, D-08, D-09, D-12, D-13, D-14, D-17, D-18, D-21). Sin cambios en specs ni tasks.

### 1.1 Línea base (antes de tocar código)

| Comando | Resultado |
|---|---|
| `cargo fmt --check` | verde |
| `cargo clippy --all-targets --all-features --locked -- -D warnings` | verde |
| `cargo test --locked` | lib **1087 passed, 21 ignored**; `architecture` 24; `manifest` 5; `no_network_crates` 9; `security_baseline` 22 |
| `cargo test --locked domain::ports` | **91 passed** (red del rediseño de 2.1) |
| `npx vitest run` | **592** tests. Observación: con todos los hilos de Vitest en paralelo falla 1 test distinto por corrida (`emptyRecycleBin.test.tsx`, `cleanupFlow.test.tsx`, timeouts de `waitFor`); aislados (`src/tests/cleanup`) 93/93 y con `--maxWorkers=2` 592/592. Flaky por carga, no una falla preexistente determinista. |
| `npm run typecheck` | verde |
| permisos `allow-*` | 27, 0 `core:*`, 0 plugins |

### 1.2 Tabla de APIs confirmadas (fuente en `~/.cargo/registry`)

| API | Confirmación |
|---|---|
| `sysinfo 0.40.3`: `System::refresh_cpu_usage`, `global_cpu_usage`, `refresh_memory_specifics(MemoryRefreshKind::nothing().with_ram())`, `total_memory`, `available_memory` | presentes en `src/common/system.rs` (`MINIMUM_CPU_UPDATE_INTERVAL` = 200 ms) |
| `Disks::new_with_refreshed_list_specifics`, `Disk::mount_point`, `Disk::refresh_specifics(DiskRefreshKind::nothing().with_io_usage())`, `Disk::usage()` → `DiskUsage { total_read_bytes, total_written_bytes }` | presentes en `src/common/disk.rs` y `mod.rs` |
| `windows 0.62.2` (módulo `Win32::System::Performance`): `PdhOpenQueryW(szdatasource, dwuserdata, *mut PDH_HQUERY) -> u32`, `PdhAddEnglishCounterW(hquery, szfullcounterpath, dwuserdata, *mut PDH_HCOUNTER) -> u32`, `PdhCollectQueryData(hquery) -> u32`, `PdhGetFormattedCounterValue(hcounter, PDH_FMT_DOUBLE, Option<*mut u32>, *mut PDH_FMT_COUNTERVALUE) -> u32`, `PdhCloseQuery(hquery) -> u32`; `PDH_CSTATUS_VALID_DATA = 0`, `PDH_CSTATUS_NEW_DATA = 1`, `PDH_INVALID_DATA = 0xC0000BC6`; `PDH_FMT_COUNTERVALUE { CStatus, Anonymous.doubleValue }` | confirmadas en `windows-0.62.2/src/Windows/Win32/System/Performance/mod.rs`; devuelven `u32` (estado PDH, 0 = `ERROR_SUCCESS`), no `Result`. Feature `Win32_System_Performance` (depende de `Win32_System`). |
| `tauri::ipc::Channel` (`Clone`, `Send`, `send` fuera del comando) | se confirma en la parte 3 (grupo 5) |
| Cambio de `Cargo.toml` | feature `Win32_System_Performance` declarada; `cargo build --locked` OK y `git diff Cargo.lock` vacío |

Nota: `winmd-api-search` (skill) no se invocó para PDH: las firmas se verificaron directamente en el código generado de `windows 0.62.2` que es el que compila el proyecto (fuente más fiable que el metadato).

### Desvíos de implementación (parte 1)

- `MonotonicClock` gana `now_micros()` con implementación por defecto (`now_ms() * 1000`); el reloj real la sobrescribe con `Instant::elapsed().as_micros()`. Motivo: el costo de una lectura (D-18, ≤ 5 ms) no se puede medir con una resolución de 1 ms.
- Se agrega `domain/monitor/window.rs` (`SampleWindow`: capacidad 300 y desalojo por antigüedad, con `clear`), no listado en D-19, para que el resumen del muestreador y el `MonitorHub` de la parte 3 usen la misma regla.
- `RingBuffer::bounded(capacity)` (capacidad ≥ 1, sin error) además de `new` (que rechaza 0), para la capacidad constante de la ventana sin `unwrap`.
- `Sampler::run_cycle` devuelve `Option<CycleResult>` (solo `None` si el resumen no se puede calcular, caso imposible tras agregar la muestra).
- `read_micros` viaja en cada `MonitorSample` (el resumen promedia el costo sobre la ventana); no forma parte del DTO de muestra.
- El motor fuerza `WarmingUp` en la primera lectura aunque la fuente informe un valor (la fuente real, `sysinfo`, devuelve 0 en la primera lectura de CPU: ese 0 nunca debe publicarse).
- Una variable llamada `oldest_kept` hacía fallar el test de arquitectura `kept_should_only_be_assigned_by_the_group_resolution` (heurística léxica sobre `kept =`); se renombró a `age_floor`.

### Evidencia RED (parte 1)

- **2.1** `cargo test --locked --lib domain::ports` con los tests nuevos y el código viejo: `error[E0432]: unresolved imports crate::domain::ports::SensorReading, crate::domain::ports::Unavailable` / `unresolved imports ...sensors::DiskIoCounters, MemoryReading, Reading, VolumeReading` / `error[E0425]: cannot find type SensorSample` (símbolo viejo). GREEN: `test result: ok. 91 passed` (conteo de 1.1; 2 tests reemplazados por 2).
- **2.2** `cargo test --locked --lib clock`: `error[E0432]: unresolved import crate::domain::ports::MonotonicClock` / `error[E0433]: cannot find type SystemMonotonicClock in this scope`. GREEN: `5 passed` (`FakeMonotonicClock` +1 500 ms y 250 + 4 000 = 4 250; real no decrece).
- **3.1** `cargo test --locked --lib domain::monitor::ring`: `error[E0433]: cannot find type RingBuffer in this scope`. GREEN: `6 passed` (capacidad 3 con 4 inserciones, vacía, capacidad 0 → `RingError::ZeroCapacity`, `retain`, `clear`) y `bounded` (+1 después).
- **3.2** `cargo test --locked --lib domain::monitor::engine`: `error[E0425]: cannot find type Tick in this scope` / `cannot find type Availability`. GREEN: `9 passed` (primera lectura `warming_up`, 1 000 000 → 3 000 000 en 1 000 ms = 2 000 000 B/s, escala con Δ 1 500 ms, contador que retrocede, Δ = 0, `seq` creciente, fuente que falla, fuera de rango, costo).
- **3.3** mismos tests con huecos: `3 FAILED` (`assertion failed: s.gap_before`, `assertion failed: out.sample.gap_before` ×2; `test result: FAILED. 11 passed; 3 failed`). GREEN: `14 passed` (Δ 3 000 → hueco; Δ monotónico 1 000 con pared 600 000 → hueco; pared hacia atrás → hueco + vaciar; 1 200/1 150 y 2,5 intervalos exactos → sin hueco; reanuda tasas tras el hueco).
- **3.4** `cargo test --locked --lib domain::monitor::schedule`: `error[E0425]: cannot find function next_deadline in this scope`. GREEN: `3 passed` (a tiempo; atraso 400 ms → `prev + interval`; atraso 3 500 ms → `now + interval`).
- **3.5** `cargo test --locked --lib domain::monitor::summary`: `error[E0425]: cannot find type MonitorSample in this scope` (tipo de resumen aún sin implementar). GREEN: `9 passed` (bordes 69,9/70/89,9/90 de CPU; memoria 85 % alto; disco 95 % muy alto; espacio 92 % y 89,9 %; ausentes ignorados y todo ausente → `unknown`; pico 100 con nivel normal; huecos, costo medio y disponibilidad; desalojo por antigüedad tras 10 min).
- **3.6** `cargo test --locked --lib domain::monitor::runner`: `error[E0432]: unresolved imports crate::domain::ports::fakes::FakeTicker, FakeWallClock` / `error[E0425]: cannot find type Sampler`. GREEN: `domain::monitor` completo `41 passed` (10 ciclos → `seq` 1..=10 separados 1 000 ms; cadencia sin deriva; costo 2 ms → 2 000 µs; sensor con `SourceError` en un ciclo; parada del ticker; 301 ciclos → 300 muestras; pared hacia atrás vacía la ventana).

### Conteos tras la parte 1

`cargo test --locked`: lib **1132 passed** (+45), 21 ignored; `architecture` 24; `manifest` 5; `no_network_crates` 9; `security_baseline` 22. `cargo fmt --check` y clippy `-D warnings` verdes. `Cargo.lock` sin cambios.

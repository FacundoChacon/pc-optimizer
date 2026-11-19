# Guía del smoke guiado — live-monitor (C-06)

> Esta guía la conduce el orquestador con el dueño (tarea 8.3). El archivo solo documenta: no ejecuta nada por sí mismo.
> Patrón de C-07. Cubre todos los puntos de D-21 y la medición de D-18.

## Reglas

1. **Un paso por vez.** El dueño confirma ("OK") antes de pasar al siguiente.
2. **Aviso y cuenta atrás real de 10 s** antes de cada lanzamiento de la UI o de UAC, con exactamente:
   `for ($i=10; $i -gt 0; $i--) { Write-Host $i; Start-Sleep 1 }`
3. **Sin capturas de pantalla.** Se verifica por el DOM (CDP, solo si el dueño lo delega: texto, roles y atributos), por el panel de detalles de Monitor y por contadores de Windows (`Get-Counter`). La comparación de valores con el Administrador de tareas la hace el dueño a ojo (tolerancia amplia: fuentes y ventanas distintas).
4. **Base temporal** con `PCOPT_DATA_DIR=%TEMP%\pcopt-smoke-c06\data` (el monitor no usa la base, pero el resto de la app sí). Terminal **NO elevada**, salvo el paso 11 (release elevado).
5. **Cargas de prueba** creadas por el agente solo en `%TEMP%\pcopt-smoke-c06\`: CPU (un bucle en un núcleo, 30 s), disco (archivo de 512 MiB escrito y leído) y, opcional, memoria. Nunca sobre carpetas del dueño.
6. **Suspensión y build de release solo con autorización explícita del dueño en ese paso.**
7. **Si algo no coincide con lo esperado, se detiene y se anota** (qué se vio, qué se esperaba, paso). No se sigue.

## Preparación (agente, antes del paso 1)

```powershell
$scratch = Join-Path $env:TEMP 'pcopt-smoke-c06'
New-Item -ItemType Directory -Force -LiteralPath $scratch, (Join-Path $scratch 'data') | Out-Null
$env:PCOPT_DATA_DIR = Join-Path $scratch 'data'
```

### Carga de CPU (30 s, un núcleo)

```powershell
$sw = [Diagnostics.Stopwatch]::StartNew()
while ($sw.Elapsed.TotalSeconds -lt 30) { $x = 1.0; for ($i = 0; $i -lt 200000; $i++) { $x = $x * 1.0000001 } }
```

Para llevar el nivel a "alto" (promedio de 60 s ≥ 70 %) hace falta cargar varios núcleos o repetirla en paralelo (`Start-Job` × cantidad de procesadores lógicos) por ~70 s; el agente lo ajusta al equipo y avisa antes.

### Carga de disco (512 MiB escritos y leídos)

```powershell
$file = Join-Path $scratch 'carga.bin'
$buffer = New-Object byte[] (4MB); (New-Object Random).NextBytes($buffer)
$out = [IO.File]::Create($file); 1..128 | ForEach-Object { $out.Write($buffer, 0, $buffer.Length) }; $out.Flush($true); $out.Dispose()
$in = [IO.File]::OpenRead($file); $read = New-Object byte[] (4MB); while ($in.Read($read, 0, $read.Length) -gt 0) { }; $in.Dispose()
```

## Medición de D-18 (presupuesto de rendimiento)

Objetivo: proceso `pc-optimizer` **< 1 %** de la CPU total con el muestreo activo y la pantalla cerrada, durante 5 min; renderizador de WebView2 **≤ 2 %** (orientativo) con Monitor avanzado visible; `avgReadMicros` **≤ 5 ms**.

```powershell
$n = [int]$env:NUMBER_OF_PROCESSORS   # procesadores lógicos
$proc = (Get-Counter '\Process(pc-optimizer*)\% Processor Time' -SampleInterval 5 -MaxSamples 60).CounterSamples
($proc | Measure-Object CookedValue -Average).Average / $n   # promedio % de la CPU total
```

Para el renderizador: identificar los `msedgewebview2` hijos de `pc-optimizer` (`Get-CimInstance Win32_Process -Filter "Name='msedgewebview2.exe'"`, `ParentProcessId` = PID de la app, línea de comandos con `--type=renderer`) y medir `\Process(msedgewebview2*)\% Processor Time` solo de ese PID (`ID Process`), con la misma fórmula. Si D-18 no se cumple: DETENER y volver al dueño con las contingencias (intervalo de 2 s, submuestreo a 150 puntos, pausar sin suscriptores).

## Pasos

| # | Paso | Resultado esperado | Autorización |
|---|---|---|---|
| 1 | Cuenta atrás de 10 s y `npm run tauri dev` (desarrollo, no elevado, base temporal). | La app abre sin errores; Inicio visible. | **OK del dueño** (lanza la UI) |
| 2 | Monitor en modo **simple**. | Tres tarjetas (Procesador, Memoria, Disco). Al inicio "Midiendo…" en el procesador y luego un valor; ningún "0 %" falso; sin botones primarios. Valores plausibles frente al Administrador de tareas. | — |
| 3 | Cambiar a modo **avanzado** (Configuración) y volver a Monitor. | Medidores (4) con actual, promedio 60 s y pico; 4 gráficos que avanzan cada segundo; panel de detalles con intervalo 1 s, muestras, huecos y costo medio. | — |
| 4 | Teclado en un gráfico: Tab hasta él, flechas, Inicio/Fin, Esc; botón "Ver tabla". | Cursor con hora y valor; foco visible; tabla con las últimas 60 filas y "No disponible" donde falte. | — |
| 5 | Carga de CPU (script). | Procesador sube; el nivel pasa a Alto/Muy alto tras ~60 s y el anunciador lo dice ("Procesador: alto"); vuelve a Normal al terminar. | **OK del dueño** (carga) |
| 6 | Carga de disco (script). | Lectura/escritura en el gráfico en KB/s–MB/s y subida de la actividad del disco. | **OK del dueño** (carga) |
| 7 | Con Monitor corriendo, un análisis de Espacio en disco (C-07) sobre una carpeta pequeña (en `%TEMP%\pcopt-smoke-c06\`). | Ambos funcionan a la vez; el monitor no se interrumpe. | — |
| 8 | Salir de Monitor y volver (2 veces). | Sin duplicados ni saltos raros en los gráficos; una sola suscripción. | — |
| 9 | **Suspensión** (opcional): suspender ~1 min y reanudar. | Hueco visible (línea cortada), sin ceros; la CPU de la primera muestra posterior ausente ("Midiendo…"). | **Autorización explícita del dueño** (suspende el equipo) |
| 10 | Medición de D-18 (5 min con la pantalla de Monitor cerrada; 5 min con avanzado visible). | Promedios dentro del presupuesto; `avgReadMicros` ≤ 5 ms en el panel de detalles. | — |
| 11 | **Release elevado** (opcional): `npm run tauri build` y ejecución del instalador/ejecutable (UAC). | Los gráficos se dibujan con la CSP de producción (sin errores de CSP en la consola). | **Autorización explícita del dueño** (build de release + UAC; cuenta atrás de 10 s) |
| 12 | Cierre: el agente detiene las cargas y cierra la app. | Sin procesos residuales (`powershell` de carga, `pc-optimizer`). | — |

## Cierre y limpieza

El agente detiene las cargas y entrega al dueño el **comando exacto** para borrar los temporales (el dueño lo corre; su regla global bloquea `rm -rf`):

```powershell
Remove-Item -LiteralPath "$env:TEMP\pcopt-smoke-c06" -Recurse -Force
```

Los resultados y mediciones de cada paso se anotan en `design.md` §Apply notes.

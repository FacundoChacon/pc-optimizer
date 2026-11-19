# Guía del smoke guiado — dashboard-simple-advanced (C-12)

> Esta guía la conduce el orquestador con el dueño (tarea 10.3). El archivo solo documenta: no ejecuta nada por sí mismo.
> Patrón de C-06. Cubre todos los puntos de D-17.

## Reglas

1. **Un paso por vez.** El dueño confirma ("OK") antes de pasar al siguiente.
2. **Aviso y cuenta atrás real de 10 s** antes de cada lanzamiento de la UI o de UAC, con exactamente:
   `for ($i=10; $i -gt 0; $i--) { Write-Host $i; Start-Sleep 1 }`
3. **Sin capturas de pantalla.** Se verifica por el DOM vía CDP (texto, roles, atributos, `data-variant`, foco activo) y por los ojos del dueño. La app de desarrollo expone WebView2 en `127.0.0.1:9222` si se lanza con `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222` (solo en desarrollo; el release lo ignora). Si `connectOverCDP` falla, se usa un driver CDP propio por WebSocket (`Runtime.evaluate`).
4. **Base temporal** con `PCOPT_DATA_DIR=%TEMP%\pcopt-smoke-c12\data` y `npm run tauri dev` (build de desarrollo: el release ignora `PCOPT_DATA_DIR`). Terminal **NO elevada**.
5. **Archivos de prueba solo en `%TEMP%\pcopt-smoke-c12\`.** Nunca sobre carpetas del dueño.
6. **Una limpieza real solo con autorización explícita del dueño en ese paso** y solo de la categoría "Temporales del usuario", con archivos de prueba que crea el agente en la carpeta temporal del usuario con nombre `pcopt-smoke-c12-*` (se mueven a la Papelera; reversible). Sin autorización, el paso se cierra con "Cancelar" y el historial con datos se cubre con la siembra sintética.
7. **Si algo no coincide con lo esperado, se detiene y se anota** (qué se vio, qué se esperaba, paso). No se sigue.

## Preparación (agente, antes del paso 1)

```powershell
$scratch = Join-Path $env:TEMP 'pcopt-smoke-c12'
New-Item -ItemType Directory -Force -LiteralPath $scratch, (Join-Path $scratch 'data') | Out-Null
$env:PCOPT_DATA_DIR = Join-Path $scratch 'data'
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = '--remote-debugging-port=9222'
```

### Archivos de prueba para la limpieza real (solo si el dueño autoriza el paso 9)

Cinco `.tmp` de 1 MiB con `LastWriteTime` de hace 3 días, en la carpeta temporal del usuario (categoría "Temporales del usuario"):

```powershell
$fixtures = Join-Path $env:TEMP 'pcopt-smoke-c12-fixtures'
New-Item -ItemType Directory -Force -LiteralPath $fixtures | Out-Null
1..5 | ForEach-Object {
  $file = Join-Path $fixtures "pcopt-smoke-c12-$_.tmp"
  [IO.File]::WriteAllBytes($file, (New-Object byte[] 1MB))
  (Get-Item -LiteralPath $file).LastWriteTime = (Get-Date).AddDays(-3)
}
```

### Siembra del historial sintético (opción de D-17; solo base temporal)

Escribe 154 filas sintéticas (25 limpiezas: una de 130 archivos, 23 de un archivo y un vaciado de la Papelera) **solo** en `PCOPT_DATA_DIR\data.db`. El test se niega si la variable falta o apunta fuera de la carpeta temporal. La app NO debe estar abierta durante la siembra.

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --locked seed_smoke_database -- --ignored --nocapture
```

(se corre con `PCOPT_DATA_DIR` ya definida, en la misma terminal).

## Pasos

| # | Paso | Resultado esperado | Autorización |
|---|---|---|---|
| 1 | Cuenta atrás de 10 s y `npm run tauri dev` (desarrollo, no elevado, base temporal vacía, StrictMode activo como en `main.tsx`). | La app abre sin errores; Inicio visible. | **OK del dueño** (lanza la UI) |
| 2 | Inicio en modo **simple**. | Tres tarjetas: Espacio y Memoria con valores plausibles; Basura "Todavía no buscaste"; exactamente una acción primaria ("Limpiar ahora", `data-variant="primary"` único). Estados "No disponible" con su motivo si el dato falta; ningún valor en 0 falso. | — |
| 3 | Historial con la base vacía (navegar desde la barra lateral). | "Todavía no limpiaste nada con la app"; sin lista, sin botón "Ver más", sin botón primario. | — |
| 4 | "Limpiar ahora" **sin escaneo** (camino `scan_then_preview`, también valida el fix 1 de StrictMode en desarrollo). | Navega a Limpieza, el foco queda en el encabezado, busca (progreso visible) y al terminar abre el preview con el foco en "Cancelar". Con StrictMode el preview se abre igual. | — |
| 5 | En el preview, **Esc**. | El diálogo se cierra, el foco vuelve a "Limpiar" y no se movió nada (0 `confirm_cleanup`). | — |
| 6 | Volver a Inicio y "Limpiar ahora" **antes de 60 min** (camino `preview`). | Preview directo, sin nueva búsqueda, foco en "Cancelar"; "Cancelar" lo cierra sin cambios. | — |
| 7 | "Limpiar ahora" con búsqueda nueva y **salir de Limpieza** (Monitor) mientras busca; volver a Limpieza. | Sin diálogo al volver y "Limpiar" disponible: el pedido se descartó al salir. | — |
| 8 | Búsqueda de duplicados en curso (Duplicados, carpeta de prueba en `%TEMP%\pcopt-smoke-c12\`) y volver a Inicio. | "Limpiar ahora" deshabilitado con el motivo visible y asociado (`aria-describedby`); se rehabilita al terminar. | — |
| 9 | **Limpieza real autorizada** (opcional; **solo si el dueño revisó qué contiene la categoría**): "Temporales del usuario" se mueve entera (todo o nada), así que mueve también los temporales reales del dueño, no solo los 5 archivos de prueba; en el smoke del 2026-11-19 tenía 4 530 archivos reales (1,2 GB) y se omitió. Si se hace: "Limpiar ahora" -> preview -> confirmar. | Resumen de C-09 con los movidos a la Papelera; Historial (simple) muestra la limpieza con fecha y frase llana, sin rutas ni códigos; Historial (avanzado) la muestra con las rutas. Los archivos se pueden restaurar desde la Papelera. | **Autorización explícita del dueño** (mueve archivos reales a la Papelera) |
| 10 | Cerrar la app y **sembrar el historial sintético** (comando de la preparación); relanzar con cuenta atrás de 10 s. | La app abre con la base sembrada. | **OK del dueño** (relanza la UI) |
| 11 | Historial en modo **simple**. | 20 limpiezas con fecha y frase llana, sin rutas ni códigos; "Ver más" agrega las 5 restantes sin repetir y desaparece al final; sin botón primario ni "Ver archivos". | — |
| 12 | Cambiar a modo **avanzado** (desde Configuración) y abrir Historial. | Cada limpieza tiene "Ver archivos" (descrito por su fecha y frase) y conteos por resultado con su código. Desplegar la de 130 archivos: 100 filas con ruta completa y "Cargar más" agrega 30; plegar y desplegar no duplica filas. El vaciado de la Papelera también se despliega. | — |
| 13 | Inicio en modo **avanzado**. | Medidores (CPU/Memoria), gráfico de **dos series** con leyenda y eje de 5 min, tabla de basura (revisar a 960 px de ancho: sin recorte, con desplazamiento horizontal si hace falta), resumen de disco y las **últimas 3 limpiezas**. | — |
| 14 | Alternar modo (simple ↔ avanzado) varias veces **desde Configuración** (Inicio no tiene alternancia). | La pantalla no se pierde; el foco no cae en `<body>`; siempre una sola acción primaria. | — |
| 15 | Teclado: Tab por Inicio hasta "Limpiar ahora" y Enter; Esc en el preview; Tab por Historial hasta "Ver archivos" y Enter/Espacio. | Foco siempre visible y con orden lógico; Inicio → diálogo → Limpieza sin perder el foco. | — |
| 16 | Cierre: el agente cierra la app y detiene `npm run tauri dev`. | Sin procesos residuales (`pc-optimizer`, `node`/`vite`, `msedgewebview2` hijos). | — |

## Cierre y limpieza

El agente detiene los procesos, verifica que no queden residuales y entrega al dueño el **comando exacto** para borrar los temporales (el dueño lo corre; su regla global bloquea `rm -rf`):

```powershell
Remove-Item -LiteralPath "$env:TEMP\pcopt-smoke-c12" -Recurse -Force
Remove-Item -LiteralPath "$env:TEMP\pcopt-smoke-c12-fixtures" -Recurse -Force
```

Los archivos de prueba movidos a la Papelera (paso 9) los vacía o restaura el dueño desde la Papelera de Windows. Los resultados de cada paso se anotan en `design.md` §Apply notes.

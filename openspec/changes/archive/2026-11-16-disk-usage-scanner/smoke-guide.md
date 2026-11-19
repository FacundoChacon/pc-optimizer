# Guía del smoke guiado de C-07 (D-25)

> Esta guía la ejecuta el agente principal **junto con el dueño**. El agente que implementó el change NO corrió nada de esto: no lanzó la app, no disparó UAC, no creó las fixtures, no analizó el disco real y no tocó las carpetas del dueño.

## Reglas del smoke

1. **Paso a paso**: un paso por vez; el dueño confirma en el chat antes de pasar al siguiente.
2. **Aviso de 10 s** antes de CADA lanzamiento de la UI o de UAC. El agente escribe en el chat "Lanzo la app en 10 segundos" y hace una cuenta atrás real de 10 s (`for ($i=10; $i -gt 0; $i--) { Write-Host $i; Start-Sleep 1 }`) antes del comando de lanzamiento.
3. **Sin capturas de pantalla**: el agente verifica con la base en solo lectura, el sistema de archivos y, si el dueño lo delega, el DOM por CDP (solo texto, roles y estados). El diálogo nativo del selector y la ventana del Explorador no son del webview: los maneja el dueño.
4. **Raíz verificada antes de cada "Analizar"**: antes de pulsar "Analizar", el agente lee en el DOM (o el dueño confirma) que la raíz elegida es la carpeta del smoke. **`C:` completo y cualquier carpeta real del dueño NUNCA se analizan sin su autorización explícita en ese paso.**
5. Base temporal con `PCOPT_DATA_DIR` (nunca la base real); terminal NO elevada salvo la variante elevada (paso 14).
6. Este change nunca borra, mueve ni modifica archivos del usuario: si algo del smoke cambia el contenido de las fixtures (salvo las que crea y restaura el propio agente), es un defecto: detenerse y anotarlo.
7. Si algo no coincide con lo esperado, se detiene y se anota; no se improvisa.

## Datos de apoyo

- Fixture principal: `%USERPROFILE%\Downloads\pcopt-smoke-c07`. Segunda fixture (cancelación): `%USERPROFILE%\Downloads\pcopt-smoke-c07-many`. Destino de la junction (fuera de la principal): `%USERPROFILE%\Downloads\pcopt-smoke-c07-outside`.
- Base temporal: `PCOPT_DATA_DIR=%LOCALAPPDATA%\Temp\pcopt-smoke-c07\data`.
- Lectura de la base en solo lectura (el agente, sin escribir): `python -I -c "import sqlite3,sys; c=sqlite3.connect('file:'+sys.argv[1]+'?mode=ro', uri=True); [print(r) for r in c.execute(sys.argv[2])]" <ruta a data.db> "<consulta>"`. Consultas útiles:
  - `select * from folder_scan_result` (raíz, volumen, contadores, `folders_truncated`).
  - `select id,parent_id,path,depth,size_bytes,direct_bytes,file_count,dir_count,access,opaque_reason,system,junk_hint from folder_snapshot order by id`.
  - `select id,folder_id,path,size_bytes,modified_at from large_file order by size_bytes desc, id`.
  - `select * from folder_scan_issue` y `select id,kind,status from scan order by id`.
- **Conducir la UI sin capturas (opcional, CDP)**: lanzar la app con `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222` (junto con `PCOPT_DATA_DIR`) y, desde una carpeta scratch con `playwright-core`, `chromium.connectOverCDP("http://127.0.0.1:9222")`; leer `innerText`, roles y estados. El selector nativo y el Explorador los maneja el dueño.
- Arranque en desarrollo sin elevar: Vite aparte (`npm run dev`) y `src-tauri\target\debug\pc-optimizer.exe` con `PCOPT_DATA_DIR` fijado.

## Fixture (la crea el agente en el paso 1; el script NO se corrió al escribir esta guía)

```powershell
$root    = Join-Path $env:USERPROFILE 'Downloads\pcopt-smoke-c07'
$outside = Join-Path $env:USERPROFILE 'Downloads\pcopt-smoke-c07-outside'
$many    = Join-Path $env:USERPROFILE 'Downloads\pcopt-smoke-c07-many'
New-Item -ItemType Directory -Force $root, "$root\big", "$root\medium\nested", "$root\small", "$root\empty", "$root\locked", $outside, $many | Out-Null

function New-File([string]$path, [long]$len, [int]$ageDays = 0) {
  $fs = [IO.File]::Create($path); $fs.SetLength($len); $fs.Close()
  if ($ageDays -gt 0) { (Get-Item $path).LastWriteTime = (Get-Date).AddDays(-$ageDays) }
}
New-File "$root\big\pcopt-smoke-big.bin" (64MB) 30
1..3 | ForEach-Object { New-File "$root\medium\nested\pcopt-smoke-m$_.bin" (8MB) 10 }
1..100 | ForEach-Object { New-File ("$root\small\s{0:000}.bin" -f $_) (4KB) 5 }
New-File "$root\recent.bin" (2MB) 0
New-File "$root\locked\pcopt-smoke-secret.bin" (16MB) 5
New-File "$outside\pcopt-smoke-outside.bin" (32MB) 5
New-Item -ItemType Junction -Path "$root\link-outside" -Target $outside | Out-Null
# Negar el listado de `locked` (se restaura en el cierre).
icacls "$root\locked" /deny "*S-1-1-0:(RD)" | Out-Null
# Segunda fixture: 30 000 archivos de 1 byte (para cancelar).
1..30000 | ForEach-Object { [IO.File]::WriteAllBytes(("$many\f{0:00000}.bin" -f $_), [byte[]]@(1)) }
```

Resultado esperado de la fixture principal (analizada como raíz): `big` 64 MiB; `medium\nested` 24 MiB (3 × 8 MiB); `small` 400 KiB (100 × 4 KiB); `recent.bin` 2 MiB; `empty` 0; `locked` inaccesible (0 B, `access_denied`); `link-outside` no se suma (32 MiB ausentes, `reparse_skipped = 1`). Total ≈ 64 + 24 + 0,39 + 2 = 90,4 MiB en 105 archivos.

## Pasos

1. **Crear las fixtures y compilar.** El agente corre el script de arriba (con cuenta atrás de aviso solo si lanza UI; aquí no) y verifica con `Get-ChildItem` los tamaños. Esperado: coinciden con la sección anterior. `npm run build` y `cargo build` (debug) verdes.
2. **Lanzar la app en simple** (aviso + cuenta atrás de 10 s; base temporal). Ir a "Espacio en disco". Esperado: encabezado, la explicación "Solo se lee: no se borra ni se mueve nada" y UN solo botón primario, "Analizar disco C:". **No pulsarlo** (sería `C:` completo): el agente lo anota y sigue.
3. **Pasar a avanzado** (Configuración). Esperado: aparece el selector de raíz ("Disco C:" marcado; "Una carpeta…").
4. **Elegir la fixture**: marcar "Una carpeta…"; "Analizar carpeta" queda deshabilitado con su motivo. El **dueño** pulsa "Elegir carpeta" y elige `pcopt-smoke-c07`. Esperado: se muestra la ruta elegida y "Analizar carpeta" se habilita. **Raíz verificada** (paso 4 de las reglas): el agente lee en el DOM "Carpeta elegida: …\pcopt-smoke-c07"; si no coincide, se detiene.
5. **Analizar** la fixture. Esperado: progreso con conteos, resultado `done`. Verificar en la base: `folder_scan_result` con `root_kind = picked`, `total_bytes ≈ 90,4 MiB`, `reparse_skipped = 1`, `inaccessible_dirs = 1`; `folder_scan_issue` con `locked` / `access_denied`; `locked` con `access = inaccessible` y la raíz `incomplete`; `link-outside` ausente en `folder_snapshot`.
6. **Sumas y navegación (avanzado).** Esperado en la UI: filas `big` (64 MiB), `medium` (24 MiB), `small`, `empty`, `locked` con "Sin acceso (inaccessible)"; fila "Archivos sueltos" = 2 MiB (`recent.bin`) y "Carpetas más chicas" solo si aplica; entrar en `medium` → `nested`, migas de pan `pcopt-smoke-c07 / medium / nested`, volver por la miga de la raíz.
7. **Top de archivos.** Por tamaño: `pcopt-smoke-big.bin` (64 MiB) antes que los de 8 MiB. Por "Más nuevos": `recent.bin` primero aunque sea más chico. Por "Más viejos": `pcopt-smoke-big.bin` primero (hace 30 días). El texto dice que son los 500 más grandes. La junction no aparece ni sus 32 MiB.
8. **Detalles.** Contadores: inaccesibles 1, enlaces no seguidos 1, nube 0, opacas 0; la muestra lista `locked` con "Acceso denegado (access_denied)".
9. **Mostrar en el Explorador.** El agente pulsa "Mostrar en el Explorador" sobre `pcopt-smoke-big.bin` (aviso de 10 s antes: se abre una ventana). El **dueño** confirma: se abrió la carpeta `big` con el archivo seleccionado y **nada se ejecutó**. Repetir con una carpeta (`medium`).
10. **Selector sobre la junction.** El **dueño** elige `pcopt-smoke-c07\link-outside` en el selector. Esperado: rechazo con el motivo "Es un enlace (junction o enlace simbólico)…" y la selección anterior intacta.
11. **Cancelación.** Elegir `pcopt-smoke-c07-many` con el selector (raíz verificada), pulsar "Analizar carpeta" y cancelar apenas aparezca el progreso. Esperado: aviso "Análisis cancelado. Se conserva el resultado anterior."; en la base el último escaneo `folders` es `cancelled` y el último `done` sigue siendo el del paso 5 (la pantalla sigue mostrando la fixture principal).
12. **Tiempos y memoria (D-21).** Analizar `many` completo (30 000 archivos) y anotar la duración y la memoria de la app (`Get-Process pc-optimizer`). Esperado: segundos; sin picos anormales.
13. **Análisis de solo lectura de `C:` completo (OPCIONAL: requiere autorización explícita del dueño en este paso).** Con la base temporal, en avanzado con "Disco C:" (raíz verificada), analizar y anotar: duración (objetivo orientativo ≤ 5 min en release/SSD), memoria pico (≤ 200 MB), carpetas guardadas, truncado, "no contabilizado", tamaño de la base y tiempo de guardado (≤ 3 s). Si se pasa de ~5 min o la memoria se dispara, se vuelve al dueño (D-21) antes de seguir.
14. **Variante elevada, release, `match` (OPCIONAL: autorización explícita).** Construir release, lanzar elevado (UAC con aviso de 10 s): el selector y "Mostrar en el Explorador" desde el proceso elevado; el **dueño** confirma que el Explorador se abre y que el diálogo funciona. Verificar el comportamiento en `mismatch` solo si hay una sesión disponible (el Explorador debe estar bloqueado con su motivo).

## Cierre

1. El agente restaura la ACL de `locked` y quita la junction **sin recursividad**:

```powershell
icacls "$env:USERPROFILE\Downloads\pcopt-smoke-c07\locked" /remove:d "*S-1-1-0"
cmd /c rmdir "$env:USERPROFILE\Downloads\pcopt-smoke-c07\link-outside"
```

2. Verificar que `pcopt-smoke-c07-outside\pcopt-smoke-outside.bin` sigue intacto (32 MiB) y que ningún archivo de las fixtures cambió de tamaño ni de fecha.
3. **La regla global del dueño bloquea `rm -rf`**: el agente NO borra las fixtures; le entrega este comando exacto al dueño, que lo corre él:

```powershell
foreach ($p in @("$env:USERPROFILE\Downloads\pcopt-smoke-c07", "$env:USERPROFILE\Downloads\pcopt-smoke-c07-outside", "$env:USERPROFILE\Downloads\pcopt-smoke-c07-many", "$env:LOCALAPPDATA\Temp\pcopt-smoke-c07")) { Remove-Item -LiteralPath $p -Recurse -Force }
```

4. Anotar los resultados de cada paso en `design.md` §Apply notes (tarea 9.3) y marcar la tarea.

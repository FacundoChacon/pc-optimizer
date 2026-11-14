# Guía del smoke guiado de C-11 (D-27)

> Esta guía la ejecuta el agente principal **junto con el dueño**. El agente que implementó el change NO corrió nada de esto: no lanzó la app, no disparó UAC, no creó la carpeta del smoke y no tocó la Papelera ni las carpetas reales del dueño.

## Reglas del smoke

1. **Paso a paso**: un paso por vez; el dueño confirma en el chat antes de pasar al siguiente.
2. **Aviso de 10 s** antes de CADA lanzamiento de la UI o de UAC. El agente escribe en el chat "Lanzo la app en 10 segundos" y hace una cuenta atrás real de 10 s (`for ($i=10; $i -gt 0; $i--) { Write-Host $i; Start-Sleep 1 }`) antes del comando de lanzamiento.
3. **Sin capturas de pantalla**: el agente verifica con el sistema de archivos, el contenido de la Papelera (solo lectura) y la base en solo lectura.
4. Los clics finales de **"Mover N copias a la Papelera"** los hace **el dueño**, no el agente.
5. Terminal NO elevada salvo la variante elevada (paso 9).
6. Si algo no coincide con lo esperado, se detiene y se anota; no se improvisa.
7. **Duplicados reales del dueño**: si el escaneo muestra grupos de archivos que NO empiezan con `pcopt-smoke-`, no se eligen, no se revisan y no se mueven.

## Datos de apoyo

- Carpeta del smoke: `%USERPROFILE%\Downloads\pcopt-smoke-c11` (la única raíz seleccionada, en modo avanzado: solo "Descargas").
- Base temporal: `PCOPT_DATA_DIR=%LOCALAPPDATA%\Temp\pcopt-smoke-c11\data` (nunca la base real del dueño).
- Lectura de la base en solo lectura (el agente, sin escribir): `python -I -c "import sqlite3,sys; c=sqlite3.connect('file:'+sys.argv[1]+'?mode=ro', uri=True); [print(r) for r in c.execute(sys.argv[2])]" <ruta a data.db> "<consulta>"`. Consultas útiles:
  - `select id,category,path,outcome,error_detail,scan_id from cleanup_log order by id` (categoría `duplicates`).
  - `select g.id,g.size_bytes,g.file_count,g.resolved_at,f.path,f.kept from duplicate_group g join duplicate_file f on f.group_id=g.id order by g.id,f.id`.
  - `select * from duplicate_scan_result` y `select * from duplicate_scan_root`.
- Arranque en desarrollo sin elevar: Vite aparte (`npm run dev`) y el ejecutable debug `src-tauri\target\debug\pc-optimizer.exe` con `PCOPT_DATA_DIR` fijado (igual que en C-09 y C-10).
- Contenido de la Papelera (solo lectura): `(New-Object -ComObject Shell.Application).NameSpace(10).Items() | % { $_.Name + ' | ' + $_.ExtendedProperty('System.Recycle.DeletedFrom') }`.

## Fixture (lo crea el agente en el paso 1; el script NO se corrió al escribir esta guía)

```powershell
$root = Join-Path $env:USERPROFILE 'Downloads\pcopt-smoke-c11'
New-Item -ItemType Directory -Force $root, "$root\a", "$root\b", "$root\c", "$root\d", "$root\sub2", "$root\node_modules\x", "$root\outside" | Out-Null

function New-Bytes([int]$len, [int]$seed) { $r = New-Object System.Random($seed); $b = New-Object byte[] $len; $r.NextBytes($b); return ,$b }
function Save([string]$path, [byte[]]$bytes, [int]$ageDays = 3) {
  [IO.File]::WriteAllBytes($path, $bytes)
  if ($ageDays -gt 0) { (Get-Item $path).LastWriteTime = (Get-Date).AddDays(-$ageDays) }
}
$MiB = 1MB

# Grupo A: 3 copias de 2 MiB; la tercera se escribe ahora (reciente: 24 h)
$a = New-Bytes (2*$MiB) 1
Save "$root\pcopt-smoke-A1.bin" $a
Save "$root\a\pcopt-smoke-A2.bin" $a
Save "$root\a\pcopt-smoke-A3-reciente.bin" $a 0
# Grupo B: 3 copias de 5 MiB
$b = New-Bytes (5*$MiB) 2
Save "$root\pcopt-smoke-B1.bin" $b
Save "$root\b\pcopt-smoke-B2.bin" $b
Save "$root\b\pcopt-smoke-B3.bin" $b
# Grupo C: 3 copias de 2 MiB (para kept_copy_unavailable)
$c = New-Bytes (2*$MiB) 3
Save "$root\c\pcopt-smoke-C1.bin" $c
Save "$root\c\pcopt-smoke-C2.bin" $c
Save "$root\c\pcopt-smoke-C3.bin" $c
# Grupo D: 3 copias de 2 MiB (para content_changed)
$d = New-Bytes (2*$MiB) 4
Save "$root\d\pcopt-smoke-D1.bin" $d
Save "$root\d\pcopt-smoke-D2.bin" $d
Save "$root\d\pcopt-smoke-D3.bin" $d
# Par falso positivo: 5 MiB con iguales los primeros y los ultimos 64 KiB y un byte distinto en el medio
$f1 = New-Bytes (5*$MiB) 5
$f2 = [byte[]]$f1.Clone(); $f2[2*$MiB] = [byte](($f2[2*$MiB] + 1) % 256)
Save "$root\pcopt-smoke-F1.bin" $f1
Save "$root\pcopt-smoke-F2.bin" $f2
# Unico (tamano sin pareja)
Save "$root\pcopt-smoke-U.bin" (New-Bytes (3*$MiB + 7) 6)
# Par bajo el minimo (900 KiB)
$s = New-Bytes (900*1KB) 7
Save "$root\pcopt-smoke-S1.bin" $s
Save "$root\pcopt-smoke-S2.bin" $s
# Par con hard link
Save "$root\pcopt-smoke-H1.bin" (New-Bytes (2*$MiB) 8)
New-Item -ItemType HardLink -Path "$root\pcopt-smoke-H2.bin" -Target "$root\pcopt-smoke-H1.bin" | Out-Null
# Junction hacia 'outside' con una copia de A (no debe aparecer en ningun grupo)
Save "$root\outside\pcopt-smoke-OUT-A.bin" $a
New-Item -ItemType Junction -Path "$root\jct" -Target "$root\outside" | Out-Null
# node_modules con un par (excluida)
$n = New-Bytes (2*$MiB) 9
Save "$root\node_modules\x\pcopt-smoke-N1.bin" $n
Save "$root\node_modules\pcopt-smoke-N2.bin" $n
```

## Pasos

### 1. Preparación (el agente; sin lanzar la app)
Crear la fixture con el script de arriba y anotar `Get-FileHash` (SHA256) de A1, A2, B1, C1, D1 y F1/F2 (F1 ≠ F2). Verificar: `pcopt-smoke-H1/H2` tienen 2 nombres (`fsutil hardlink list`), `jct` es una junction (`Get-Item`: `LinkType Junction`) y `A3-reciente` tiene fecha de hoy. Anotar la Papelera actual (cantidad de elementos) para comparar al final.

### 2. Escaneo (aviso de 10 s antes de lanzar; modo avanzado)
1. Lanzar la app sin elevar con la base temporal. Configuración → Modo avanzado. Ir a **Duplicados**.
2. En "Carpetas a revisar" dejar marcada **solo Descargas** (desmarcar las otras cinco; verificar que la última no se puede desmarcar); tamaño mínimo "1 MB". Pulsar "Buscar duplicados". El agente anota la hora de inicio y fin (para D-24) y los eventos de progreso vistos por etapa.
3. Esperado entre los archivos `pcopt-smoke-*`: **exactamente** los grupos A (3 copias, 2 MB), B (3 copias, 5 MB), C y D (3 copias, 2 MB); ninguna `OUT-A`, `N1/N2`, `F1/F2`, `U`, `S1/S2` ni `H1/H2`. Contadores avanzados (como mínimo): con varios nombres 2, carpetas excluidas 1, menores que el mínimo 2. Carpetas: Descargas "Revisada", las otras "No elegida". Ninguna copia viene elegida.
4. El agente lee la base: `duplicate_scan_result` con `min_size_bytes = 1048576`; los `duplicate_file.kept` están todos en 0; `cleanup_log` vacío.

### 3. Esc no mueve nada
Elegir en A (modo simple o avanzado) conservar `A1`, y en B conservar `B1` y `B2` (avanzado: casillas). "Revisar y mover 3 copias…": el diálogo muestra 2 copias a mover (A2 y B3; A3 se informa como reciente) y la nota de las 24 h. Foco en "Cancelar". Pulsar **Esc**: el diálogo se cierra y el foco vuelve a "Revisar…". El agente verifica: los 12 archivos intactos, `cleanup_log` vacío y `kept` en 0 en todas las filas.

### 4. Confirmación del dueño
El dueño vuelve a pulsar "Revisar…" y luego **"Mover 2 copias a la Papelera"** (clic del dueño; un doble clic no debe duplicar nada). Esperado: progreso por copia y resumen "Se movieron 2 copias…" con el foco en su título.

### 5. Verificación de la ejecución (solo lectura)
1. Papelera: aparecen `pcopt-smoke-A2.bin` y `pcopt-smoke-B3.bin` con origen en la carpeta del smoke.
2. Intactos: A1, A3-reciente, B1, B2 (hash igual al anotado).
3. `cleanup_log`: 2 filas, `category = duplicates`, `outcome = moved_to_trash`, el `scan_id` del escaneo; rutas reales.
4. Grupos A y B con `resolved_at` y `kept = 1` solo en A1, B1 y B2; C y D sin resolver y sin marcas. En la UI A y B quedan plegados como "Resuelto · conservaste …" y el resumen del escaneo informa 2 grupos abiertos.

### 6. `kept_copy_unavailable`
1. En C elegir conservar `C1` y abrir "Revisar…" (diálogo abierto, sin confirmar).
2. **El agente** mueve `c\pcopt-smoke-C1.bin` a `sub2\` (el archivo cambia de ruta: la copia elegida ya no está donde estaba).
3. El dueño pulsa "Mover 2 copias…". Esperado: ninguna copia de C se mueve; C2 y C3 quedan en su lugar; `cleanup_log` registra 2 filas `skipped_protected` / `kept_copy_unavailable`; el resumen avanzado muestra el motivo ("La copia que elegiste conservar ya no estaba igual") con su código crudo.

### 7. `content_changed`
1. En D elegir conservar `D1` y abrir "Revisar…" (preview con 2 copias).
2. **El agente** cambia un byte del medio de `d\pcopt-smoke-D2.bin` (mismo tamaño) y le restaura la fecha de última escritura anterior (`(Get-Item …).LastWriteTime = <fecha anotada>`).
3. El dueño confirma. Esperado: D2 `skipped_locked` / `content_changed` y sigue en su lugar; D3 se mueve a la Papelera; el grupo D queda resuelto (el resumen sugiere buscar de nuevo para reintentar).

### 8. Tiempos y rendimiento (D-24)
El agente anota por etapa (recorrido, parcial, completo) los tiempos del paso 2 y, de la lectura de `hashed_bytes` y de la duración, MB/s del hash completo. Anotar también cuánto tardó la re-verificación (pasos 4, 6 y 7). Si algún tiempo parece inaceptable (escaneo real > 5 min o re-verificación lenta), **detenerse y volver al dueño** con D-07 B o D-14 B antes de seguir.

### 9. Opcionales (solo con autorización explícita en el chat)
- **Escaneo real de solo lectura** (el dueño escribe: `autorizo escanear mis carpetas de contenido, solo lectura`): modo simple, las seis carpetas, base temporal. No se elige nada ni se abre "Revisar…". Anotar tiempo total, archivos vistos, archivos/s, grupos, bytes recuperables y contadores de omitidos. Cerrar sin decidir.
- **Variante elevada (release; aviso de 10 s antes del UAC)**: el dueño acepta UAC con su propia cuenta (`match`); solo escaneo de la carpeta del smoke, sin mover nada; verificar que los resultados coinciden con el paso 2. Over-the-shoulder solo si el dueño quiere (otra cuenta admin): se espera "No se pueden buscar duplicados con esta cuenta" sin recorrer nada.

### 10. Cierre
1. Cerrar la app. Quitar la junction **primero** con `rmdir "$root\jct"` (sin `/s` ni `/q`), luego la carpeta `pcopt-smoke-c11` completa y la base temporal.
2. Lo que quedó en la Papelera (A2, B3, D3 y los de pruebas anteriores) queda a criterio del dueño.
3. Anotar los resultados de cada paso, los tiempos y cualquier diferencia en `design.md` §Apply notes (tarea 10.3).

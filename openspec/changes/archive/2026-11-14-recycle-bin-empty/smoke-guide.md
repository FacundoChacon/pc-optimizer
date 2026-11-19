# Guía del smoke guiado de C-10 (D-16)

> Esta guía la ejecuta el agente principal **junto con el dueño**. El agente que implementó el change NO corrió nada de esto: no lanzó la app, no disparó UAC y no tocó la Papelera real.

## Reglas del smoke

1. **Paso a paso**: un paso por vez; el dueño confirma en el chat antes de pasar al siguiente.
2. **Aviso de 10 s** antes de CADA lanzamiento de la UI o de UAC. El agente escribe en el chat "Lanzo la app en 10 segundos" y hace una cuenta atrás real de 10 s (`for ($i=10; $i -gt 0; $i--) { Write-Host $i; Start-Sleep 1 }`) antes del comando de lanzamiento.
3. **Sin capturas de pantalla**: el agente verifica por medición (`SHQueryRecycleBin` de solo lectura) y leyendo la base en solo lectura.
4. **Compuerta de escritura (obligatoria)**: ningún paso que pueda vaciar la Papelera (pasos 5, 6 y 8, y el test `#[ignore]` real) se ejecuta hasta que el dueño escriba **exactamente**: `mi Papelera de C: puede vaciarse`. Sin esa frase el smoke se detiene al terminar el paso 4.
5. Los clics de confirmación final del vaciado ("Continuar", casilla y "Eliminar para siempre") los hace **el dueño**, no el agente.
6. Terminal NO elevada para todo salvo el paso 8 (variante elevada).
7. Si algo no coincide con lo esperado, se detiene y se anota; no se improvisa.

## Datos de apoyo

- Carpeta de trabajo: `%LOCALAPPDATA%\Temp\pcopt-smoke-c10` (el agente la crea; marcadores `pcopt-smoke-c10-*`, 1 KB cada uno).
- Base de datos temporal: `PCOPT_DATA_DIR=%LOCALAPPDATA%\Temp\pcopt-smoke-c10\data` (nunca la base real del dueño).
- Lectura de la base en solo lectura (el agente, con Python, sin escribir): `python -I -c "import sqlite3,sys; c=sqlite3.connect('file:'+sys.argv[1]+'?mode=ro', uri=True); [print(r) for r in c.execute(\"select id,ts,category,path,size_bytes,outcome,scan_id,error_detail from cleanup_log order by id desc limit 5\")]" <ruta a data.db>`.
- Medición de la Papelera (solo lectura): `cargo test --manifest-path src-tauri/Cargo.toml --locked -- --ignored query_should_measure_the_real_system_drive_recycle_bin --nocapture` (test de C-08, solo mide).
- Arranque en desarrollo sin elevar: Vite aparte (`npm run dev`) y el ejecutable debug `src-tauri\target\debug\pc-optimizer.exe` con `PCOPT_DATA_DIR` fijado (igual que en el smoke de C-09).

## Pasos

### 1. Preparación del dueño (antes de cualquier ejecución real)
El dueño abre la Papelera, **restaura todo lo que quiera conservar** (hoy contiene además 642 `pcopt-smoke-*` y 4 `pcopt-test-*` de C-09) y escribe en el chat: `mi Papelera de C: puede vaciarse`. Mientras esa frase no esté escrita, solo se pueden hacer los pasos 2 a 4 (no vacían nada).

### 2. Marcadores
El agente crea 20 archivos `pcopt-smoke-c10-*` (1 KB cada uno) en la carpeta de trabajo y los envía a la Papelera con el shell (PowerShell, `Microsoft.VisualBasic.FileIO.FileSystem.DeleteFile(ruta, OnlyErrorDialogs, SendToRecycleBin)`). Mide la Papelera y anota el estado inicial (elementos y bytes). **No requiere la frase**: solo agrega elementos.

### 3. Ensayo sin vaciar (dev sin elevar; aviso de 10 s antes de lanzar)
1. Lanzar la app (sin elevar, base temporal). Limpieza → "Buscar basura": la tarjeta de la Papelera muestra la cantidad medida y el botón "Vaciar Papelera…".
2. Pulsar "Vaciar Papelera…": el primer diálogo ("Vaciar la Papelera") muestra la cantidad y el tamaño correctos; el foco está en "Cancelar". Pulsar **Esc**: el diálogo se cierra y el foco vuelve a "Vaciar Papelera…". El agente re-mide: igual que antes.
3. Repetir y pulsar "Continuar": aparece el segundo diálogo ("¿Eliminar para siempre?"). Verificar que la casilla está sin marcar y que "Eliminar para siempre N elementos" está deshabilitado. Pulsar "Cancelar". El agente re-mide: igual que antes.
4. Repetir hasta el segundo diálogo, dejarlo abierto **más de 2 minutos**, marcar la casilla y pulsar "Eliminar para siempre": debe aparecer "La solicitud de vaciado venció…" con el botón "Revisar de nuevo". El agente re-mide: igual que antes (el dueño no vació nada: esta confirmación vence).
5. El agente lee `cleanup_log`: **0 filas** de `recycle_bin` (los rechazos no se registran).

### 4. Cambio detectado
1. Abrir los pasos 1 y 2 y dejar el segundo diálogo abierto (casilla sin marcar).
2. El agente envía **un** marcador más a la Papelera (como en el paso 2).
3. El dueño marca la casilla y pulsa "Eliminar para siempre": debe aparecer "La Papelera cambió desde que la revisaste. No se eliminó nada." y "Revisar de nuevo".
4. El agente re-mide: estado anterior + 1, nada se vació; `cleanup_log` sigue sin filas de `recycle_bin`.

> **Detenerse aquí si el dueño no escribió `mi Papelera de C: puede vaciarse`.** Los pasos siguientes vacían de verdad.

### 5. Vaciado real (requiere la frase)
1. El dueño pulsa "Revisar de nuevo" → primer diálogo (con la medición nueva) → "Continuar" → marca la casilla → "Eliminar para siempre" (clics del dueño).
2. Verificar: resultado `emptied` ("Se liberaron … "); **sin ventanas del shell ni sonido**; medición posterior 0 elementos; la tarjeta de la Papelera en 0; "Vaciar Papelera…" ya no se ofrece.
3. El agente lee `cleanup_log`: una fila nueva con `category = recycle_bin`, `path = C:\`, `size_bytes` = bytes medidos, `outcome = recycle_bin_emptied`, `scan_id` NULL, sin `error_detail`.
4. El escaneo sigue admitiendo "Limpiar" si tenía elegibles (el vaciado no lo marca como limpiado: verificar que no aparece "Ya limpiaste este análisis").

### 6. Exclusividad (opcional, sin vaciar)
Con 500 archivos de prueba y una limpieza en curso, verificar que "Vaciar Papelera…" no se ofrece y que "Buscar basura" queda deshabilitado durante un vaciado. No requiere la frase si no se confirma el segundo paso.

### 7. Test real `#[ignore]` (solo si el dueño lo autoriza explícitamente)
`real_empty_should_leave_the_system_recycle_bin_empty` vacía la Papelera real. Solo con la frase y una autorización extra, y con la Papelera previamente cargada con 5 marcadores. Comando: `cargo test --manifest-path src-tauri/Cargo.toml --locked -- --ignored real_empty_should_leave_the_system_recycle_bin_empty --nocapture`.

### 8. Variante elevada / release (aviso de 10 s antes del UAC; requiere la frase)
1. El agente carga 5 marcadores nuevos (paso 2) y compila el ejecutable de release (`requireAdministrator`).
2. Aviso de 10 s; se lanza el ejecutable: aparece UAC; el dueño acepta con **su propia cuenta** (`match`).
3. Repetir el paso 5 (el dueño hace los clics). Confirmar que el vaciado elevado afectó a la Papelera del mismo usuario (medición posterior 0 desde la terminal no elevada).
4. Over-the-shoulder: solo si el dueño quiere (requiere otra cuenta admin); se espera `recycle_bin_blocked` sin medir.

### 9. Limpieza del smoke
Quitar la carpeta de trabajo `pcopt-smoke-c10` con `rmdir` del directorio de pruebas (nunca sobre archivos del usuario) y cerrar la app. Anotar resultados en `design.md` §Apply notes (10.2).

## Alternativa
Si el dueño prefiere no vaciar su Papelera, hacer los pasos 5 y 8 en **Windows Sandbox** (descartable): requiere habilitar la característica y copiar el build; es más lento y no prueba la cuenta real.

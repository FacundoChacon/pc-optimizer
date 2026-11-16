# Proposal

## Why

US-006 y US-007 (v0.1, gobernanza MEDIUM): el usuario quiere saber **qué llena su disco** (qué carpetas ocupan más) y **cuáles son los archivos más grandes**, para decidir qué revisar. Hoy la app solo muestra basura de la whitelist (C-08) y duplicados en las carpetas de contenido (C-11); no hay ninguna vista del espacio del disco. Es la pieza de "Espacio en disco" del mapa de pantallas (KB 12), la necesita el dashboard de C-12 y, por la nota de C-05, es el primer consumidor del **selector de carpeta** que reemplaza al arrastrar y soltar (UIPI bloquea el drag & drop del Explorador hacia la ventana elevada).

## What Changes

- **Análisis de uso del disco de solo lectura** (dominio puro sobre puertos): recorre una raíz (por defecto la raíz del volumen del sistema; en avanzado, una carpeta elegida con el selector) reutilizando el núcleo de recorrido de C-08/C-11, sin seguir junctions ni symlinks, sin abrir ni leer el contenido de ningún archivo y sin escribir nada en el disco. Suma tamaños y cantidades por carpeta, guarda las carpetas más grandes conectadas con su padre y el top de archivos más grandes.
- **Carpetas inaccesibles informadas sin romper el escaneo**: cada carpeta que no se puede listar queda marcada (y sus ancestros como "tamaño mínimo"), con un motivo estable y una muestra acotada de rutas.
- **Privacidad de otros perfiles**: los perfiles de otros usuarios (y en over-the-shoulder, todos los perfiles), la Papelera de la raíz y `System Volume Information` se miden como una sola carpeta **opaca**: tamaño sí, nombres de su contenido nunca (ni en la base ni en el top de archivos).
- **Progreso y cancelación**: progreso por conteo en el canal de la invocación; cancelación con el `cancel_scan` existente; exclusividad con los demás trabajos por `ActiveJob`.
- **Persistencia** (migración **004**, contigua a la 003 de C-11): `folder_snapshot` (UNIQUE(scan_id, path), índice (path, scan_id), `parent_id` autorreferencia, como pide KB 04) más el resultado del escaneo y el top de archivos grandes; solo se conservan los datos del último análisis terminado.
- **Selector de carpeta propio** (sin plugins): diálogo nativo de Windows abierto por el backend; la interfaz nunca envía rutas, solo el identificador de la carpeta elegida, que el backend valida (volumen del sistema, existe, no es punto de reanálisis, no es perfil ajeno).
- **Mostrar en el Explorador** sin el plugin `shell`: comando del backend que abre la carpeta contenedora y selecciona el elemento, resolviendo la ruta por id desde el último análisis (nunca una ruta libre de la interfaz) y sin ejecutar nunca el archivo.
- **Nunca borra**: no hay ningún camino de borrado nuevo. "Enviar a revisión de limpieza" se limita a un enlace a la pantalla Limpieza cuando la carpeta corresponde a una ubicación de la whitelist de basura; para todo lo demás, solo "Mostrar en el Explorador".
- **UI**: pantalla nueva **Espacio en disco** en la navegación. Simple: resumen del disco, las 5 carpetas que más ocupan y los 5 archivos más grandes. Avanzado: lista navegable completa con barras proporcionales y migas de pan, top de archivos ordenable por tamaño y por fecha, carpetas sin acceso y contadores. Sin librería de gráficos (no fuerza PA-01).
- **IPC**: 6 comandos nuevos con sus 6 permisos `allow-*` explícitos; reutiliza `cancel_scan`; sin `core:*`, sin plugins, sin cambios de CSP.
- Decisiones que requieren aprobación del dueño **[DUEÑO]** en `design.md` (alcance y selector, privacidad de perfiles, revelar en el Explorador, "enviar a revisión", visualización, granularidad y contabilidad de tamaños, esquema, superficie IPC, UI, smoke).

Fuera de alcance: otros volúmenes (PA-12), borrar o mover archivos o carpetas desde esta pantalla, una categoría de limpieza "archivos grandes" (requiere `/opsx:propose` aprobado), el treemap gráfico (queda la lista con barras; ver D-12), tendencias "qué carpeta crece más" (C-25), top de archivos por carpeta, arrastrar y soltar, carpetas arbitrarias para duplicados (PA-16 sigue abierta para C-11).

## Capabilities

### New Capabilities
- `disk-usage`: análisis de uso del disco por carpeta y top de archivos grandes (raíces permitidas, selector de carpeta, recorrido de solo lectura sin seguir enlaces, contabilidad de tamaños, carpetas inaccesibles, carpetas opacas por privacidad, progreso y cancelación, persistencia del último análisis, mostrar en el Explorador, enlace a Limpieza, comandos IPC y pantalla Espacio en disco en ambos modos).

### Modified Capabilities
- `app-shell`: lista exacta de comandos IPC habilitados (6 nuevos) y pantalla Espacio en disco en la navegación.
- `local-store`: migración 4 (esquema del análisis de carpetas), datos solo del último análisis terminado, escaneos interrumpidos que también descartan sus carpetas y archivos.
- `domain-ports`: contratos nuevos de persistencia del análisis de carpetas, espacio del volumen, selector de carpeta y Explorador, con dobles y batería de contrato.
- `path-protection`: validación de raíces de análisis (más permisiva que la de reglas de limpieza porque no habilita ningún borrado, pero que rechaza perfiles ajenos, otros volúmenes y puntos de reanálisis).

## Impact

- **Rust**: nuevo `src-tauri/src/domain/disk_usage/` (modelo, raíz, agregación, selección de carpetas, top de archivos, escaneo, `*_tests.rs`); `domain/junk/walk.rs` (dos ganchos opcionales del visitante: carpeta listada y carpeta inaccesible con motivo, sin cambio de comportamiento para C-08/C-11); `domain/protection/service.rs` (validación de raíz de análisis); `domain/ports/` (`volume.rs`, `shell.rs`, contrato de persistencia de carpetas en `store.rs`, dobles); nuevos `adapters/windows/volume.rs`, `folder_picker.rs`, `explorer.rs` (`unsafe` mínimo con `// SAFETY:`); `adapters/sqlite/` (migración `004_folders.sql`, `folders.rs`, contrato); nuevo `commands/disk_usage.rs` (+ tests); `commands/names.rs`, `lib.rs`, `error.rs` (kinds nuevos).
- **Capabilities**: `capabilities/main-window.json` + `allow-start-folder-scan`, `allow-get-last-folder-scan`, `allow-list-folder-children`, `allow-list-large-files`, `allow-pick-folder-root`, `allow-reveal-in-explorer` (27 permisos en total, 0 `core:*`, 0 plugins).
- **Frontend**: `src/lib/ipc.ts` (tipos y decodificadores), nuevo `src/features/disk/`, `src/app/navigation.ts`, `src/shared/i18n/{es,errors}.ts`, `src/tests/support/mockBackend.ts` y tests.
- **Dependencias**: ninguna nueva en npm ni en Cargo; `walkdir` sigue sin consumidor (ver D-05). Las APIs del selector, del Explorador y del espacio del volumen se confirman en el apply contra las features ya habilitadas de `windows` 0.62.2; si alguna exige una feature nueva, se consulta (D-24).
- **Esquema**: versión 3 → 4.
- **Docs** (en el apply): `CHANGES.md` (C-07 en curso y decisiones), KB 04 (tablas del análisis), KB 02 (comandos reales), KB 05 (reglas del análisis), KB 06 US-006/US-007, KB 10 (PA-01 no se fuerza, PA-16 parcialmente cubierta por el selector), KB 11 y 13 (selector y Explorador en un proceso elevado), KB 12 (pantalla real).

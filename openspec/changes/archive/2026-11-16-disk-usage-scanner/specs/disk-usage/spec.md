# Spec Delta

## Purpose

Mostrar al usuario qué carpetas y archivos ocupan su disco mediante un análisis de solo lectura del volumen del sistema o de una carpeta elegida, sin seguir enlaces, sin exponer el contenido de perfiles ajenos y sin ningún camino de borrado propio.

## ADDED Requirements

### Requirement: Análisis de solo lectura
El análisis de uso del disco SHALL limitarse a listar carpetas y leer los metadatos de la enumeración; MUST NOT abrir ni leer el contenido de ningún archivo, MUST NOT crear, modificar, mover ni borrar nada en el disco y MUST NOT recuperar contenido de la nube.

#### Scenario: Análisis de un árbol
- **WHEN** se analiza una carpeta con archivos y subcarpetas
- **THEN** ningún archivo se abre para leer contenido y el disco queda igual que antes del análisis

#### Scenario: Marcador en la nube
- **WHEN** el análisis encuentra un archivo que solo está en la nube
- **THEN** no se descarga ni se abre y se cuenta como archivo en la nube

### Requirement: Raíz por defecto en el volumen del sistema
Sin otra indicación, el análisis SHALL recorrer la raíz del volumen del sistema tomada del contexto del backend, nunca de una ruta enviada por la interfaz. Las rutas protegidas (Windows, programas, datos de programas) MUST recorrerse en solo lectura y marcarse como "sistema".

#### Scenario: Analizar disco C
- **WHEN** se inicia un análisis con la raíz `system_drive` en un equipo con sistema en `C:`
- **THEN** se recorre `C:\` y el resultado informa la raíz `system_drive` con su ruta

#### Scenario: Carpeta de Windows marcada
- **WHEN** el análisis de `C:\` guarda la carpeta `C:\Windows`
- **THEN** la carpeta tiene su tamaño y está marcada como sistema

### Requirement: Raíz elegida por identificador de selección
Una raíz elegida SHALL indicarse solo con el identificador que devolvió la última selección de carpeta; el backend MUST volver a validarla al iniciar el análisis, y un identificador desconocido o reemplazado MUST fallar con `folder_pick_not_found` sin crear ningún escaneo.

#### Scenario: Carpeta elegida válida
- **WHEN** el usuario eligió `C:\Users\ana\Downloads` y se inicia el análisis con su identificador
- **THEN** se recorre solo esa carpeta y el resultado informa la raíz `picked` con esa ruta

#### Scenario: Identificador viejo
- **WHEN** se inicia un análisis con un identificador de selección reemplazado por una selección posterior
- **THEN** la petición falla con `folder_pick_not_found` y no se crea ningún escaneo

#### Scenario: Carpeta reemplazada por un enlace tras elegirla
- **WHEN** la carpeta elegida se reemplaza por una junction antes de iniciar el análisis
- **THEN** la petición falla con `folder_root_rejected` y motivo `reparse_point`, sin crear ningún escaneo

### Requirement: Selector de carpeta nativo abierto por el backend
La elección de carpeta SHALL hacerse con el diálogo nativo de selección de carpetas de Windows abierto por el backend, sin plugins expuestos a la interfaz; la ruta elegida MUST validarse como raíz de análisis antes de aceptarse y solo un diálogo MUST poder estar abierto a la vez.

#### Scenario: Selección válida
- **WHEN** el usuario elige una carpeta del volumen del sistema que no es de otro usuario
- **THEN** la respuesta es `picked` con un identificador y la ruta para mostrar

#### Scenario: Selección cancelada
- **WHEN** el usuario cierra el diálogo sin elegir
- **THEN** la respuesta es `cancelled` y la selección anterior, si existía, se conserva

#### Scenario: Selección rechazada
- **WHEN** el usuario elige `D:\Datos` con el sistema en `C:`, o la carpeta de otro usuario
- **THEN** la petición falla con `folder_root_rejected` y motivo `unsupported_volume` u `other_user_profile`

#### Scenario: Segundo diálogo
- **WHEN** se pide abrir el selector mientras otro sigue abierto
- **THEN** la petición falla con `folder_picker_busy`

#### Scenario: Sin arrastrar y soltar
- **WHEN** se revisa la pantalla de análisis
- **THEN** no ofrece una zona para arrastrar carpetas desde el Explorador; la alternativa es el selector

### Requirement: Enlaces no seguidos
El recorrido SHALL no seguir junctions, symlinks ni puntos de montaje: su destino MUST NOT sumarse ni recorrerse y cada uno MUST contarse como enlace omitido.

#### Scenario: Junction hacia otra carpeta
- **WHEN** la carpeta analizada contiene una junction hacia una carpeta externa con 32 MiB
- **THEN** esos 32 MiB no se suman, la carpeta externa no aparece en el resultado y el contador de enlaces omitidos es 1

#### Scenario: Symlink de archivo
- **WHEN** la carpeta analizada contiene un symlink a un archivo grande
- **THEN** el archivo destino no se suma ni aparece en el top de archivos

### Requirement: Tamaños lógicos por carpeta
Cada carpeta SHALL informar el tamaño lógico total de los archivos que contiene (recursivo), el de los archivos directamente dentro y las cantidades recursivas de archivos y carpetas; un archivo con varios nombres MUST contarse una vez por nombre y un archivo en la nube MUST contarse aparte con 0 bytes.

#### Scenario: Sumas de un árbol conocido
- **WHEN** se analiza una carpeta con `a\x.bin` de 4 MiB, `a\b\y.bin` de 2 MiB y `z.bin` de 1 MiB
- **THEN** la raíz suma 7 MiB con 1 MiB directo y 3 archivos, `a` suma 6 MiB con 4 MiB directos y `a\b` suma 2 MiB

#### Scenario: Archivos en la nube
- **WHEN** una carpeta contiene 3 archivos solo en la nube
- **THEN** no suman bytes y el resultado informa 3 archivos en la nube

### Requirement: Carpetas inaccesibles informadas sin abortar
Una carpeta que no se puede listar SHALL quedar marcada como inaccesible con un motivo estable (`access_denied`, `not_found`, `io_error`), sus ancestros como incompletos, y el análisis MUST continuar con el resto; el resultado MUST informar el total y una muestra de hasta 50 rutas con su motivo.

#### Scenario: Carpeta sin permisos
- **WHEN** una subcarpeta niega el listado al proceso
- **THEN** queda inaccesible con `access_denied` y tamaño 0, sus ancestros quedan incompletos y las demás carpetas se suman normalmente

#### Scenario: Muchas carpetas sin acceso
- **WHEN** el análisis encuentra 120 carpetas que no puede listar
- **THEN** el total informa 120 y la muestra contiene exactamente 50 rutas con su motivo

#### Scenario: Raíz inaccesible
- **WHEN** la raíz del análisis no se puede listar
- **THEN** el análisis termina `done` con la raíz inaccesible, tamaño 0 y la incidencia informada

### Requirement: Carpetas opacas por privacidad
El perfil de otro usuario, la Papelera de la raíz del volumen y `System Volume Information` SHALL analizarse como carpetas opacas: su tamaño y cantidades se informan, pero ninguna ruta de su interior MUST guardarse, mostrarse, enviarse en eventos ni aparecer en el top de archivos o en la muestra de inaccesibles.

#### Scenario: Perfil de otro usuario
- **WHEN** el análisis de `C:\` encuentra `C:\Users\bob` con 10 GB y el usuario actual es `ana`
- **THEN** `C:\Users\bob` aparece como perfil de otro usuario con 10 GB, sin carpetas hijas, y ningún archivo de ese perfil está en el top

#### Scenario: Papelera de la raíz
- **WHEN** el análisis de `C:\` encuentra `C:\$Recycle.Bin`
- **THEN** aparece como Papelera con su tamaño y sin contenido navegable

### Requirement: Análisis en over-the-shoulder
Con contexto de privilegios `mismatch`, el análisis SHALL permitirse (es de solo lectura y de máquina) pero todos los perfiles de usuario MUST tratarse como opacos, la selección de carpetas bajo la raíz de perfiles MUST rechazarse con `other_user_profile` y mostrar en el Explorador MUST bloquearse.

#### Scenario: Análisis en mismatch
- **WHEN** se analiza `C:\` con contexto `mismatch`
- **THEN** el análisis termina `done` y `C:\Users\ana` aparece opaco, sin carpetas hijas ni archivos en el top

#### Scenario: Elegir el perfil en mismatch
- **WHEN** en `mismatch` el usuario elige `C:\Users\ana\Downloads`
- **THEN** la petición falla con `folder_root_rejected` y motivo `other_user_profile`

### Requirement: Carpetas guardadas acotadas y conectadas
El análisis SHALL guardar todas las carpetas si son 20 000 o menos y, si hay más, las 20 000 de mayor tamaño (desempate por menor profundidad y luego por ruta); toda carpeta guardada MUST tener guardado a su padre (salvo la raíz) y el resultado MUST informar si se truncó.

#### Scenario: Árbol chico
- **WHEN** se analiza una carpeta con 120 subcarpetas en total
- **THEN** se guardan las 121 carpetas y el resultado no está truncado

#### Scenario: Árbol grande
- **WHEN** el análisis encuentra 250 000 carpetas
- **THEN** se guardan exactamente 20 000, todas con su padre guardado, y el resultado indica que se truncó

#### Scenario: Resto de una carpeta
- **WHEN** se consulta una carpeta cuyos hijos guardados no cubren todo su tamaño
- **THEN** la respuesta informa como resto el tamaño total menos el directo menos la suma de los hijos guardados

### Requirement: Top de archivos más grandes
El análisis SHALL guardar los 500 archivos más grandes fuera de carpetas opacas, con ruta, tamaño y fecha de modificación, y la consulta MUST poder ordenarlos por tamaño descendente, por más nuevos o por más viejos.

#### Scenario: Orden por tamaño
- **WHEN** se consultan los archivos grandes ordenados por tamaño
- **THEN** el de 64 MiB aparece antes que los de 8 MiB

#### Scenario: Orden por fecha
- **WHEN** se consultan los archivos grandes ordenados por más nuevos
- **THEN** el archivo modificado hoy aparece antes que el de hace 30 días, aunque sea más chico

#### Scenario: Más de 500 archivos
- **WHEN** la raíz contiene 2 000 archivos
- **THEN** se guardan exactamente los 500 más grandes

### Requirement: Espacio del volumen y espacio no contabilizado
El resumen SHALL informar el espacio total y libre del volumen del sistema y, cuando la raíz es `system_drive`, el espacio usado no contabilizado por el análisis (nunca negativo, con indicación cuando lo analizado supera lo usado); si la consulta del volumen falla, el resumen MUST salir sin esos datos.

#### Scenario: Espacio no contabilizado
- **WHEN** el volumen tiene 400 GB usados y el análisis de `C:\` suma 370 GB
- **THEN** el resumen informa 30 GB no contabilizados

#### Scenario: Lo analizado supera lo usado
- **WHEN** el análisis suma más que el espacio usado del volumen
- **THEN** el resumen informa 0 no contabilizado e indica que lo analizado supera lo usado

### Requirement: Progreso sin rutas
Durante el análisis SHALL enviarse eventos de progreso por el canal de la invocación (inicio con el identificador del escaneo, avance con archivos, carpetas y bytes vistos por conteo, y aviso de guardado); ningún evento MUST incluir rutas.

#### Scenario: Avance periódico
- **WHEN** el análisis recorre 10 000 archivos
- **THEN** la interfaz recibe al menos dos eventos de avance con conteos crecientes y sin rutas

### Requirement: Cancelación del análisis
El análisis SHALL poder cancelarse con `cancel_scan` y su identificador; un análisis cancelado MUST terminar `cancelled` sin guardar carpetas ni archivos y el último análisis `done` MUST seguir disponible.

#### Scenario: Cancelar a mitad
- **WHEN** se cancela un análisis en curso
- **THEN** termina `cancelled`, no guarda carpetas ni archivos y la consulta del último análisis devuelve el anterior

### Requirement: Un solo trabajo a la vez
Iniciar un análisis SHALL fallar con el error del trabajo activo (`scan_in_progress`, `cleanup_in_progress` o `recycle_bin_empty_in_progress`) mientras haya otro escaneo, limpieza o vaciado en curso, sin crear ningún escaneo.

#### Scenario: Análisis durante una limpieza
- **WHEN** se inicia un análisis mientras corre una limpieza
- **THEN** la petición falla con `cleanup_in_progress` y no se crea ningún escaneo

### Requirement: Consultas del último análisis
Las consultas de carpetas y archivos SHALL responder solo sobre el último análisis `done` (otro identificador falla con `scan_not_found`), paginadas con un límite de 1 a 200 (fuera de rango: `invalid_argument`); una carpeta de otro análisis MUST fallar con `folder_not_found` y una carpeta opaca MUST responder sin hijos.

#### Scenario: Hijos de una carpeta
- **WHEN** se piden los hijos de la raíz ordenados por tamaño
- **THEN** la respuesta trae la carpeta, sus migas de pan, el total de hijos, la página pedida en orden descendente de tamaño y el resto

#### Scenario: Análisis viejo
- **WHEN** se piden carpetas de un análisis que ya no es el último `done`
- **THEN** la petición falla con `scan_not_found`

#### Scenario: Límite inválido
- **WHEN** se piden 500 elementos en una página
- **THEN** la petición falla con `invalid_argument`

### Requirement: Mostrar en el Explorador sin ejecutar
Mostrar en el Explorador SHALL abrir la carpeta contenedora con el elemento seleccionado, identificado por su id en el último análisis y nunca por una ruta enviada por la interfaz; MUST NOT ejecutar ni abrir el elemento, y MUST fallar con `reveal_unavailable` y motivo (`target_missing`, `opaque`, `different_user`, `undetermined`, `shell_error`).

#### Scenario: Mostrar un archivo grande
- **WHEN** el usuario pide mostrar `C:\Users\ana\Videos\viaje.mp4` del top
- **THEN** se abre el Explorador en `C:\Users\ana\Videos` con ese archivo seleccionado y el archivo no se reproduce ni se abre

#### Scenario: Ejecutable no ejecutado
- **WHEN** el usuario pide mostrar un archivo `.exe` del top
- **THEN** solo se selecciona en el Explorador; ningún proceso se inicia a partir de él

#### Scenario: Elemento borrado después del análisis
- **WHEN** el archivo ya no existe al pedir mostrarlo
- **THEN** la petición falla con `reveal_unavailable` y motivo `target_missing`

#### Scenario: En mismatch
- **WHEN** el contexto es `mismatch` y se pide mostrar una carpeta
- **THEN** la petición falla con `reveal_unavailable` y motivo `different_user` o `undetermined`

### Requirement: Revisión de limpieza solo por la pantalla Limpieza
Una carpeta que es o está dentro de una ubicación de la whitelist de basura, con alcance permitido en el contexto actual, SHALL marcarse para revisión, y la interfaz MUST ofrecer solo ir a la pantalla Limpieza; el análisis MUST NOT ofrecer ningún otro camino para borrar o mover elementos.

#### Scenario: Caché del navegador
- **WHEN** el análisis guarda la carpeta de caché de Edge del usuario actual
- **THEN** la carpeta está marcada para revisión y su acción "Revisar en Limpieza" lleva a la pantalla Limpieza sin iniciar nada

#### Scenario: Archivo grande personal
- **WHEN** se muestra un video grande en el top de archivos
- **THEN** sus únicas acciones son mostrarlo en el Explorador; no hay acción de borrar ni de mover

#### Scenario: Ubicación por-usuario en mismatch
- **WHEN** el contexto es `mismatch`
- **THEN** solo se marcan para revisión las ubicaciones de alcance máquina

### Requirement: Pantalla Espacio en disco en modo simple
En modo simple, la pantalla SHALL ofrecer una sola acción primaria por estado: "Analizar disco C:" sin resultados, "Cancelar" durante el análisis (con progreso en una región `aria-live`) y, con resultados, un resumen del disco, las 5 carpetas que más ocupan dentro de la raíz y los 5 archivos más grandes.

#### Scenario: Sin análisis
- **WHEN** se abre la pantalla sin análisis previos
- **THEN** se explica que solo se lee y la única acción primaria es "Analizar disco C:"

#### Scenario: Resultados
- **WHEN** termina un análisis de `C:\`
- **THEN** se muestran el resumen, 5 carpetas con barra y tamaño en texto, 5 archivos con tamaño y fecha, y los avisos de inaccesibles y archivos en la nube en lenguaje llano

### Requirement: Pantalla Espacio en disco en modo avanzado
En modo avanzado, la pantalla SHALL permitir elegir la raíz (disco del sistema o una carpeta con el selector), navegar la lista completa de carpetas con migas de pan, ordenar por tamaño o nombre, ver el top de archivos ordenable por tamaño y fecha, y ver los contadores y la muestra de carpetas sin acceso con su código.

#### Scenario: Carpeta sin elegir
- **WHEN** el usuario marca "Una carpeta…" sin haber elegido una válida
- **THEN** "Analizar" queda deshabilitado con el motivo asociado

#### Scenario: Navegar una carpeta
- **WHEN** el usuario entra en una carpeta de la lista
- **THEN** se muestran sus hijos, las migas de pan llevan de vuelta a la raíz y aparecen las filas de carpetas más chicas y archivos sueltos

### Requirement: Barras proporcionales accesibles sin estilos en línea
Las barras de tamaño SHALL ser decorativas para tecnologías de asistencia, ir siempre acompañadas del tamaño y el porcentaje en texto, no depender solo del color para el estado y renderizarse sin estilos en línea, compatibles con la política de seguridad de contenido de producción.

#### Scenario: Barra de una carpeta
- **WHEN** se muestra una carpeta que ocupa el 31 % de su padre
- **THEN** su barra no se anuncia y el texto visible dice el tamaño y "31 %"

#### Scenario: Sin estilos en línea
- **WHEN** se renderiza la lista de carpetas
- **THEN** ningún elemento de las barras usa el atributo `style`

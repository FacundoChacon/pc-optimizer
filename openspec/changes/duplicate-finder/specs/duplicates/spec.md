# Spec Delta

## Purpose

Encontrar archivos con contenido idéntico en las carpetas de contenido del usuario y dejar que él elija, grupo por grupo, qué copia conservar, moviendo las demás a la Papelera por el flujo de limpieza re-verificado, sin que nunca se pierda la última copia ni se elija por él.

## ADDED Requirements

### Requirement: Búsqueda solo en las carpetas de contenido del usuario actual
El escaneo de duplicados SHALL recorrer únicamente las carpetas de contenido del usuario del proceso (claves `desktop`, `documents`, `downloads`, `music`, `pictures`, `videos`), tomadas del contexto de protección y nunca de rutas enviadas por la interfaz. Por defecto se recorren todas; en modo avanzado el usuario MAY desmarcar algunas, y pedir una clave desconocida, repetida o ninguna MUST fallar con `invalid_argument`.

#### Scenario: Escaneo por defecto
- **WHEN** se inicia un escaneo de duplicados sin indicar carpetas
- **THEN** se recorren las seis carpetas de contenido del usuario actual y ninguna otra ubicación

#### Scenario: Solo Descargas
- **WHEN** se inicia un escaneo pidiendo solo `downloads`
- **THEN** solo se recorre la carpeta Descargas del usuario y las demás figuran como no seleccionadas en el resultado

#### Scenario: Clave inválida
- **WHEN** se pide un escaneo con las carpetas `downloads` y `appdata`, o con `downloads` repetida, o con la lista vacía
- **THEN** la petición falla con `invalid_argument` y no se crea ningún escaneo

### Requirement: Carpetas anidadas o no disponibles informadas
Cada carpeta seleccionada SHALL quedar en el resultado con un estado: `done`, `not_selected`, `nested` (está dentro de otra carpeta seleccionada y se recorre una sola vez) o `unavailable` con motivo (no existe, otro volumen, protegida, punto de reanálisis o en la nube). Una carpeta no disponible MUST NOT abortar el escaneo de las demás.

#### Scenario: Descargas dentro de Documentos
- **WHEN** la carpeta Descargas del usuario está en `C:\Users\ana\Documents\Descargas` y se recorren ambas
- **THEN** Descargas queda `nested`, sus archivos se recorren una sola vez y ninguno aparece dos veces en un grupo

#### Scenario: Carpeta redirigida a otro disco
- **WHEN** la carpeta Videos del usuario está en `D:\Videos`
- **THEN** Videos queda `unavailable` con motivo `unsupported_volume` y el resto del escaneo continúa

### Requirement: Recorrido sin seguir puntos de reanálisis ni carpetas de herramientas
El recorrido SHALL ser de solo lectura y MUST NOT seguir junctions, symlinks ni otros puntos de reanálisis, ni descender a carpetas de una lista cerrada de herramientas (`.git`, `.hg`, `.svn`, `node_modules`); las carpetas que no pueden listarse se cuentan como inaccesibles sin abortar.

#### Scenario: Junction dentro de Documentos
- **WHEN** `C:\Users\ana\Documents\j` es una junction hacia `C:\Windows`
- **THEN** no se recorre su destino y ningún archivo de `C:\Windows` aparece en los resultados

#### Scenario: Repositorio de código
- **WHEN** Documentos contiene `proyecto\node_modules\lib.bin` y `proyecto\.git\objects\pack\p.pack`
- **THEN** ninguno de esos archivos se examina y la cantidad de carpetas excluidas se informa

### Requirement: Verificación de protección de cada archivo
Cada archivo candidato SHALL decidirse con la protección de rutas antes de leer su contenido: solo los `allowed` pueden formar grupos; los `protected` y `skipped_cloud` se cuentan por motivo y MUST NOT leerse ni persistirse como copias. Duplicados dentro de rutas protegidas MUST NOT buscarse (RN-DUP-06).

#### Scenario: Archivo en un perfil de navegador accesible desde Documentos
- **WHEN** un archivo candidato resulta `protected` por la protección de rutas
- **THEN** no se abre para leer su contenido, no forma parte de ningún grupo y se suma a los protegidos del resultado

#### Scenario: Contexto de protección inválido
- **WHEN** el contexto de protección no puede construirse
- **THEN** ningún archivo es candidato y el escaneo termina sin grupos informando el motivo

### Requirement: Contenido en la nube nunca abierto
Un archivo marcado como contenido en la nube (marcador, sin datos locales o con tag de proveedor de nube, incluso hidratado) SHALL contarse como omitido en la nube y MUST NOT abrirse para leer su contenido, de modo que el escaneo nunca provoque una descarga.

#### Scenario: Carpeta Documentos en OneDrive
- **WHEN** Documentos está en `C:\Users\ana\OneDrive\Documents` y sus archivos son marcadores de nube
- **THEN** ninguno se lee, todos se cuentan como omitidos en la nube y no se descarga ningún contenido

### Requirement: Archivos con varios hard links excluidos
Un archivo con más de un nombre (hard links) SHALL excluirse de la búsqueda y contarse aparte; dos rutas con la misma identidad de archivo MUST NOT formar un grupo de duplicados.

#### Scenario: Hard link en Descargas
- **WHEN** `Descargas\a.iso` y `Documentos\a.iso` son dos nombres del mismo archivo
- **THEN** ninguno de los dos forma parte de un grupo y ambos se cuentan como excluidos por hard links

### Requirement: Tamaño mínimo con opciones cerradas
El escaneo SHALL ignorar los archivos menores que el tamaño mínimo, que por defecto es 1 MiB (SU-06). En modo avanzado el usuario MAY elegir uno de 100 KiB, 1 MiB, 10 MiB o 100 MiB para ese escaneo; cualquier otro valor MUST fallar con `invalid_argument`. El tamaño mínimo usado MUST quedar en el resultado del escaneo.

#### Scenario: Valor por defecto
- **WHEN** se escanea sin indicar tamaño mínimo y hay dos copias idénticas de 900 KiB y dos de 2 MiB
- **THEN** solo las de 2 MiB forman un grupo y el resultado informa un mínimo de 1 MiB

#### Scenario: Valor fuera de la lista
- **WHEN** se pide un escaneo con tamaño mínimo de 5 bytes
- **THEN** la petición falla con `invalid_argument`

### Requirement: Detección en tres etapas
La detección SHALL avanzar por etapas: (1) agrupar por tamaño y descartar tamaños únicos; (2) hash parcial blake3 de los primeros y los últimos 64 KiB y descartar parciales únicos; (3) hash blake3 completo. Un archivo de hasta 128 KiB MUST usar su contenido completo como hash parcial y no leerse de nuevo.

#### Scenario: Tamaño único
- **WHEN** hay un solo archivo de 3 MiB entre los candidatos
- **THEN** su contenido no se lee

#### Scenario: Parcial distinto
- **WHEN** dos archivos de 5 MiB difieren en sus primeros 64 KiB
- **THEN** no se calcula el hash completo de ninguno y no forman grupo

### Requirement: Solo es duplicado lo que coincide en hash completo
Un grupo SHALL formarse solo con dos o más archivos de igual tamaño e igual hash blake3 completo, con identidades de archivo distintas (RN-DUP-02); coincidir en tamaño y hash parcial MUST NOT bastar.

#### Scenario: Grupo de dos copias
- **WHEN** `Descargas\foto.jpg` y `Imágenes\foto (1).jpg` tienen el mismo contenido de 4 MiB
- **THEN** forman un grupo de 2 copias con tamaño 4 MiB, el hash completo y 4 MiB recuperables

#### Scenario: Grupo de tres copias
- **WHEN** tres archivos de 10 MiB tienen el mismo contenido
- **THEN** forman un único grupo de 3 copias con 20 MiB recuperables

#### Scenario: Falso positivo del hash parcial
- **WHEN** dos archivos de 5 MiB tienen iguales los primeros y los últimos 64 KiB pero difieren en un byte del medio
- **THEN** no forman grupo

### Requirement: Lectura del contenido verificada
Al leer un archivo para hashearlo, el sistema SHALL abrirlo sin seguir puntos de reanálisis y sin impedir que otros procesos lo lean, escriban o borren, y comparar la identidad del archivo abierto con la verificada; si difiere, o si su tamaño o última escritura cambian durante la lectura, el archivo MUST excluirse y contarse como cambiado. Un archivo que no puede abrirse se cuenta como ilegible.

#### Scenario: Archivo reemplazado antes de leerlo
- **WHEN** entre la verificación y la lectura `a.mkv` se reemplaza por otro archivo con el mismo nombre
- **THEN** `a.mkv` no forma parte de ningún grupo y se cuenta como cambiado

#### Scenario: Archivo escrito durante la lectura
- **WHEN** otro proceso agranda `b.zip` mientras se calcula su hash completo
- **THEN** `b.zip` no forma parte de ningún grupo y se cuenta como cambiado

### Requirement: Escaneo de solo lectura
El escaneo de duplicados SHALL limitarse a listar carpetas, leer metadatos y leer contenido; MUST NOT mover, renombrar, modificar ni borrar ningún archivo ni carpeta.

#### Scenario: Escaneo completo
- **WHEN** termina un escaneo con 5 grupos
- **THEN** todos los archivos examinados siguen en su lugar sin cambios

### Requirement: Escaneo y preview bloqueados en mismatch de usuario
Con el contexto de privilegios en mismatch (otro usuario o indeterminado), iniciar un escaneo de duplicados o pedir un preview de duplicados SHALL fallar con `duplicates_blocked` y el motivo (`different_user` o `undetermined`), sin crear escaneo ni preview y sin recorrer nada.

#### Scenario: Over-the-shoulder
- **WHEN** la app corre elevada con la cuenta de otro administrador y se pide un escaneo de duplicados
- **THEN** la petición falla con `duplicates_blocked` y motivo `different_user`, y no se lee ninguna carpeta

#### Scenario: Preview con usuario indeterminado
- **WHEN** el contexto de privilegios es `undetermined` y se pide un preview de duplicados
- **THEN** la petición falla con `duplicates_blocked` y motivo `undetermined`, y no se crea ningún preview

### Requirement: Progreso y cancelación del escaneo de duplicados
Durante el escaneo el sistema SHALL informar inicio con el identificador del escaneo y progreso por etapa (archivos vistos; parciales hechos y total; bytes hasheados y total) sin bloquear la interfaz. El usuario SHALL poder cancelarlo con la cancelación de escaneos existente: se detiene en el próximo archivo o bloque leído, queda `cancelled` y no se guarda ningún grupo.

#### Scenario: Cancelación durante el hash completo
- **WHEN** se cancela mientras se hashea un archivo de 2 GiB
- **THEN** la lectura se detiene antes de terminar ese archivo, el escaneo queda `cancelled` y no hay grupos guardados de ese escaneo

#### Scenario: Progreso por etapas
- **WHEN** se inicia un escaneo con candidatos repetidos
- **THEN** la interfaz recibe el aviso de inicio, avisos de cada etapa y el resultado final

### Requirement: Un trabajo a la vez
El escaneo de duplicados y la ejecución de su borrado SHALL compartir el lugar de trabajo único de escaneos, limpiezas y vaciado de la Papelera: iniciarlos con otro trabajo en curso MUST fallar con `scan_in_progress`, `cleanup_in_progress` o `recycle_bin_empty_in_progress` según el trabajo activo, sin afectarlo.

#### Scenario: Escaneo de basura durante el de duplicados
- **WHEN** se pide un escaneo de basura mientras corre uno de duplicados
- **THEN** la petición falla con `scan_in_progress` y el de duplicados continúa

#### Scenario: Duplicados durante el vaciado
- **WHEN** se pide un escaneo de duplicados mientras se vacía la Papelera
- **THEN** la petición falla con `recycle_bin_empty_in_progress` y no se crea ningún escaneo

### Requirement: Resultado y grupos consultables
El sistema SHALL ofrecer el resumen del último escaneo de duplicados terminado (fechas, tamaño mínimo, estado por carpeta, contadores de omitidos por motivo, cantidad de grupos, copias y bytes recuperables) y sus grupos paginados, ordenados por bytes recuperables descendentes y luego por identificador, filtrables por carpeta y por tamaño mínimo. Cada copia MUST informar su identificador, carpeta de contenido, ruta, ruta relativa a la carpeta, tamaño y última escritura.

#### Scenario: Sin escaneos
- **WHEN** nunca se terminó un escaneo de duplicados
- **THEN** el resumen es vacío y la lista de grupos también

#### Scenario: Filtro por carpeta
- **WHEN** se piden los grupos filtrando por `pictures`
- **THEN** solo se devuelven los grupos con al menos una copia en Imágenes, con el total filtrado

### Requirement: Ninguna copia preseleccionada
Ningún grupo SHALL presentarse con una copia elegida para conservar ni para eliminar, en ningún modo; el sistema MUST NOT marcar ninguna copia como conservada sin una decisión explícita del usuario confirmada (RN-DUP-03), y la marca de conservada solo se escribe al resolver un grupo confirmado.

#### Scenario: Grupos recién encontrados
- **WHEN** se listan los grupos de un escaneo recién terminado
- **THEN** ninguna copia figura como conservada y la interfaz no muestra ninguna opción marcada

#### Scenario: Nada confirmado
- **WHEN** el usuario elige copias en varios grupos y cierra la revisión sin confirmar
- **THEN** ninguna copia queda marcada como conservada en la base

### Requirement: Al menos una copia conservada por grupo
Cada decisión SHALL conservar al menos una copia y eliminar al menos una. El backend MUST rechazar con `duplicate_keep_required` una decisión que no conserve ninguna copia (eliminaría todas), y con `invalid_argument` una que no elimine ninguna, que repita un grupo o que nombre copias de otro grupo (RN-DUP-04).

#### Scenario: Intento de eliminar todas las copias
- **WHEN** se pide un preview para un grupo de 3 copias sin conservar ninguna
- **THEN** la petición falla con `duplicate_keep_required` y no se crea ningún preview

#### Scenario: Copia de otro grupo
- **WHEN** una decisión conserva en el grupo 4 una copia que pertenece al grupo 9
- **THEN** la petición falla con `invalid_argument`

#### Scenario: Conservar dos de tres
- **WHEN** en modo avanzado se conservan 2 copias de un grupo de 3
- **THEN** el preview incluye solo la tercera copia de ese grupo

### Requirement: Preview de copias a eliminar
Pedir un preview con decisiones SHALL validar que el escaneo es el último de duplicados terminado (`scan_not_found` si no), que cada grupo existe en él (`duplicate_group_not_found`) y no está resuelto (`duplicate_group_resolved`), y devolver un preview de limpieza de un solo uso, con vencimiento, con las copias a eliminar, la cantidad de grupos, copias y bytes, las excluidas por escritura reciente y la disponibilidad de la Papelera, sin acceder al contenido.

#### Scenario: Preview válido
- **WHEN** se piden 2 grupos con una copia a eliminar de 4 MiB y otra de 10 MiB
- **THEN** el preview informa 2 grupos, 2 copias y 14 MiB, con su identificador y su vencimiento

#### Scenario: Grupo ya resuelto
- **WHEN** se pide un preview con un grupo que ya se resolvió
- **THEN** la petición falla con `duplicate_group_resolved`

#### Scenario: Copia escrita en las últimas 24 horas
- **WHEN** una copia a eliminar tiene última escritura hace 2 horas
- **THEN** el preview la informa como reciente y no la incluye entre las que se moverán

### Requirement: Eliminación por el flujo de limpieza
Confirmar un preview de duplicados SHALL ejecutar la limpieza estándar (re-verificación de cada copia contra el disco y los privilegios de ese momento, guardas de cambio y de 24 h, solo Papelera, un resultado por ítem, abortos, progreso, cancelación y registro append-only con categoría `duplicates`) y MUST fallar con `preview_stale` si el escaneo dejó de ser el último de duplicados terminado.

#### Scenario: Copias movidas
- **WHEN** se confirma un preview de 2 copias que pasan todas las verificaciones
- **THEN** ambas quedan `moved_to_trash`, las copias conservadas siguen en su lugar y el log tiene 2 entradas con categoría `duplicates` y el escaneo de duplicados

#### Scenario: Escaneo nuevo entre preview y confirmación
- **WHEN** termina otro escaneo de duplicados después del preview y se confirma
- **THEN** la confirmación falla con `preview_stale` y no se mueve nada

### Requirement: Contenido re-verificado antes de mover cada copia
Justo antes de mover una copia, con su ruta ya re-verificada y fijada, el sistema SHALL volver a calcular su hash completo leyendo el archivo abierto. Sin moverla, MUST quedar `skipped_locked` con `content_changed` si el hash difiere del grupo, `skipped_locked` con `in_use` o `content_unreadable` si no puede leerse, y `skipped_protected` con `identity_changed` si el archivo abierto no es el verificado.

#### Scenario: Copia editada después del escaneo con la misma fecha
- **WHEN** una copia a eliminar cambió de contenido sin cambiar tamaño ni fecha
- **THEN** queda `skipped_locked` con detalle `content_changed` y sigue en su lugar

#### Scenario: Copia abierta sin compartir lectura
- **WHEN** otro proceso tiene abierta una copia a eliminar sin compartir lectura
- **THEN** queda `skipped_locked` con detalle `in_use` y sigue en su lugar

### Requirement: Copia conservada verificada antes de mover las demás
Antes de mover la primera copia de un grupo, el sistema SHALL verificar que al menos una copia conservada sigue en su ruta con la misma identidad, tamaño, fecha y hash completo; si ninguna lo cumple, todas las copias a eliminar de ese grupo MUST quedar `skipped_protected` con detalle `kept_copy_unavailable` y ninguna se mueve.

#### Scenario: Copia conservada borrada por el usuario
- **WHEN** después del preview el usuario borró la única copia que eligió conservar y se confirma
- **THEN** ninguna copia de ese grupo se mueve, todas quedan `skipped_protected` con `kept_copy_unavailable` y los demás grupos se procesan normalmente

#### Scenario: Copia conservada modificada
- **WHEN** la copia conservada cambió de contenido después del escaneo
- **THEN** ninguna copia de ese grupo se mueve y quedan `skipped_protected` con `kept_copy_unavailable`

### Requirement: Cancelación durante la re-verificación del contenido
Si el usuario cancela mientras se re-hashea una copia, la lectura SHALL detenerse, esa copia MUST quedar sin procesar (no se mueve ni se registra) y la ejecución termina `cancelled` con las restantes como no procesadas.

#### Scenario: Cancelación a mitad de un archivo grande
- **WHEN** se cancela mientras se re-hashea una copia de 4 GiB
- **THEN** esa copia sigue en su lugar sin entrada en el log y el resultado es `cancelled`

### Requirement: Resolución de los grupos procesados
Al terminar una ejecución, los grupos con al menos una copia procesada SHALL quedar resueltos, con fecha y con la marca de conservada en las copias que el usuario eligió, de forma atómica; los grupos sin copias procesadas (cancelación o aborto) MUST quedar sin resolver y sin marcas. Un grupo resuelto MUST NOT admitir otra decisión hasta un escaneo nuevo.

#### Scenario: Ejecución completa
- **WHEN** termina una ejecución sobre 3 grupos
- **THEN** los 3 quedan resueltos y solo las copias elegidas tienen la marca de conservada

#### Scenario: Cancelación antes del tercer grupo
- **WHEN** se cancela la ejecución después de procesar las copias de 2 de 3 grupos
- **THEN** 2 grupos quedan resueltos y el tercero sigue disponible para decidir

### Requirement: Comandos IPC de duplicados sin rutas de entrada
El backend SHALL exponer iniciar un escaneo de duplicados (carpetas y tamaño mínimo opcionales, progreso por el canal de la invocación), consultar el último resumen, listar grupos paginados con filtros, pedir un preview con decisiones (identificadores de grupo y de copias a conservar) y confirmarlo; ningún argumento MUST aceptar rutas, y los resúmenes de ejecución MUST NOT incluir rutas, SIDs ni nombres de cuenta.

#### Scenario: Firma de los comandos
- **WHEN** se revisan los argumentos de los comandos de duplicados
- **THEN** solo aceptan claves de carpeta, un tamaño mínimo de la lista cerrada, identificadores de escaneo, grupo, copia y preview, y paginación

#### Scenario: Identificador fuera de rango
- **WHEN** se pide un preview con un identificador de grupo negativo o mayor que 2^53 - 1
- **THEN** la petición falla con `invalid_argument`

### Requirement: Pantalla Duplicados en modo simple
La pantalla Duplicados SHALL ofrecer una acción primaria por estado: "Buscar duplicados" sin resultados; con grupos, un listado de grupos (nombre, tamaño, cantidad de copias y espacio recuperable) donde cada grupo pregunta "¿Cuál querés conservar?" con una opción por copia, ninguna marcada, mostrando la carpeta de contenido, la ruta relativa y la fecha de cada copia; y "Revisar y mover a la Papelera" habilitado cuando al menos un grupo tiene una copia elegida.

#### Scenario: Elección en un grupo
- **WHEN** el usuario elige conservar `Imágenes\foto (1).jpg` en un grupo de 2 copias
- **THEN** la otra copia se indica como "va a la Papelera" y la acción de revisar queda habilitada

#### Scenario: Sin elección
- **WHEN** hay grupos pero ninguno tiene una copia elegida
- **THEN** la acción de revisar está deshabilitada con su motivo visible

### Requirement: Elección guiada sin recomendación
La ayuda del modo simple SHALL limitarse a datos neutrales de cada copia (carpeta, ruta relativa, fecha) y a un texto que explique que las demás copias van a la Papelera y pueden recuperarse; MUST NOT destacar ninguna copia como recomendada ni ofrecer elegir en lote.

#### Scenario: Copias con fechas distintas
- **WHEN** un grupo tiene una copia de 2019 en Documentos y otra de ayer en Descargas
- **THEN** ambas se muestran con su carpeta y su fecha, sin insignia de recomendada y sin ninguna marcada

### Requirement: Detalle en modo avanzado
En modo avanzado la pantalla SHALL permitir elegir las carpetas y el tamaño mínimo antes de buscar, mostrar rutas completas y el hash blake3 de cada grupo, filtrar por carpeta y tamaño mínimo, mostrar los omitidos por motivo y por carpeta, y conservar más de una copia por grupo (casillas "Conservar"), sin permitir dejar un grupo sin ninguna copia conservada.

#### Scenario: Desmarcar la última copia conservada
- **WHEN** en un grupo con una sola casilla "Conservar" marcada el usuario intenta desmarcarla
- **THEN** la casilla no se desmarca y se anuncia que debe quedar al menos una copia

#### Scenario: Hash visible
- **WHEN** el modo es avanzado y se muestra un grupo
- **THEN** se ve su hash blake3 y la ruta completa de cada copia

### Requirement: Revisión, progreso y resumen accesibles
"Revisar y mover a la Papelera" SHALL abrir un diálogo modal con grupos, copias y tamaño total, la aclaración de que todo va a la Papelera y de que se conserva la copia elegida en cada grupo, foco inicial en "Cancelar" y un único botón para confirmar; cerrarlo MUST NOT ejecutar nada. El progreso y el resumen se anuncian por una región viva y el foco pasa al título del resumen.

#### Scenario: Esc en la revisión
- **WHEN** el diálogo de revisión está abierto y el usuario pulsa Esc
- **THEN** el diálogo se cierra, no se mueve nada y el foco vuelve a "Revisar y mover a la Papelera"

#### Scenario: Resumen
- **WHEN** termina una ejecución con 2 copias movidas y 1 con contenido cambiado
- **THEN** el resumen dice cuántas copias y bytes se movieron a la Papelera, que el espacio se libera al vaciarla, y cuántas se omitieron por haber cambiado

### Requirement: Tests sin los archivos reales del usuario
Los tests automáticos de duplicados SHALL usar dobles en memoria o directorios temporales; los que dependan de las carpetas reales del usuario, de contenido en la nube o de privilegios MUST marcarse ignorados y correrse a mano.

#### Scenario: Suite automática
- **WHEN** se ejecuta la suite completa sin argumentos extra
- **THEN** ningún test lee las carpetas de contenido reales del usuario ni mueve archivos reales a la Papelera

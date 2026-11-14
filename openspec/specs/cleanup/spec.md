# cleanup Specification

## Purpose

Mover a la Papelera, con preview y confirmación explícita, solo los hallazgos de basura verificados del último escaneo, re-verificando cada ítem contra el disco y el contexto de privilegios justo antes de moverlo, sin borrar nunca de forma permanente y dejando un registro append-only de cada resultado.

## Requirements

### Requirement: Preview solo sobre el último escaneo de basura terminado
El sistema SHALL armar un preview únicamente sobre el último escaneo de basura `done` y solo si ese escaneo todavía no fue limpiado; pedirlo para otro escaneo MUST fallar con `scan_not_found` y para un escaneo ya limpiado con `scan_already_cleaned`.

#### Scenario: Escaneo vigente
- **WHEN** se pide un preview del último escaneo `done`, que no tiene limpiezas registradas
- **THEN** se devuelve un preview con identificador, cantidad y tamaño

#### Scenario: Escaneo anterior
- **WHEN** se pide un preview de un escaneo que no es el último escaneo de basura `done`
- **THEN** la petición falla con `scan_not_found`

#### Scenario: Escaneo ya limpiado
- **WHEN** se pide un preview de un escaneo sobre el que ya se ejecutó una limpieza que procesó al menos un ítem
- **THEN** la petición falla con `scan_already_cleaned`

### Requirement: Selección por categorías limpiables
El preview SHALL incluir solo las categorías pedidas, que MUST ser categorías de la whitelist con hallazgos movibles: `recycle_bin` y `old_logs` no son limpiables en este flujo. Una lista vacía, una categoría desconocida, repetida o no limpiable MUST fallar con `invalid_argument`. La interfaz MUST NOT enviar rutas.

#### Scenario: Subconjunto de categorías
- **WHEN** el escaneo tiene elegibles en `temp_user` y `browser_cache_chrome` y se pide el preview solo de `temp_user`
- **THEN** el preview incluye únicamente los elegibles de `temp_user`

#### Scenario: Papelera pedida como categoría
- **WHEN** se pide un preview que incluye `recycle_bin`
- **THEN** la petición falla con `invalid_argument` y no se crea ningún preview

#### Scenario: Lista vacía o repetida
- **WHEN** se pide un preview sin categorías o con `temp_user` dos veces
- **THEN** la petición falla con `invalid_argument`

### Requirement: Contenido del preview sin acceder al disco
El preview SHALL calcularse solo con los hallazgos persistidos, sin abrir ni listar archivos: incluye los elegibles de cada categoría pedida e informa por categoría estado (`included` o `blocked` con motivo), cantidad y bytes a mover, y cantidad y bytes excluidos por escritura reciente, más los totales. Los omitidos del escaneo MUST NOT incluirse.

#### Scenario: Totales
- **WHEN** `temp_user` tiene 3 elegibles de 100, 200 y 300 bytes escritos hace más de 24 h y 1 omitido protegido
- **THEN** el preview informa para `temp_user` 3 ítems y 600 bytes, y esos mismos totales generales

#### Scenario: Sin acceso al disco
- **WHEN** se arma un preview
- **THEN** no se realiza ninguna operación sobre el sistema de archivos ni sobre la Papelera

### Requirement: Categorías por-usuario bloqueadas en el preview
Con el contexto de privilegios en `mismatch`, el preview SHALL informar cada categoría de alcance por-usuario pedida como `blocked` con el motivo del mismatch y sin ítems; las categorías de máquina y la mixta (`crash_dumps`) MUST seguir incluidas.

#### Scenario: Over-the-shoulder con hallazgos de una sesión anterior
- **WHEN** los hallazgos se guardaron en una sesión `match` y el preview se pide en una sesión `mismatch/different_user` para `temp_user` y `temp_windows`
- **THEN** `temp_user` figura `blocked` con motivo `different_user` y 0 ítems, y `temp_windows` incluye sus elegibles

### Requirement: Escritura reciente excluida
Un archivo cuya última escritura sea nula o de menos de 24 horas antes del momento de evaluar SHALL excluirse de la limpieza en todas las categorías: en el preview con la fecha persistida y al confirmar con la fecha leída del archivo abierto.

#### Scenario: Escrito hace 23 horas
- **WHEN** un elegible de `wu_cache` tiene última escritura persistida 23 h antes del preview
- **THEN** no cuenta en los ítems a mover y sí en los excluidos por escritura reciente de su categoría

#### Scenario: Sin fecha
- **WHEN** un elegible no tiene fecha de última escritura
- **THEN** se excluye como escrito recientemente

### Requirement: Preview de un solo uso con vencimiento
Cada preview SHALL tener un identificador único en la ejecución de la app; crear otro MUST invalidar el anterior, confirmarlo MUST consumirlo aunque la ejecución falle, y MUST vencer a los 10 minutos de creado. Confirmar un identificador desconocido, reemplazado o consumido MUST fallar con `preview_not_found`, y uno vencido con `preview_expired`, sin mover nada.

#### Scenario: Preview reemplazado
- **WHEN** se crea el preview 1, luego el preview 2, y se confirma el 1
- **THEN** la confirmación falla con `preview_not_found` y no se mueve nada

#### Scenario: Doble confirmación del mismo preview
- **WHEN** se confirma un preview y, terminada la ejecución, se vuelve a confirmar el mismo identificador
- **THEN** la segunda confirmación falla con `preview_not_found`

#### Scenario: Preview vencido
- **WHEN** se confirma un preview creado hace 11 minutos
- **THEN** la confirmación falla con `preview_expired` y no se mueve nada

#### Scenario: Cerrar el preview
- **WHEN** la interfaz descarta un preview sin confirmarlo
- **THEN** no se mueve ningún archivo ni se escribe ninguna entrada de log

### Requirement: Confirmación solo del conjunto exacto mostrado
La confirmación SHALL procesar exactamente los ítems que el preview incluyó, en su orden, y MUST fallar con `preview_stale` sin mover nada si desde el preview terminó otro escaneo de basura o el escaneo del preview fue limpiado. Un preview sin ítems MUST fallar con `nothing_to_clean`.

#### Scenario: Escaneo nuevo entre preview y confirmación
- **WHEN** después de crear el preview termina otro escaneo de basura y luego se confirma
- **THEN** la confirmación falla con `preview_stale` y no se mueve nada

#### Scenario: Preview vacío
- **WHEN** se confirma un preview con 0 ítems
- **THEN** la confirmación falla con `nothing_to_clean`

### Requirement: Privilegios re-evaluados al confirmar
Al confirmar, el sistema SHALL volver a consultar el acceso del alcance por-usuario y volver a resolver el contexto de protección restringido por el contexto de privilegios vigente, sin reutilizar el del escaneo ni el del preview. Un ítem de categoría por-usuario bloqueada MUST omitirse como `skipped_protected` con detalle `blocked_different_user` o `blocked_undetermined` sin tocar el disco.

#### Scenario: Contexto distinto al confirmar
- **WHEN** el preview incluyó `temp_user` y la confirmación se evalúa con un contexto `mismatch/undetermined`
- **THEN** cada ítem de `temp_user` queda `skipped_protected` con detalle `blocked_undetermined`, ninguna ruta del perfil se abre ni se fija y nada se mueve

#### Scenario: Volcado del perfil en mismatch
- **WHEN** en `mismatch` se confirma un ítem de `crash_dumps` ubicado en el perfil del usuario
- **THEN** la re-verificación lo deja `skipped_protected` con detalle `other_user_profile` y no se mueve

### Requirement: Re-verificación de cada ítem justo antes de moverlo
Inmediatamente antes de mover cada ítem, el sistema SHALL fijar sus ancestros, repetir la verificación completa de protección sobre la ruta real persistida y comparar identidad (volumen e id de archivo) y tipo con lo persistido; la fijación MUST mantenerse hasta que el movimiento retorne y la ruta movida MUST ser la ruta real verificada.

#### Scenario: Archivo reemplazado por otro con el mismo nombre
- **WHEN** entre el escaneo y la confirmación `a.tmp` se borra y se crea otro `a.tmp`
- **THEN** el ítem queda `skipped_protected` con detalle `identity_changed` y el archivo nuevo no se mueve

#### Scenario: Archivo reemplazado por un symlink o junction
- **WHEN** entre el escaneo y la confirmación `a.tmp` se reemplaza por un symlink hacia `C:\Windows\System32\x.dll`
- **THEN** el ítem queda `skipped_protected` con detalle `reparse_point` y ni el enlace ni su destino se mueven

#### Scenario: Hard link creado después del escaneo
- **WHEN** después del escaneo se crea un segundo nombre para `a.tmp` en otra carpeta
- **THEN** el ítem queda `skipped_protected` con detalle `multiple_hard_links`

#### Scenario: Carpeta padre reemplazada por una junction
- **WHEN** entre el escaneo y la confirmación la carpeta padre se reemplaza por una junction hacia `C:\Windows`
- **THEN** el ítem queda `skipped_protected` y no se mueve nada bajo `C:\Windows`

#### Scenario: Archivo que pasó a la nube
- **WHEN** al confirmar el archivo o un ancestro tiene marca de contenido en la nube
- **THEN** el ítem queda `skipped_cloud` y no se mueve

#### Scenario: Archivo que ya no existe
- **WHEN** al confirmar el archivo ya no existe
- **THEN** el ítem queda `skipped_protected` con detalle `unresolvable_not_found` y el resto continúa

### Requirement: Archivos cambiados desde el escaneo omitidos
Si el tamaño o la fecha de última escritura leídos del archivo abierto durante la re-verificación difieren de los persistidos, el ítem SHALL omitirse como `skipped_locked` con detalle `changed_since_scan`; si la fecha leída es reciente según la regla de escritura reciente, como `skipped_locked` con detalle `recently_modified`.

#### Scenario: Caché de un navegador abierto
- **WHEN** un archivo de caché conserva su identidad pero su fecha leída del archivo abierto es más nueva que la persistida
- **THEN** el ítem queda `skipped_locked` con detalle `changed_since_scan` y no se mueve

#### Scenario: Temporal escrito después del escaneo
- **WHEN** un elegible de `temp_user` con el mismo tamaño y fecha que al escanear fue escrito hace 2 horas según el archivo abierto
- **THEN** el ítem queda `skipped_locked` con detalle `recently_modified`

### Requirement: Solo a la Papelera, nunca borrado permanente
Todo ítem que se limpie SHALL moverse a la Papelera; el sistema MUST NOT borrarlo de forma permanente por ningún camino, tampoco si la Papelera no está disponible o el ítem no entra en ella, y MUST NOT vaciar la Papelera en este flujo (vaciarla es la única excepción permanente declarada y vive en su propio flujo de doble confirmación).

#### Scenario: Ítem movido
- **WHEN** un ítem pasa todas las verificaciones y la Papelera lo acepta
- **THEN** queda `moved_to_trash`, deja de estar en su ruta original y puede restaurarse desde la Papelera

#### Scenario: Papelera deshabilitada en el volumen
- **WHEN** la Papelera del volumen del sistema está deshabilitada y se confirma una limpieza
- **THEN** el archivo sigue en su lugar, no se destruye nada y la ejecución se aborta con `recycle_bin_unavailable` sin intentar los demás (por la comprobación previa, o por el control por ítem del adaptador si la política cambió a mitad de la ejecución)

#### Scenario: Ítem más grande que la Papelera
- **WHEN** la Papelera rechaza un ítem por tamaño
- **THEN** el ítem queda `error` con detalle `recycle_bin_too_small`, el archivo sigue en su lugar y el resto continúa

#### Scenario: Papelera deshabilitada detectada antes de mover
- **WHEN** la política de la Papelera del volumen del sistema la declara deshabilitada (`NukeOnDelete`, `NoRecycleFiles`) o no puede leerse, y se confirma una limpieza
- **THEN** la ejecución se aborta con `recycle_bin_unavailable` antes de mover el primer ítem, no se mueve ni se registra ningún ítem y todos los archivos siguen en su lugar

#### Scenario: Control por ítem dentro del adaptador
- **WHEN** la política cambia a deshabilitada o ilegible entre el preflight y un ítem
- **THEN** el adaptador de Papelera se niega a moverlo sin invocar al shell, el ítem queda `error` con detalle `recycle_bin_unavailable` y la ejecución se aborta

#### Scenario: Ítem mayor que la capacidad máxima de la Papelera
- **WHEN** el tamaño verificado de un ítem supera la capacidad máxima configurada de la Papelera del volumen
- **THEN** el adaptador se niega a moverlo sin invocar al shell, el ítem queda `error` con detalle `recycle_bin_too_small` y el resto continúa

#### Scenario: Aviso en el preview
- **WHEN** se arma el preview con la Papelera deshabilitada o con política ilegible
- **THEN** el preview informa que la Papelera no está disponible para que la interfaz advierta y no ofrezca confirmar

#### Scenario: Ninguna ruta de código borra de forma permanente
- **WHEN** se revisa el código de producción
- **THEN** no contiene llamadas de borrado permanente de archivos, y la única llamada de vaciado de la Papelera es la del adaptador del vaciado con doble confirmación (capacidad `recycle-bin-empty`), en un solo archivo y fuera de este flujo

### Requirement: Archivos en uso omitidos sin forzar
Un ítem que otro proceso tiene abierto de forma que impide moverlo SHALL omitirse como `skipped_locked` (detalle `in_use` o `unresolvable_in_use`); el sistema MUST NOT reintentar, cerrar procesos ni programar el borrado para el reinicio.

#### Scenario: Archivo abierto sin compartir
- **WHEN** al mover un ítem el sistema informa violación de compartición
- **THEN** el ítem queda `skipped_locked` con detalle `in_use`, sigue en su lugar y la ejecución continúa con el siguiente

### Requirement: Un fallo por ítem no detiene la ejecución
Cada ítem SHALL terminar con exactamente uno de `moved_to_trash`, `skipped_locked`, `skipped_protected`, `skipped_cloud` o `error`, con un código de detalle estable para todo resultado distinto de `moved_to_trash`; un `error` de un ítem MUST NOT detener el resto, salvo los abortos definidos.

#### Scenario: Error intermedio
- **WHEN** se confirman 3 ítems y el segundo falla con acceso denegado
- **THEN** el primero y el tercero quedan `moved_to_trash` y el segundo `error` con detalle `permission_denied`

#### Scenario: Éxito informado pero el archivo sigue
- **WHEN** la Papelera informa éxito y el archivo sigue en su ruta con la misma identidad
- **THEN** el ítem queda `error` con detalle `not_moved`

### Requirement: Aborto sin seguir moviendo
La ejecución SHALL abortarse, sin procesar más ítems, cuando la Papelera no está disponible, cuando el mecanismo de Papelera no puede usarse (detalle `trash_unavailable`) o cuando no puede escribirse el registro de un ítem; el resultado MUST informar `aborted` con el motivo y lo ya movido MUST quedar en la Papelera.

#### Scenario: Log no disponible
- **WHEN** al registrar el segundo ítem la base falla
- **THEN** la ejecución termina `aborted` con motivo `log_unavailable` y no se intenta mover el tercer ítem

#### Scenario: Mecanismo de Papelera inutilizable
- **WHEN** el mecanismo de Papelera falla de forma interna al primer ítem
- **THEN** la ejecución termina `aborted` con motivo `trash_unavailable`, la app sigue funcionando y no se intenta ningún otro ítem

### Requirement: Registro append-only de cada ítem procesado
Cada ítem procesado SHALL registrarse en `cleanup_log` apenas se conoce su resultado, con fecha, categoría, ruta real (o la persistida si no llegó a verificarse), tamaño leído del archivo abierto (o el persistido), resultado, código de detalle y escaneo de origen. Los ítems no procesados por cancelación o aborto MUST NOT registrarse.

#### Scenario: Entradas de una ejecución
- **WHEN** termina una ejecución de 3 ítems con un movido, un en uso y un protegido
- **THEN** el log tiene 3 entradas nuevas con esos resultados, sus detalles y el escaneo del preview

#### Scenario: Registro inmutable
- **WHEN** se intenta modificar o borrar una entrada registrada por la limpieza
- **THEN** la base lo rechaza

### Requirement: Progreso y cancelación de la limpieza
Durante la ejecución, el sistema SHALL informar inicio con el total y progreso periódico con procesados, movidos y bytes movidos, sin bloquear la interfaz. El usuario SHALL poder cancelarla: el ítem en curso termina y se registra, no se procesan más y el resultado es `cancelled` con los no procesados. Cancelar una ejecución que no está en curso MUST fallar con `preview_not_found`.

#### Scenario: Cancelación a mitad
- **WHEN** se cancela una ejecución de 100 ítems después del ítem 40
- **THEN** el resultado es `cancelled`, hay a lo sumo 41 entradas de log nuevas, los ítems restantes siguen en su lugar y se informan como no procesados

#### Scenario: Progreso
- **WHEN** se confirma un preview de 100 ítems
- **THEN** la interfaz recibe un aviso de inicio con total 100, avisos de progreso y el resumen final

### Requirement: Escaneo y limpieza mutuamente excluyentes
Como máximo SHALL correr un escaneo, una limpieza o un vaciado de la Papelera a la vez: iniciar un escaneo durante una limpieza MUST fallar con `cleanup_in_progress`; pedir un preview o confirmar durante un escaneo MUST fallar con `scan_in_progress`; una segunda confirmación durante una limpieza MUST fallar con `cleanup_in_progress`; pedir un preview o confirmar durante un vaciado MUST fallar con `recycle_bin_empty_in_progress`.

#### Scenario: Escaneo durante la limpieza
- **WHEN** se pide un escaneo mientras una limpieza está en curso
- **THEN** la petición falla con `cleanup_in_progress` y la limpieza continúa

#### Scenario: Preview durante un escaneo
- **WHEN** se pide un preview mientras un escaneo está en curso
- **THEN** la petición falla con `scan_in_progress`

#### Scenario: Limpieza durante el vaciado
- **WHEN** se pide un preview o se confirma una limpieza mientras la Papelera se está vaciando
- **THEN** la petición falla con `recycle_bin_empty_in_progress` y no se mueve nada

### Requirement: Comandos IPC de limpieza
El backend SHALL exponer pedir un preview (escaneo y categorías), confirmarlo (identificador del preview, con progreso por el canal de la invocación) y cancelar la ejecución en curso. Las respuestas MUST NOT incluir rutas, SIDs ni nombres de cuenta, y la interfaz MUST NOT enviar rutas.

#### Scenario: Resumen sin rutas
- **WHEN** termina una limpieza
- **THEN** el resumen informa estado, motivo de aborto si lo hubo, totales, procesados, no procesados, movidos y bytes, omitidos por tipo, errores, conteo por resultado y detalle, y totales por categoría, sin ninguna ruta

#### Scenario: Argumentos con rutas ignorados por diseño
- **WHEN** se revisa la firma de los comandos de limpieza
- **THEN** ningún argumento acepta rutas: solo identificador de escaneo, categorías e identificador de preview

### Requirement: Diálogo de preview con confirmación explícita
La pantalla Limpieza SHALL ofrecer "Limpiar" cuando el último escaneo tiene elegibles, no fue limpiado y no hay operaciones en curso. Pulsarlo SHALL abrir un diálogo modal con las categorías marcables, cantidad y tamaño por categoría y total, las bloqueadas deshabilitadas con su motivo y la aclaración de que todo va a la Papelera, con foco inicial en "Cancelar". Cerrarlo MUST NOT ejecutar nada.

#### Scenario: Apertura
- **WHEN** el usuario pulsa "Limpiar"
- **THEN** se abre el diálogo con todas las categorías limpiables marcadas, el total a mover y el foco en "Cancelar"

#### Scenario: Cambio de selección
- **WHEN** el usuario desmarca una categoría
- **THEN** se pide un preview nuevo, el botón de confirmar queda deshabilitado hasta recibirlo y luego muestra la nueva cantidad

#### Scenario: Escape
- **WHEN** el usuario pulsa Esc con el diálogo abierto
- **THEN** el diálogo se cierra, el foco vuelve a "Limpiar" y no se confirma nada

#### Scenario: Preview vencido al confirmar
- **WHEN** la confirmación falla con `preview_expired`
- **THEN** la interfaz pide un preview nuevo, lo muestra con el aviso de que se actualizó y no confirma automáticamente

### Requirement: Progreso y resumen accesibles
Durante la ejecución la pantalla SHALL anunciar el avance en una región `aria-live` educada y ofrecer "Cancelar"; al terminar SHALL mostrar un resumen en lenguaje llano (movido a la Papelera, que el espacio se libera al vaciarla, omitidos por familia, errores, cancelación o aborto) con el foco en su título y la acción "Buscar de nuevo".

#### Scenario: Resumen en modo simple
- **WHEN** termina una limpieza con 10 movidos de 1 MB en total y 2 en uso
- **THEN** el resumen dice que se movieron 10 archivos (1 MB) a la Papelera, que el espacio se libera al vaciarla y que 2 en uso o modificados hace poco se omitieron, sin rutas ni códigos

#### Scenario: Escaneo ya limpiado al volver
- **WHEN** el usuario vuelve a Limpieza tras limpiar el último escaneo
- **THEN** ve un aviso de que ese análisis ya se limpió, con la fecha, "Limpiar" no está disponible y "Buscar basura" sí

### Requirement: Detalle de la limpieza en modo avanzado
En modo avanzado, el diálogo de preview SHALL mostrar además los excluidos por escritura reciente por categoría, y el resumen SHALL mostrar los totales por categoría y los conteos por resultado y detalle con el código estable junto al texto del catálogo.

#### Scenario: Motivos en avanzado
- **WHEN** termina una limpieza con 3 ítems `skipped_protected` por `identity_changed` en modo avanzado
- **THEN** el resumen muestra una fila con el texto del motivo, el código `identity_changed` y la cantidad 3

### Requirement: Tests sin el disco ni la Papelera reales del usuario
Los tests de la limpieza SHALL usar dobles o directorios temporales; los que mueven archivos a la Papelera real MUST crear sus propios archivos en un directorio temporal y MUST estar excluidos de la ejecución por defecto, documentando el motivo.

#### Scenario: Suite por defecto
- **WHEN** se ejecuta la suite de tests por defecto
- **THEN** ningún test mueve archivos a la Papelera real ni toca archivos fuera de directorios temporales

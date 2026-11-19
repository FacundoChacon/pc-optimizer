## Purpose

Define la pantalla Inicio de pc-optimizer en ambos modos: el estado del equipo en tarjetas de lenguaje llano (simple) o en bloques densos con métricas crudas, rutas y detalle (avanzado), con una única acción primaria "Limpiar ahora" que lleva siempre al flujo seguro de preview y Papelera.

## ADDED Requirements

### Requirement: Inicio en modo simple con tres tarjetas
En modo simple, Inicio SHALL mostrar las tarjetas Espacio, Basura y Memoria, en ese orden, cada una con su cifra principal y una frase en lenguaje llano del catálogo, y un enlace secundario "Ver detalle" a Espacio en disco, Limpieza o Monitor respectivamente. MUST NOT mostrar rutas, códigos ni métricas crudas.

#### Scenario: Tarjetas con datos
- **WHEN** el monitor informa 120 GB libres de 476 GB y memoria al 62 % en nivel normal, y el último escaneo de basura terminado encontró 1,2 GB limpiables
- **THEN** Espacio dice que quedan 120 GB libres de 476 GB con "Normal" e icono, Basura dice que se puede liberar hasta 1,2 GB con la fecha del escaneo y Memoria muestra 62 % con su nivel en palabras

#### Scenario: Ver detalle
- **WHEN** el usuario activa "Ver detalle" de la tarjeta Memoria
- **THEN** se muestra la pantalla Monitor

#### Scenario: Sin rutas en simple
- **WHEN** Inicio se muestra en modo simple con un análisis de disco y un escaneo de basura guardados
- **THEN** ningún texto de la pantalla contiene una ruta ni un código de estado

### Requirement: Una sola acción primaria en Inicio
Inicio SHALL tener exactamente una acción primaria, "Limpiar ahora", en ambos modos, acompañada de un texto que explica que primero se muestra qué se moverá a la Papelera y que nada se borra sin confirmación.

#### Scenario: Acción primaria única
- **WHEN** se muestra Inicio en modo simple o en modo avanzado
- **THEN** hay un solo botón primario y su texto es "Limpiar ahora", con la explicación asociada de forma accesible

### Requirement: Limpiar ahora abre el preview de la limpieza sin ejecutar
"Limpiar ahora" MUST NOT mover ni borrar archivos por sí mismo: SHALL llevar a la pantalla Limpieza y abrir el preview de la limpieza existente, que es la única confirmación. Si el último escaneo de basura terminado tiene elegibles, no fue limpiado y terminó hace menos de 60 minutos, el preview se abre directamente.

#### Scenario: Escaneo reciente
- **WHEN** el último escaneo terminó hace 10 minutos, no fue limpiado y tiene elegibles, y el usuario pulsa "Limpiar ahora"
- **THEN** se muestra Limpieza con el diálogo de preview abierto, todas las categorías limpiables marcadas y el foco en "Cancelar", y no se movió ningún archivo

#### Scenario: Cerrar el preview
- **WHEN** el usuario abrió el preview desde Inicio y pulsa Esc
- **THEN** el diálogo se cierra sin confirmar nada y el foco queda en "Limpiar" de la pantalla Limpieza

### Requirement: Limpiar ahora busca basura cuando no hay un escaneo útil
Si no hay un escaneo de basura útil (no existe, fue cancelado, ya se limpió, no tiene elegibles o terminó hace 60 minutos o más), "Limpiar ahora" SHALL llevar a Limpieza, iniciar una búsqueda de basura y, solo si termina con elegibles y el usuario sigue en Limpieza, abrir el preview. Si no hay elegibles, la búsqueda se cancela, falla o el usuario sale de Limpieza, MUST NOT abrir el preview.

#### Scenario: Sin escaneo previo
- **WHEN** no hay escaneos de basura y el usuario pulsa "Limpiar ahora"
- **THEN** se muestra Limpieza con el progreso de la búsqueda y, al terminar con elegibles, se abre el preview con el foco en "Cancelar"

#### Scenario: Nada para limpiar
- **WHEN** la búsqueda iniciada por "Limpiar ahora" termina sin elegibles
- **THEN** no se abre el preview, se ve el resultado de Limpieza y se anuncia que no se encontraron archivos para limpiar

#### Scenario: Escaneo viejo
- **WHEN** el último escaneo con elegibles terminó hace 2 horas y el usuario pulsa "Limpiar ahora"
- **THEN** se inicia una búsqueda nueva antes de abrir el preview

#### Scenario: Salir durante la búsqueda
- **WHEN** el usuario pulsa "Limpiar ahora", navega a Monitor antes de que termine la búsqueda y la búsqueda termina con elegibles
- **THEN** no se abre ningún diálogo y al volver a Limpieza se ven los resultados con "Limpiar" disponible

#### Scenario: Búsqueda cancelada
- **WHEN** el usuario cancela la búsqueda iniciada por "Limpiar ahora"
- **THEN** no se abre el preview

### Requirement: Limpiar ahora deshabilitado con motivo
"Limpiar ahora" SHALL estar deshabilitado, con el motivo en texto visible y asociado de forma accesible, mientras haya en curso una búsqueda de basura, una limpieza, un vaciado de la Papelera, un análisis de disco, una búsqueda o limpieza de duplicados, o mientras la base de datos no esté disponible. El contexto de privilegios `mismatch` MUST NOT deshabilitarlo.

#### Scenario: Otro trabajo en curso
- **WHEN** hay una búsqueda de duplicados en curso
- **THEN** "Limpiar ahora" está deshabilitado y se lee que hay que esperar a que termine

#### Scenario: Base no disponible
- **WHEN** el estado de la base no es `ready`
- **THEN** "Limpiar ahora" está deshabilitado con el motivo de base no disponible

#### Scenario: Over-the-shoulder
- **WHEN** el contexto de privilegios es `mismatch` y no hay trabajos en curso
- **THEN** "Limpiar ahora" está habilitado

### Requirement: Fuentes de las tarjetas
La tarjeta Espacio SHALL usar el espacio libre y total del volumen del sistema de la última muestra del monitor con su nivel "casi lleno"; si el monitor no lo informa, SHALL usar el volumen del último análisis de disco terminado indicando su fecha y sin nivel. Basura SHALL usar el total limpiable del último escaneo de basura terminado, sin la Papelera. Memoria SHALL usar el valor actual y el nivel del monitor.

#### Scenario: Espacio sin monitor
- **WHEN** el monitor no está disponible y el último análisis de disco del 18/11 informa 100 GB libres de 476 GB
- **THEN** la tarjeta Espacio muestra 100 GB libres de 476 GB indicando que es según el análisis del 18/11, sin nivel

#### Scenario: Papelera aparte
- **WHEN** el último escaneo tiene 1 GB limpiable y la Papelera ocupa 2 GB
- **THEN** Basura muestra 1 GB como lo que se puede liberar y menciona aparte que la Papelera ocupa 2 GB y se vacía desde Limpieza

#### Scenario: Escaneo ya limpiado
- **WHEN** el último escaneo de basura ya fue limpiado
- **THEN** Basura dice que ya se limpió lo que encontró la última búsqueda, con su fecha

### Requirement: Estados no disponibles explícitos en Inicio
Cada tarjeta y bloque de Inicio SHALL distinguir con texto del catálogo los estados cargando, midiendo, no disponible (con explicación breve), sin datos recientes y sin análisis; MUST NOT mostrar 0 ni dejar el bloque vacío cuando un dato falta. En modo avanzado el estado no disponible incluye además su código.

#### Scenario: Monitor caído
- **WHEN** el monitor está no disponible y no hay análisis de disco
- **THEN** Espacio y Memoria muestran "No disponible" con su explicación y ninguno muestra 0

#### Scenario: Métrica midiendo
- **WHEN** la memoria todavía no tiene lectura disponible por calentamiento
- **THEN** Memoria muestra "Midiendo…"

#### Scenario: Sin escaneo de basura
- **WHEN** no hay escaneos de basura guardados
- **THEN** Basura dice "Todavía no buscaste basura" y no "No disponible"

#### Scenario: Base no disponible al leer el escaneo
- **WHEN** la lectura del último escaneo de basura falla con `store_unavailable`
- **THEN** Basura muestra "No disponible" con el mensaje del catálogo para base no disponible y, en avanzado, el código `store_unavailable`

#### Scenario: Sin datos recientes
- **WHEN** el monitor no entrega muestras durante 3 intervalos
- **THEN** Espacio y Memoria marcan sus valores como "sin datos recientes"

### Requirement: Inicio en modo avanzado con métricas, rutas y detalle
En modo avanzado Inicio SHALL mostrar, con densidad compacta: los medidores de CPU, memoria, tiempo activo y espacio con actual, promedio de 60 s y pico; un gráfico en vivo de 5 minutos con CPU y memoria; la tabla por categoría del último escaneo de basura con códigos y conteos; el resumen del último análisis de disco con la ruta completa de su raíz; y las tres limpiezas más recientes con enlace a Historial.

#### Scenario: Bloques avanzados
- **WHEN** Inicio se muestra en modo avanzado con monitor activo, un escaneo de basura y un análisis de `C:\`
- **THEN** se ven los cuatro medidores, el gráfico de dos series con leyenda, la tabla de categorías con su código de estado, la raíz `C:\` del análisis con su fecha y las últimas limpiezas

#### Scenario: Gráfico en vivo
- **WHEN** llega una muestra nueva del monitor
- **THEN** el gráfico de Inicio incorpora el punto y su eje cubre los últimos 5 minutos

#### Scenario: Sin limpiezas
- **WHEN** el historial de limpieza está vacío
- **THEN** el bloque de últimas limpiezas dice que todavía no se limpió nada con la app

### Requirement: Mismo Inicio al cambiar de modo
Cambiar de modo desde Configuración y volver a Inicio, o cambiarlo con Inicio en pantalla, SHALL mostrar el mismo Inicio con la densidad y los bloques del modo activo, con la misma acción primaria y sin reiniciar la app.

#### Scenario: Alternancia de modo
- **WHEN** el usuario pasa de modo simple a avanzado y vuelve a Inicio
- **THEN** Inicio muestra los bloques avanzados y la misma acción "Limpiar ahora"; al volver a simple muestra las tres tarjetas

### Requirement: Inicio no vacía la Papelera ni resuelve duplicados
Inicio MUST NOT ofrecer vaciar la Papelera, confirmar una limpieza ni resolver duplicados; esas acciones siguen solo en sus pantallas con sus confirmaciones.

#### Scenario: Papelera con contenido
- **WHEN** la Papelera ocupa 2 GB
- **THEN** Inicio informa el tamaño y remite a Limpieza, sin botón para vaciarla

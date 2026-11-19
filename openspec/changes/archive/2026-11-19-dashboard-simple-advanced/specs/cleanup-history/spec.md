## Purpose

Permite consultar el registro de limpieza (append-only) desde la interfaz: limpiezas pasadas agrupadas y explicadas en lenguaje llano, y su detalle por ítem con rutas completas solo en modo avanzado y nunca en over-the-shoulder, sin escribir ni modificar el registro.

## ADDED Requirements

### Requirement: Limpiezas clasificadas por tipo
Cada limpieza del historial SHALL clasificarse como `junk` (todas sus filas de categorías de la whitelist de basura), `duplicates` (todas sus filas de categoría `duplicates`), `recycle_bin_empty` (fila sin escaneo de categoría `recycle_bin`) u `other` (cualquier otra combinación).

#### Scenario: Limpieza de basura
- **WHEN** una limpieza tiene filas de `temp_user` y `browser_cache_edge`
- **THEN** su tipo es `junk`

#### Scenario: Vaciado de la Papelera
- **WHEN** una limpieza es una única fila sin escaneo, de categoría `recycle_bin` y resultado `recycle_bin_emptied`
- **THEN** su tipo es `recycle_bin_empty`

#### Scenario: Mezcla desconocida
- **WHEN** una limpieza tiene filas de `temp_user` y de una categoría que no es de la whitelist ni `duplicates`
- **THEN** su tipo es `other`

### Requirement: Comando de limpiezas paginadas
El backend SHALL exponer `list_cleanup_runs` con un cursor opcional y un límite de 1 a 50, que devuelve las limpiezas de la más reciente a la más antigua con su clave, tipo, fechas, cantidad de filas, conteos por resultado, bytes movidos, bytes vaciados, detalle del vaciado y categorías, y el cursor siguiente o nulo. La respuesta MUST NOT incluir rutas.

#### Scenario: Primera página
- **WHEN** hay 3 limpiezas y se invoca `list_cleanup_runs` sin cursor con límite 2
- **THEN** responde las 2 más recientes en camelCase y un cursor no nulo

#### Scenario: Última página
- **WHEN** se invoca con el cursor de la página anterior
- **THEN** responde la limpieza restante y cursor nulo

#### Scenario: Límite inválido
- **WHEN** se invoca con límite 0, 51, negativo, fraccionario o mayor que 2^53−1
- **THEN** responde el error `invalid_argument` sin leer el log

### Requirement: Comando de ítems de una limpieza
El backend SHALL exponer `list_cleanup_entries` con la clave de una limpieza, un desplazamiento mayor o igual a 0 y un límite de 1 a 200, que devuelve el total de filas de esa limpieza y la página de filas (identificador, fecha, categoría, ruta, tamaño, resultado y código de detalle) en orden de inserción. La interfaz MUST NOT enviar rutas.

#### Scenario: Página de ítems
- **WHEN** se piden los ítems de una limpieza de 3 filas con desplazamiento 0 y límite 100 en contexto `match`
- **THEN** responde total 3 y las 3 filas con su ruta completa, resultado y código de detalle

#### Scenario: Clave mal formada
- **WHEN** se invoca con una clave de tipo desconocido o un identificador no entero
- **THEN** responde el error `invalid_argument`

### Requirement: Rutas ocultas en over-the-shoulder
Cuando el contexto de privilegios es `mismatch` (incluido el motivo sin determinar), `list_cleanup_entries` MUST devolver todas las rutas como nulas e indicar que están ocultas; conteos, tamaños, categorías, resultados y códigos SHALL mantenerse. La decisión SHALL tomarse en el backend.

#### Scenario: Otra cuenta en la sesión
- **WHEN** el contexto es `mismatch` por usuario distinto y se piden los ítems de una limpieza de 2 filas
- **THEN** responde las 2 filas con ruta nula, sus tamaños y resultados, y la marca de rutas ocultas

#### Scenario: Contexto sin determinar
- **WHEN** el contexto es `mismatch` sin determinar
- **THEN** las rutas también se devuelven nulas

### Requirement: Historial de solo lectura
Consultar el historial MUST NOT escribir, modificar ni borrar entradas del log ni ningún archivo, y SHALL funcionar mientras otro trabajo (escaneo, limpieza, análisis) esté en curso.

#### Scenario: Consulta durante una limpieza
- **WHEN** se invoca `list_cleanup_runs` mientras hay una limpieza en curso
- **THEN** responde normalmente y la cantidad de filas del log no cambia por la consulta

#### Scenario: Base no disponible
- **WHEN** la base no está disponible y se invoca cualquiera de los dos comandos
- **THEN** responde el error de base no disponible existente

### Requirement: Pantalla Historial en modo simple
En modo simple la pantalla Historial SHALL listar las limpiezas de la más reciente a la más antigua, cada una con su fecha y una frase llana según su tipo (movidos a la Papelera con cantidad y tamaño, omitidos por familia, errores, o espacio liberado al vaciar la Papelera), con "Ver más" para paginar. MUST NOT mostrar rutas ni códigos y no tiene acción primaria.

#### Scenario: Limpieza de basura
- **WHEN** hay una limpieza de basura con 340 movidos (1,2 GB) y 5 omitidos
- **THEN** se lee que se movieron 340 archivos (1,2 GB) a la Papelera y que 5 se omitieron, con su fecha, sin rutas

#### Scenario: Vaciado de la Papelera
- **WHEN** hay un vaciado de la Papelera que liberó 2 GB
- **THEN** se lee que la Papelera se vació y se liberaron 2 GB

#### Scenario: Historial vacío
- **WHEN** el log está vacío
- **THEN** la pantalla dice que todavía no se limpió nada con la app

### Requirement: Pantalla Historial en modo avanzado
En modo avanzado cada limpieza SHALL mostrar además sus conteos por resultado con su código y poder desplegarse en una tabla paginada de ítems con fecha y hora, categoría con código, ruta completa, tamaño, resultado con su código y código de detalle; con rutas ocultas SHALL mostrar en su lugar un texto que lo explica.

#### Scenario: Ver archivos
- **WHEN** el usuario despliega una limpieza de 150 ítems en modo avanzado
- **THEN** se ven los primeros 100 ítems con su ruta completa y "Cargar más" agrega los 50 restantes

#### Scenario: Rutas ocultas
- **WHEN** el backend responde los ítems con rutas ocultas
- **THEN** la columna de ruta dice que la ruta está oculta porque la app corre con otra cuenta y hay un aviso sobre la tabla

#### Scenario: Error al leer
- **WHEN** la lectura del historial falla con `store_unavailable`
- **THEN** se muestra el mensaje del catálogo para base no disponible, sin el mensaje técnico

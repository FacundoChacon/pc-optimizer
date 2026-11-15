## MODIFIED Requirements

### Requirement: Migraciones versionadas por número de versión de esquema
La base SHALL registrar su versión de esquema en `PRAGMA user_version` y aplicar en orden, al abrirse, cada migración con número mayor al registrado; cada migración MUST aplicarse junto con la actualización de la versión en una única transacción.

#### Scenario: Base nueva
- **WHEN** se abre una ruta donde no existe la base
- **THEN** se crea el archivo, se aplican todas las migraciones y `user_version` queda en la última versión conocida (hoy `4`)

#### Scenario: Reapertura idempotente
- **WHEN** se abre dos veces seguidas la misma base ya migrada
- **THEN** la segunda apertura no aplica ninguna migración, no duplica datos del seed y `user_version` no cambia

#### Scenario: Migraciones pendientes parciales
- **WHEN** se abre una base con versión `N` y el código conoce migraciones hasta `N+k`
- **THEN** se aplican solo las migraciones `N+1` a `N+k`, en orden

#### Scenario: Base de la versión 1 con datos
- **WHEN** se abre una base en versión `1` con ajustes, escaneos y entradas del log
- **THEN** se aplican solo las migraciones `2`, `3` y `4`, `user_version` queda en `4` y los ajustes, escaneos y entradas del log se conservan intactos

#### Scenario: Base de la versión 2 con hallazgos
- **WHEN** se abre una base en versión `2` con hallazgos de basura y entradas del log
- **THEN** se aplican solo las migraciones `3` y `4`, `user_version` queda en `4` y los hallazgos, resúmenes y entradas del log se conservan intactos

#### Scenario: Base de la versión 3 con duplicados
- **WHEN** se abre una base en versión `3` con grupos de duplicados, hallazgos de basura y entradas del log
- **THEN** se aplica solo la migración `4`, `user_version` queda en `4` y los grupos, copias, hallazgos y entradas del log se conservan intactos

### Requirement: Escaneos interrumpidos cerrados al abrir la base
Al abrir la base, la persistencia SHALL finalizar como `error`, con fin igual al momento de apertura, todo escaneo que haya quedado `running` de una ejecución anterior, y MUST eliminar sus hallazgos, resúmenes, grupos de duplicados, resultados de duplicados, carpetas, archivos grandes, incidencias y resultado de análisis de carpetas si los hubiera.

#### Scenario: Cierre inesperado durante un escaneo
- **WHEN** la app se cerró con un escaneo `running` y se vuelve a abrir
- **THEN** ese escaneo queda `error` con fin informado y ya no impide su retención

#### Scenario: Sin escaneos en curso
- **WHEN** se abre una base sin escaneos `running`
- **THEN** ningún escaneo cambia

#### Scenario: Escaneo de duplicados interrumpido
- **WHEN** la app se cerró con un escaneo de duplicados `running` y se vuelve a abrir
- **THEN** ese escaneo queda `error`, no tiene grupos ni resultado guardados y los grupos del último escaneo de duplicados `done` siguen intactos

#### Scenario: Análisis de carpetas interrumpido
- **WHEN** la app se cerró con un análisis de carpetas `running` y se vuelve a abrir
- **THEN** ese escaneo queda `error`, no tiene carpetas, archivos ni resultado guardados y los datos del último análisis de carpetas `done` siguen intactos

## ADDED Requirements

### Requirement: Esquema del análisis de carpetas
La migración 4 SHALL crear el resultado de un análisis de carpetas (tipo de raíz de lista cerrada, ruta, espacio del volumen opcional en pareja, contadores ≥ 0, uno por escaneo), las carpetas (ruta única por escaneo, padre del mismo árbol salvo la raíz de profundidad 0, tamaños y cantidades ≥ 0 con directo ≤ total, acceso y motivo de opacidad de listas cerradas, marcas 0/1), los archivos grandes (tamaño > 0, carpeta obligatoria) y las incidencias (motivo de lista cerrada), todo borrado en cascada con su escaneo, con índice por ruta y escaneo.

#### Scenario: Raíz con padre
- **WHEN** se intenta guardar una carpeta de profundidad 0 con padre, o una de profundidad 1 sin padre
- **THEN** la base rechaza la escritura

#### Scenario: Tamaño directo mayor que el total
- **WHEN** se intenta guardar una carpeta con 5 MiB directos y 4 MiB en total
- **THEN** la base rechaza la escritura

#### Scenario: Valores fuera de las listas cerradas
- **WHEN** se intenta guardar una carpeta con acceso `partial`, una opacidad `appdata` o una incidencia con motivo `timeout`
- **THEN** la base rechaza la escritura

#### Scenario: Ruta repetida
- **WHEN** se intenta guardar dos veces la misma ruta de carpeta en un escaneo
- **THEN** la base rechaza la escritura

#### Scenario: Cascada con la retención
- **WHEN** la retención elimina un análisis de carpetas
- **THEN** su resultado, sus carpetas, sus archivos grandes y sus incidencias también se eliminan y el log de limpieza queda intacto

### Requirement: Datos solo del último análisis de carpetas completo
Al guardar un análisis de carpetas terminado `done`, la persistencia SHALL guardar de forma atómica su resultado, carpetas, archivos grandes e incidencias, finalizar el escaneo y eliminar en la misma transacción las carpetas, archivos e incidencias de todo análisis de carpetas anterior, conservando su escaneo y su resultado. Un árbol inconsistente MUST rechazarse sin guardar nada.

#### Scenario: Segundo análisis
- **WHEN** existe un análisis `done` con 300 carpetas y termina otro con 120
- **THEN** quedan solo las 120 carpetas nuevas y ambos escaneos conservan su resultado

#### Scenario: Árbol inconsistente
- **WHEN** se intenta guardar un análisis con dos raíces, con una carpeta cuyo padre no está guardado, con un archivo de una carpeta de otro escaneo o con más de 50 incidencias
- **THEN** no queda nada del análisis nuevo, el escaneo sigue `running` y los datos del análisis anterior siguen intactos

#### Scenario: Otros datos intactos
- **WHEN** termina un análisis de carpetas
- **THEN** los hallazgos del último escaneo de basura y los grupos del último escaneo de duplicados no cambian

### Requirement: Carpetas y archivos grandes paginados
La persistencia SHALL devolver los hijos de una carpeta del análisis ordenados por tamaño descendente o por nombre (desempate por identificador), con el total, y los archivos grandes ordenados por tamaño descendente, por fecha descendente o ascendente (sin fecha al final, desempate por identificador), con el total, en páginas de a lo sumo 200 elementos.

#### Scenario: Hijos por tamaño
- **WHEN** se piden los hijos de una carpeta con hijos de 10, 30 y 20 MiB
- **THEN** se devuelven en el orden 30, 20, 10 MiB con total 3

#### Scenario: Archivos por fecha
- **WHEN** se piden los archivos grandes por más viejos y uno no tiene fecha
- **THEN** se devuelven del más viejo al más nuevo y el que no tiene fecha queda al final

#### Scenario: Página fuera de rango
- **WHEN** se pide la página que empieza en el elemento 500 de una carpeta con 30 hijos
- **THEN** se devuelve una lista vacía con total 30

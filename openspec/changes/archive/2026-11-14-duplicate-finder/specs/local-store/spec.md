## MODIFIED Requirements

### Requirement: Migraciones versionadas por número de versión de esquema
La base SHALL registrar su versión de esquema en `PRAGMA user_version` y aplicar en orden, al abrirse, cada migración con número mayor al registrado; cada migración MUST aplicarse junto con la actualización de la versión en una única transacción.

#### Scenario: Base nueva
- **WHEN** se abre una ruta donde no existe la base
- **THEN** se crea el archivo, se aplican todas las migraciones y `user_version` queda en la última versión conocida (hoy `3`)

#### Scenario: Reapertura idempotente
- **WHEN** se abre dos veces seguidas la misma base ya migrada
- **THEN** la segunda apertura no aplica ninguna migración, no duplica datos del seed y `user_version` no cambia

#### Scenario: Migraciones pendientes parciales
- **WHEN** se abre una base con versión `N` y el código conoce migraciones hasta `N+k`
- **THEN** se aplican solo las migraciones `N+1` a `N+k`, en orden

#### Scenario: Base de la versión 1 con datos
- **WHEN** se abre una base en versión `1` con ajustes, escaneos y entradas del log
- **THEN** se aplican solo las migraciones `2` y `3`, `user_version` queda en `3` y los ajustes, escaneos y entradas del log se conservan intactos

#### Scenario: Base de la versión 2 con hallazgos
- **WHEN** se abre una base en versión `2` con hallazgos de basura y entradas del log
- **THEN** se aplica solo la migración `3`, `user_version` queda en `3` y los hallazgos, resúmenes y entradas del log se conservan intactos

### Requirement: Escaneos interrumpidos cerrados al abrir la base
Al abrir la base, la persistencia SHALL finalizar como `error`, con fin igual al momento de apertura, todo escaneo que haya quedado `running` de una ejecución anterior, y MUST eliminar sus hallazgos, resúmenes, grupos de duplicados y resultados de duplicados si los hubiera.

#### Scenario: Cierre inesperado durante un escaneo
- **WHEN** la app se cerró con un escaneo `running` y se vuelve a abrir
- **THEN** ese escaneo queda `error` con fin informado y ya no impide su retención

#### Scenario: Sin escaneos en curso
- **WHEN** se abre una base sin escaneos `running`
- **THEN** ningún escaneo cambia

#### Scenario: Escaneo de duplicados interrumpido
- **WHEN** la app se cerró con un escaneo de duplicados `running` y se vuelve a abrir
- **THEN** ese escaneo queda `error`, no tiene grupos ni resultado guardados y los grupos del último escaneo de duplicados `done` siguen intactos

### Requirement: Fecha de limpieza de un escaneo derivada del log
La persistencia SHALL informar la fecha de la última entrada de `cleanup_log` asociada a un escaneo y registrada en o después de su fin, o vacío si no hay ninguna, sin agregar tablas ni columnas.

#### Scenario: Escaneo limpiado
- **WHEN** el escaneo 7 terminó en t=1000 y el log tiene entradas con escaneo 7 en t=1500 y t=1800
- **THEN** la fecha de limpieza del escaneo 7 es 1800

#### Scenario: Escaneo sin limpiar
- **WHEN** el log no tiene entradas del escaneo 7 posteriores a su fin
- **THEN** la fecha de limpieza del escaneo 7 es vacía

#### Scenario: Sin migración nueva
- **WHEN** se abre la base tras la migración 3 (que solo agrega el esquema de duplicados)
- **THEN** `cleanup_log` conserva exactamente sus columnas, su `CHECK` de resultados y sus triggers append-only, y la fecha de limpieza se sigue derivando sin tablas ni columnas nuevas

## ADDED Requirements

### Requirement: Esquema de duplicados
La migración 3 SHALL crear el resultado de un escaneo de duplicados (tamaño mínimo y contadores ≥ 0, uno por escaneo), su estado por carpeta de contenido (clave y estado de listas cerradas, motivo solo si no está disponible), los grupos (tamaño > 0, hash blake3 de 64 caracteres hexadecimales, cantidad de copias ≥ 2, fecha de resolución opcional; grupo único por escaneo, tamaño y hash) y sus copias (carpeta, ruta, fecha, identidad de archivo obligatoria y marca de conservada 0/1 que por defecto es 0; ruta única por grupo), todo borrado en cascada con su escaneo y con índices por escaneo y tamaño y por hash.

#### Scenario: Hash inválido
- **WHEN** se intenta guardar un grupo con un hash de 10 caracteres
- **THEN** la base rechaza la escritura

#### Scenario: Copia sin identidad
- **WHEN** se intenta guardar una copia sin volumen o sin identificador de archivo
- **THEN** la base rechaza la escritura

#### Scenario: Carpeta desconocida
- **WHEN** se intenta guardar una copia o un estado de carpeta con la clave `appdata`
- **THEN** la base rechaza la escritura

#### Scenario: Marca de conservada por defecto
- **WHEN** se guarda una copia sin indicar la marca de conservada
- **THEN** queda en 0

#### Scenario: Cascada con la retención
- **WHEN** la retención elimina un escaneo de duplicados
- **THEN** su resultado, sus estados por carpeta, sus grupos y sus copias también se eliminan y el log de limpieza queda intacto

### Requirement: Grupos solo del último escaneo de duplicados completo
Al guardar el resultado de un escaneo de duplicados terminado `done`, la persistencia SHALL guardar de forma atómica su resultado, sus estados por carpeta, sus grupos y copias, finalizar el escaneo y eliminar en la misma transacción los grupos y copias de todo escaneo de duplicados anterior, conservando su escaneo y su resultado. Un grupo con menos de 2 copias, o dos copias con la misma identidad en un grupo, MUST rechazarse sin guardar nada.

#### Scenario: Segundo escaneo de duplicados
- **WHEN** existe un escaneo de duplicados `done` con 10 grupos y termina otro con 4
- **THEN** quedan solo los 4 grupos nuevos y ambos escaneos conservan su resultado

#### Scenario: Grupo inválido
- **WHEN** se intenta guardar un escaneo con un grupo de una sola copia
- **THEN** no queda nada del escaneo nuevo, el escaneo sigue `running` y los grupos anteriores siguen intactos

#### Scenario: Hallazgos de basura intactos
- **WHEN** termina un escaneo de duplicados
- **THEN** los hallazgos del último escaneo de basura no cambian

### Requirement: Resolución atómica de grupos
La persistencia SHALL resolver un conjunto de grupos de un escaneo en una sola transacción, fijando su fecha de resolución y la marca de conservada solo en las copias indicadas; MUST rechazar sin cambios la resolución de un grupo ya resuelto, de un grupo de otro escaneo, sin copias conservadas o con copias que no son del grupo. La marca de conservada MUST NOT escribirse por ninguna otra operación.

#### Scenario: Resolución válida
- **WHEN** se resuelven los grupos 3 y 5 conservando una copia en cada uno
- **THEN** ambos tienen fecha de resolución y solo esas dos copias tienen la marca de conservada

#### Scenario: Resolución sin copia conservada
- **WHEN** se intenta resolver el grupo 3 sin copias conservadas
- **THEN** la operación falla y ningún grupo cambia

#### Scenario: Doble resolución
- **WHEN** se intenta resolver otra vez el grupo 3 ya resuelto
- **THEN** la operación falla y el grupo conserva su fecha y sus marcas

### Requirement: Grupos paginados y filtrados
La persistencia SHALL devolver los grupos de un escaneo de duplicados con sus copias, ordenados por bytes recuperables (tamaño por copias menos una) descendentes y luego por identificador, con el total, filtrables por carpeta de contenido (grupos con al menos una copia en ella) y por tamaño mínimo, en páginas de a lo sumo 200 grupos.

#### Scenario: Orden por espacio recuperable
- **WHEN** un escaneo tiene un grupo de 2 copias de 50 MiB y otro de 4 copias de 20 MiB
- **THEN** primero viene el de 20 MiB (60 MiB recuperables) y luego el de 50 MiB (50 MiB)

#### Scenario: Página fuera de rango
- **WHEN** se pide la página que empieza en el grupo 500 de un escaneo con 30 grupos
- **THEN** se devuelve una lista vacía con total 30

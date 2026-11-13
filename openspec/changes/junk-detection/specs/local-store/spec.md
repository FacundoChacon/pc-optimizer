# Spec Delta

## MODIFIED Requirements

### Requirement: Migraciones versionadas por número de versión de esquema
La base SHALL registrar su versión de esquema en `PRAGMA user_version` y aplicar en orden, al abrirse, cada migración con número mayor al registrado; cada migración MUST aplicarse junto con la actualización de la versión en una única transacción.

#### Scenario: Base nueva
- **WHEN** se abre una ruta donde no existe la base
- **THEN** se crea el archivo, se aplican todas las migraciones y `user_version` queda en la última versión conocida (hoy `2`)

#### Scenario: Reapertura idempotente
- **WHEN** se abre dos veces seguidas la misma base ya migrada
- **THEN** la segunda apertura no aplica ninguna migración, no duplica datos del seed y `user_version` no cambia

#### Scenario: Migraciones pendientes parciales
- **WHEN** se abre una base con versión `N` y el código conoce migraciones hasta `N+k`
- **THEN** se aplican solo las migraciones `N+1` a `N+k`, en orden

#### Scenario: Base de la versión 1 con datos
- **WHEN** se abre una base en versión `1` con ajustes, escaneos y entradas del log
- **THEN** se aplica solo la migración `2`, `user_version` queda en `2` y los ajustes, escaneos y entradas del log se conservan intactos

## ADDED Requirements

### Requirement: Esquema de hallazgos de basura
La migración 2 SHALL crear `junk_finding` (escaneo, categoría restringida a la whitelist, ruta, tamaño ≥ 0, fecha opcional, marca de omitido con motivo obligatorio solo si está omitido e identidad de archivo obligatoria solo si es elegible; ruta única por escaneo) y `junk_category_result` (una fila por escaneo y categoría con estado, motivo de bloqueo y contadores ≥ 0), ambas borradas en cascada con su escaneo.

#### Scenario: Categoría inválida
- **WHEN** se intenta guardar un hallazgo o un resumen con categoría `documents`
- **THEN** la base rechaza la escritura

#### Scenario: Omitido sin motivo
- **WHEN** se intenta guardar un hallazgo marcado como omitido sin motivo, o uno elegible con motivo
- **THEN** la base rechaza la escritura

#### Scenario: Elegible sin identidad
- **WHEN** se intenta guardar un hallazgo elegible sin volumen o sin identificador de archivo
- **THEN** la base rechaza la escritura

#### Scenario: Ruta repetida en un escaneo
- **WHEN** se intenta guardar dos hallazgos con la misma ruta en el mismo escaneo
- **THEN** la base rechaza la segunda escritura

#### Scenario: Escaneo inexistente
- **WHEN** se intenta guardar un hallazgo que referencia un escaneo inexistente
- **THEN** la base rechaza la escritura

#### Scenario: Cascada con la retención
- **WHEN** la retención elimina un escaneo de basura
- **THEN** sus hallazgos y su resumen por categoría también se eliminan y el log de limpieza queda intacto

### Requirement: Hallazgos solo del último escaneo de basura completo
Al guardar los resultados de un escaneo de basura terminado `done`, la persistencia SHALL eliminar en la misma transacción los hallazgos de todo escaneo de basura anterior, conservando su escaneo y su resumen por categoría; el guardado MUST ser atómico.

#### Scenario: Segundo escaneo
- **WHEN** existe un escaneo de basura `done` con 10 hallazgos y termina otro con 4
- **THEN** quedan solo los 4 hallazgos nuevos y ambos escaneos conservan su resumen por categoría

#### Scenario: Guardado fallido
- **WHEN** falla la escritura de un hallazgo a mitad del guardado
- **THEN** no queda ningún hallazgo ni resumen del escaneo nuevo y los hallazgos del escaneo anterior siguen intactos

### Requirement: Escaneos interrumpidos cerrados al abrir la base
Al abrir la base, la persistencia SHALL finalizar como `error`, con fin igual al momento de apertura, todo escaneo que haya quedado `running` de una ejecución anterior, y MUST eliminar sus hallazgos y resúmenes si los hubiera.

#### Scenario: Cierre inesperado durante un escaneo
- **WHEN** la app se cerró con un escaneo `running` y se vuelve a abrir
- **THEN** ese escaneo queda `error` con fin informado y ya no impide su retención

#### Scenario: Sin escaneos en curso
- **WHEN** se abre una base sin escaneos `running`
- **THEN** ningún escaneo cambia

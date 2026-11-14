# local-store Specification

## Purpose
Define la persistencia local de pc-optimizer en un único archivo SQLite: cómo se abre y migra, qué esquema y datos iniciales tiene, cómo se protege el log de auditoría de limpieza, qué se retiene y cómo se comporta ante una base corrupta, más nueva o con una migración fallida.

## Requirements

### Requirement: Configuración de cada conexión
Toda conexión a la base SHALL abrirse con claves foráneas activadas, modo de diario WAL, sincronización NORMAL y un tiempo de espera ante bloqueo de al menos 5 segundos.

#### Scenario: Claves foráneas activas
- **WHEN** se abre la base y se consulta `PRAGMA foreign_keys`
- **THEN** el valor es `1`

#### Scenario: Modo WAL persistente
- **WHEN** se abre una base nueva en disco y se consulta su modo de diario
- **THEN** el modo es `wal`

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

### Requirement: Migración fallida sin cambios parciales
Si una migración falla, la apertura MUST fallar con un error de migración que indique la versión que falló, y la base MUST quedar exactamente en la versión y el esquema previos a esa migración.

#### Scenario: Error en mitad de una migración
- **WHEN** una migración ejecuta correctamente su primera sentencia y falla en la segunda
- **THEN** la apertura devuelve error de migración con esa versión, `user_version` conserva el valor previo y no queda ninguna tabla creada por esa migración

### Requirement: Base de una versión más nueva
Si `user_version` es mayor que la última migración conocida, la apertura MUST fallar con un error que informe la versión encontrada y la soportada, sin escribir en la base.

#### Scenario: Base creada por una versión futura
- **WHEN** se abre una base con `user_version = 99` y la última migración conocida es `1`
- **THEN** la apertura devuelve el error de esquema más nuevo con `found = 99` y `supported = 1`, y el archivo no se modifica

### Requirement: Esquema inicial
La migración 1 SHALL crear exactamente las tablas `settings` (clave → valor JSON válido), `scan` (inicio y fin en UTC epoch ms, tipo, origen y estado restringidos a sus enumeraciones) y `cleanup_log` (timestamp, categoría, ruta, tamaño ≥ 0, resultado restringido a su enumeración, referencia opcional al escaneo y detalle de error opcional), con índices por inicio de escaneo y por timestamp del log.

#### Scenario: Valor de enumeración inválido
- **WHEN** se intenta guardar un escaneo con tipo `quick` o una entrada de log con resultado `deleted`
- **THEN** la base rechaza la escritura

#### Scenario: Valor de configuración no JSON
- **WHEN** se intenta guardar en `settings` un valor que no es JSON válido
- **THEN** la base rechaza la escritura

#### Scenario: Tamaño negativo
- **WHEN** se intenta registrar en el log una entrada con tamaño `-1`
- **THEN** la base rechaza la escritura

### Requirement: Datos iniciales
Al crear la base SHALL quedar configurado `ui_mode = "simple"`, `autostart_tray = false` y `language = "es"`; las reaperturas MUST NOT sobrescribir valores que el usuario haya cambiado.

#### Scenario: Seed en base nueva
- **WHEN** se crea la base y se leen las configuraciones
- **THEN** `ui_mode` es `"simple"`, `autostart_tray` es `false` y `language` es `"es"`

#### Scenario: Seed no pisa cambios del usuario
- **WHEN** el usuario cambia `ui_mode` a `"advanced"` y la base se vuelve a abrir
- **THEN** `ui_mode` sigue siendo `"advanced"`

### Requirement: Lectura y escritura de configuración
La persistencia SHALL permitir leer una clave (ausente si no existe), listar todas las claves y escribir una clave con un valor JSON, reemplazando el valor previo si existía.

#### Scenario: Clave inexistente
- **WHEN** se lee la clave `no_existe`
- **THEN** el resultado es ausente y no un error

#### Scenario: Escritura que reemplaza
- **WHEN** se escribe `autostart_tray = true` y luego se lee
- **THEN** el valor leído es `true` y existe una única fila para esa clave

### Requirement: Log de limpieza append-only
Las entradas de `cleanup_log` SHALL poder agregarse y leerse, pero la base MUST rechazar cualquier modificación o borrado de entradas existentes, provenga o no del código de la app; la persistencia MUST NOT ofrecer operaciones de modificación ni de borrado del log.

#### Scenario: Agregar y leer entradas
- **WHEN** se agregan tres entradas en un solo lote y se leen las más recientes
- **THEN** se obtienen las tres, ordenadas de la más nueva a la más vieja, con los mismos valores guardados

#### Scenario: UPDATE directo rechazado
- **WHEN** se ejecuta un `UPDATE` sobre `cleanup_log` directamente contra el archivo de la base
- **THEN** la sentencia falla con un error que menciona que el log es append-only y la fila no cambia

#### Scenario: DELETE directo rechazado
- **WHEN** se ejecuta un `DELETE` sobre `cleanup_log` directamente contra el archivo de la base
- **THEN** la sentencia falla y la cantidad de filas no cambia

#### Scenario: Lote atómico
- **WHEN** un lote de entradas contiene una entrada inválida
- **THEN** no se guarda ninguna entrada del lote

### Requirement: Ciclo de vida de un escaneo
La persistencia SHALL crear un escaneo en estado `running` con su inicio, y permitir finalizarlo una sola vez con estado `done`, `cancelled` o `error` y su fin; finalizar un escaneo inexistente o ya finalizado MUST devolver error sin modificarlo.

#### Scenario: Escaneo finalizado
- **WHEN** se crea un escaneo y se finaliza como `done` con fin `t2`
- **THEN** al leerlo su estado es `done` y su fin es `t2`

#### Scenario: Doble finalización
- **WHEN** se intenta finalizar como `cancelled` un escaneo ya `done`
- **THEN** se devuelve error y el escaneo sigue `done`

### Requirement: Retención de los últimos 50 escaneos
Al crear un escaneo, la persistencia SHALL conservar solo los 50 escaneos más recientes por fecha de inicio, eliminando los más antiguos en la misma transacción, y MUST NOT eliminar escaneos en estado `running`; las entradas del log de limpieza que referencian un escaneo eliminado MUST conservarse intactas.

#### Scenario: Escaneo número 51
- **WHEN** existen 50 escaneos finalizados y se crea uno nuevo
- **THEN** quedan 50 escaneos y el eliminado es el de inicio más antiguo

#### Scenario: Escaneo en curso protegido
- **WHEN** el escaneo más antiguo de 50 está `running` y se crea uno nuevo
- **THEN** el escaneo `running` se conserva y se elimina el finalizado más antiguo

#### Scenario: Log intacto tras la retención
- **WHEN** se elimina por retención un escaneo referenciado por entradas del log
- **THEN** esas entradas siguen existiendo con la misma referencia y los mismos valores

### Requirement: Timestamps en UTC epoch milisegundos
Todo instante persistido SHALL guardarse como entero de milisegundos UTC desde la época Unix y MUST ser mayor que cero.

#### Scenario: Timestamp no positivo
- **WHEN** se intenta crear un escaneo con inicio `0`
- **THEN** la base rechaza la escritura

### Requirement: Base corrupta
Si al abrir el archivo no es una base SQLite válida o su verificación rápida de integridad falla, la apertura MUST fallar con un error de base corrupta sin modificar el archivo; la app MUST NOT recrear la base por su cuenta.

#### Scenario: Archivo que no es una base
- **WHEN** se abre una ruta cuyo archivo contiene bytes arbitrarios
- **THEN** la apertura devuelve error de base corrupta y el contenido del archivo no cambia

### Requirement: Respaldo y recreación
Ante una base corrupta o una migración fallida, la persistencia SHALL ofrecer una operación que renombra la base y sus archivos auxiliares a un respaldo con sufijo `.corrupt-<epoch ms>.bak` en el mismo directorio y crea una base nueva migrada y con datos iniciales; MUST NOT borrar el original y MUST NOT aplicarse a una base de versión más nueva.

#### Scenario: Recreación tras corrupción
- **WHEN** se ejecuta el respaldo y recreación sobre una base corrupta con el reloj en `1700000000000`
- **THEN** existe `data.db.corrupt-1700000000000.bak` con el contenido original y `data.db` es una base nueva con los datos iniciales

#### Scenario: Base más nueva no se recrea
- **WHEN** se pide respaldo y recreación sobre una base con versión de esquema mayor a la soportada
- **THEN** la operación se rechaza y el archivo no se modifica

### Requirement: Arranque sin caída ante fallos de la base
Si la base no puede abrirse al iniciar, la app SHALL arrancar igual y registrar el estado de la persistencia como no disponible con el error tipado correspondiente, sin cerrar el proceso.

#### Scenario: Base corrupta al iniciar
- **WHEN** la app inicia con una base corrupta en su directorio de datos
- **THEN** el arranque continúa sin cerrar el proceso y el estado de persistencia es "no disponible" con error de base corrupta

### Requirement: Errores de persistencia sin detalles internos
Los errores de persistencia SHALL llegar a la capa IPC con un tipo estable (`store_corrupt`, `store_schema_too_new`, `store_migration_failed`, `store_unavailable`) y un mensaje que MUST NOT incluir texto SQL.

#### Scenario: Error mapeado a tipo estable
- **WHEN** un error de base corrupta se convierte al error de la app y se serializa
- **THEN** el resultado tiene `kind = "store_corrupt"` y un mensaje sin sentencias SQL

### Requirement: Acceso concurrente seguro
La persistencia SHALL poder usarse desde varios hilos a la vez sin corromper datos ni abortar el proceso; las operaciones MUST serializarse sobre una única conexión.

#### Scenario: Escrituras concurrentes
- **WHEN** cuatro hilos agregan 25 entradas de log cada uno en paralelo
- **THEN** el log contiene exactamente 100 entradas nuevas y ninguna operación falla

### Requirement: Tests sin la base real del usuario
Los tests de persistencia SHALL ejecutarse sobre archivos en directorios temporales o bases en memoria y MUST NOT abrir el directorio de datos real del usuario.

#### Scenario: Ejecución de la suite
- **WHEN** se ejecuta la suite de tests en una máquina con datos reales de la app
- **THEN** ningún test lee ni escribe en el directorio de datos real

### Requirement: Estado de la persistencia consultable
El backend SHALL exponer por IPC el estado actual de la persistencia: "lista", o "no disponible" con un motivo estable (`corrupt`, `schema_too_new`, `migration_failed`, `unavailable`) y si es recuperable; la respuesta MUST NOT incluir rutas, SQL ni detalles internos del error.

#### Scenario: Base lista
- **WHEN** la base se abrió correctamente y se consulta el estado
- **THEN** la respuesta es `{ "status": "ready" }`

#### Scenario: Base dañada
- **WHEN** la base no pudo abrirse por estar dañada y se consulta el estado
- **THEN** la respuesta es `{ "status": "unavailable", "reason": "corrupt", "recoverable": true }`

#### Scenario: Base de una versión más nueva
- **WHEN** la base tiene una versión de esquema mayor a la soportada y se consulta el estado
- **THEN** la respuesta tiene `reason = "schema_too_new"` y `recoverable = false`

#### Scenario: Directorio de datos no resoluble
- **WHEN** no se pudo resolver el directorio de datos y se consulta el estado
- **THEN** la respuesta tiene `reason = "unavailable"` y `recoverable = false`

### Requirement: Recuperación iniciada por el usuario sin reinicio
Ante una base dañada o una migración fallida, el backend SHALL ofrecer por IPC la operación de respaldo y recreación, que conserva los archivos originales como respaldo `.corrupt-<epoch ms>.bak`, devuelve el nombre del archivo de respaldo principal (sin directorio) y deja la persistencia lista para usarse sin reiniciar la app.

#### Scenario: Recuperación de una base dañada
- **WHEN** el estado es "no disponible" por base dañada y se invoca la recuperación con el reloj en `1700000000000`
- **THEN** la respuesta contiene `data.db.corrupt-1700000000000.bak`, el estado pasa a "lista" y leer la configuración devuelve los valores iniciales

#### Scenario: Recuperación tras migración fallida
- **WHEN** el estado es "no disponible" por migración fallida y se invoca la recuperación
- **THEN** el original queda como respaldo y el estado pasa a "lista"

#### Scenario: Fallo al respaldar
- **WHEN** el archivo de la base no puede renombrarse (p. ej. está en uso por otro proceso)
- **THEN** la operación devuelve error `store_unavailable`, ningún archivo se borra y el estado sigue "no disponible"

### Requirement: Recuperación rechazada fuera de los casos recuperables
La operación de recuperación MUST rechazarse con el error `store_not_recoverable`, sin modificar ningún archivo, cuando la persistencia está lista, cuando la base es de una versión más nueva o cuando el motivo de no disponibilidad no es base dañada ni migración fallida.

#### Scenario: Base lista
- **WHEN** se invoca la recuperación con la persistencia lista
- **THEN** la respuesta es el error `store_not_recoverable` y la base no cambia

#### Scenario: Base de una versión más nueva
- **WHEN** se invoca la recuperación sobre una base con versión de esquema mayor a la soportada
- **THEN** la respuesta es el error `store_not_recoverable` y el archivo no se modifica

#### Scenario: Invocaciones simultáneas
- **WHEN** se invoca la recuperación dos veces a la vez sobre una base dañada
- **THEN** exactamente una invocación respalda y recrea, la otra devuelve `store_not_recoverable` y existe un único respaldo

### Requirement: Recuperación nunca automática
La app MUST NOT ejecutar el respaldo y recreación sin una invocación explícita originada en una acción confirmada por el usuario.

#### Scenario: Arranque con base dañada
- **WHEN** la app arranca con una base dañada y el usuario no confirma la recuperación
- **THEN** el archivo dañado sigue intacto y no existe ningún archivo de respaldo

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

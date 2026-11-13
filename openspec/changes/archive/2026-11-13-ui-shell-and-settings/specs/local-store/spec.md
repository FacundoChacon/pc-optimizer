## ADDED Requirements

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

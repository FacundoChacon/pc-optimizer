# runtime-config Specification

## Purpose
Define cómo pc-optimizer resuelve su configuración de ejecución a partir de variables de entorno, garantizando valores por defecto seguros y que los overrides pensados para desarrollo y tests no estén disponibles en builds de producción.

## Requirements

### Requirement: Override del directorio de datos solo en desarrollo y tests
La app SHALL usar el valor de `PCOPT_DATA_DIR` como directorio de datos únicamente en builds de desarrollo/tests; en builds de producción la variable MUST ser ignorada.

#### Scenario: Override en desarrollo
- **WHEN** un build de desarrollo arranca con `PCOPT_DATA_DIR=C:\tmp\pcopt-dev`
- **THEN** la configuración resuelta usa `C:\tmp\pcopt-dev` como directorio de datos

#### Scenario: Override ignorado en producción
- **WHEN** un build de producción arranca con `PCOPT_DATA_DIR` definida
- **THEN** la configuración resuelta no tiene override y se usa el directorio de datos por defecto

#### Scenario: Valor vacío
- **WHEN** `PCOPT_DATA_DIR` está definida pero vacía o solo con espacios
- **THEN** se trata como no definida

### Requirement: Override relativo rechazado
Un valor de `PCOPT_DATA_DIR` que no sea una ruta absoluta MUST ser ignorado.

#### Scenario: Ruta relativa
- **WHEN** `PCOPT_DATA_DIR=datos\dev` en un build de desarrollo
- **THEN** la configuración resuelta no tiene override

### Requirement: Nivel de log configurable con valor por defecto seguro
La app SHALL leer el nivel de log de `PCOPT_LOG` aceptando `error`, `warn`, `info`, `debug` y `trace` sin distinguir mayúsculas; si la variable falta o tiene un valor inválido SHALL usar `info`.

#### Scenario: Nivel válido
- **WHEN** `PCOPT_LOG=DEBUG`
- **THEN** el nivel de log resuelto es `debug`

#### Scenario: Nivel inválido
- **WHEN** `PCOPT_LOG=verbose`
- **THEN** el nivel de log resuelto es `info`

#### Scenario: Variable ausente
- **WHEN** `PCOPT_LOG` no está definida
- **THEN** el nivel de log resuelto es `info`

### Requirement: Directorio de datos por defecto en el perfil del usuario
Sin override vigente, el directorio de datos SHALL ser la subcarpeta `pc-optimizer` dentro del directorio de datos local de aplicación (`%LOCALAPPDATA%`, no itinerante) del perfil de usuario resuelto para la sesión, independiente del identificador de bundle de la app.

#### Scenario: Sin override
- **WHEN** el perfil resuelto tiene `%LOCALAPPDATA%` en `C:\Users\ana\AppData\Local` y no hay override vigente
- **THEN** el directorio de datos es `C:\Users\ana\AppData\Local\pc-optimizer`

#### Scenario: Override de desarrollo vigente
- **WHEN** un build de desarrollo tiene `PCOPT_DATA_DIR=C:\tmp\pcopt-dev`
- **THEN** el directorio de datos es `C:\tmp\pcopt-dev`, sin agregar la subcarpeta `pc-optimizer`

#### Scenario: Perfil no resoluble
- **WHEN** no hay override vigente y no se puede resolver el `%LOCALAPPDATA%` del perfil
- **THEN** la resolución devuelve error y la persistencia queda no disponible, sin usar un directorio alternativo

### Requirement: Archivo de base de datos
La base local SHALL ubicarse en el archivo `data.db` dentro del directorio de datos resuelto, y el directorio MUST crearse si no existe.

#### Scenario: Primer arranque
- **WHEN** el directorio de datos resuelto no existe
- **THEN** se crea y la base queda en `<directorio de datos>\data.db`

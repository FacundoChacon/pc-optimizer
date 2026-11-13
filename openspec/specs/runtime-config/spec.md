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

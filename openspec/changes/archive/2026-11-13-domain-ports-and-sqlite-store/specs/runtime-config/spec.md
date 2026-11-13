## ADDED Requirements

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

# Spec Delta

## MODIFIED Requirements

### Requirement: Directorio de datos por defecto en el perfil del usuario
Sin override vigente, el directorio de datos SHALL ser la subcarpeta `pc-optimizer` dentro del directorio de datos local de aplicación (`%LOCALAPPDATA%`, no itinerante) del perfil del usuario del token del proceso, independiente del identificador de bundle de la app y del contexto de privilegios. El proceso elevado MUST NOT ubicar su directorio de datos en el perfil de otra cuenta.

#### Scenario: Sin override
- **WHEN** el perfil resuelto tiene `%LOCALAPPDATA%` en `C:\Users\ana\AppData\Local` y no hay override vigente
- **THEN** el directorio de datos es `C:\Users\ana\AppData\Local\pc-optimizer`

#### Scenario: Override de desarrollo vigente
- **WHEN** un build de desarrollo tiene `PCOPT_DATA_DIR=C:\tmp\pcopt-dev`
- **THEN** el directorio de datos es `C:\tmp\pcopt-dev`, sin agregar la subcarpeta `pc-optimizer`

#### Scenario: Perfil no resoluble
- **WHEN** no hay override vigente y no se puede resolver el `%LOCALAPPDATA%` del perfil
- **THEN** la resolución devuelve error y la persistencia queda no disponible, sin usar un directorio alternativo

#### Scenario: Over-the-shoulder
- **WHEN** la sesión interactiva es de `ana`, el proceso corre elevado como `admin` (contexto `mismatch`) y no hay override vigente
- **THEN** el directorio de datos es `C:\Users\admin\AppData\Local\pc-optimizer` y nada se escribe en el perfil de `ana`

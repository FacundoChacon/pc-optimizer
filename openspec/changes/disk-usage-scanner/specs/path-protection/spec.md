## ADDED Requirements

### Requirement: Validación de raíces de análisis de solo lectura
El sistema SHALL validar una raíz de análisis con la misma resolución de rutas que las demás decisiones (forma léxica, ruta real, sin reparse en la ruta ni como objetivo, existe y es carpeta) y aceptar raíces protegidas solo por ser del sistema, de programas, raíz del volumen o anclas; MUST rechazar con su motivo perfiles ajenos, otros volúmenes, entradas de sistema de la raíz y contexto inválido. Aceptar una raíz de análisis no habilita ningún borrado.

#### Scenario: Ubicaciones del sistema y propias aceptadas
- **WHEN** se valida como raíz de análisis `C:\`, `C:\Windows`, `C:\Program Files`, `C:\Users\ana` o `C:\Users\ana\Downloads` con el usuario actual `ana`
- **THEN** la raíz se acepta con su ruta real

#### Scenario: Perfil ajeno y otro volumen rechazados
- **WHEN** se valida como raíz de análisis `C:\Users\bob\Documents` o `D:\Datos` con el sistema en `C:`
- **THEN** se rechaza con motivo `other_user_profile` o `unsupported_volume`

#### Scenario: Entradas privadas de la raíz rechazadas
- **WHEN** se valida como raíz de análisis `C:\$Recycle.Bin` o `C:\System Volume Information`
- **THEN** se rechaza con motivo `volume_system_entry`

#### Scenario: Enlaces y rutas inválidas rechazados
- **WHEN** se valida como raíz de análisis una junction, una carpeta bajo una junction, una ruta de red, un archivo o una carpeta inexistente
- **THEN** se rechaza con el motivo correspondiente y nunca se sigue el enlace

#### Scenario: Contexto restringido
- **WHEN** se valida `C:\Users\ana\Downloads` con el contexto restringido de un `mismatch`
- **THEN** se rechaza con motivo `other_user_profile`

#### Scenario: Aceptación sin efecto en el borrado
- **WHEN** `C:\Windows` se acepta como raíz de análisis y luego se decide un archivo dentro para limpiar
- **THEN** ese archivo sigue siendo `protected` con motivo `windows_dir`

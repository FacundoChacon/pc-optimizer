# Spec Delta

## ADDED Requirements

### Requirement: Candidatos de limpieza de un escaneo
La persistencia SHALL devolver los hallazgos elegibles (no omitidos) de un escaneo para un conjunto de categorías, con su identificador, categoría, ruta real, tamaño, fecha de última escritura e identidad de archivo, ordenados por categoría en el orden de la whitelist y luego por tamaño descendente e identificador; los omitidos MUST NOT incluirse.

#### Scenario: Solo elegibles de las categorías pedidas
- **WHEN** un escaneo tiene 2 elegibles y 1 omitido en `temp_user` y 3 elegibles en `wu_cache`, y se piden candidatos de `temp_user`
- **THEN** se devuelven los 2 elegibles de `temp_user` con su identidad, del más grande al más chico

#### Scenario: Identidad intacta
- **WHEN** se lee un candidato guardado con volumen `0x8000_0000_dead_beef` e id de archivo máximo de 128 bits
- **THEN** el candidato trae exactamente ese volumen y ese id

#### Scenario: Escaneo sin hallazgos
- **WHEN** se piden candidatos de un escaneo inexistente o sin hallazgos
- **THEN** se devuelve una lista vacía sin error

### Requirement: Fecha de limpieza de un escaneo derivada del log
La persistencia SHALL informar la fecha de la última entrada de `cleanup_log` asociada a un escaneo y registrada en o después de su fin, o vacío si no hay ninguna, sin agregar tablas ni columnas.

#### Scenario: Escaneo limpiado
- **WHEN** el escaneo 7 terminó en t=1000 y el log tiene entradas con escaneo 7 en t=1500 y t=1800
- **THEN** la fecha de limpieza del escaneo 7 es 1800

#### Scenario: Escaneo sin limpiar
- **WHEN** el log no tiene entradas del escaneo 7 posteriores a su fin
- **THEN** la fecha de limpieza del escaneo 7 es vacía

#### Scenario: Sin migración nueva
- **WHEN** se abre la base tras este cambio
- **THEN** la versión de esquema sigue siendo 2

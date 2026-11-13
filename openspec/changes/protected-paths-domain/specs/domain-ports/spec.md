## MODIFIED Requirements

### Requirement: Metadata sin seguir puntos de reanálisis
El puerto de sistema de archivos SHALL informar, para una ruta o para cada entrada de un directorio, su tipo, tamaño en bytes, fecha de modificación en UTC epoch ms, si es punto de reanálisis (junction/symlink) y si es contenido de nube, sin seguir el destino de los puntos de reanálisis. La metadata de una ruta MUST incluir además su identidad de archivo (volumen + id) y cantidad de hard links cuando el sistema los provea; las entradas de un listado pueden omitirlos.

#### Scenario: Junction informada como punto de reanálisis
- **WHEN** se lista un directorio que contiene una junction hacia otra carpeta
- **THEN** la entrada de la junction se informa como punto de reanálisis y su contenido de destino no aparece en el listado

#### Scenario: Marcador de posición en la nube
- **WHEN** se consulta la metadata de un archivo de OneDrive que solo existe en la nube
- **THEN** la metadata lo marca como marcador de posición en la nube

#### Scenario: Archivo de nube hidratado
- **WHEN** se consulta la metadata de un archivo de OneDrive descargado localmente que conserva su tag de reanálisis de proveedor de nube
- **THEN** la metadata lo marca como contenido de nube

#### Scenario: Identidad estable de un archivo
- **WHEN** se consulta dos veces la metadata del mismo archivo sin modificarlo
- **THEN** ambas informan la misma identidad de archivo

#### Scenario: Identidad distinta tras recrear
- **WHEN** un archivo se borra y se crea otro con el mismo nombre
- **THEN** la metadata del nuevo informa una identidad distinta

#### Scenario: Hard links informados
- **WHEN** se consulta la metadata de un archivo que tiene dos nombres
- **THEN** la metadata informa 2 hard links

#### Scenario: Metadata de un symlink sin seguirlo
- **WHEN** se consulta la metadata de un symlink hacia un archivo de 1 MB
- **THEN** la metadata describe el symlink como punto de reanálisis, con su propia identidad y no la del destino

### Requirement: Canonicalización explícita
El puerto de sistema de archivos SHALL ofrecer la canonicalización de una ruta como operación propia que puede fallar, devolviendo la ruta absoluta real con nombres largos y sin seguir el último componente si es punto de reanálisis; una ruta que no puede canonicalizarse o cuya ruta real no tiene letra de unidad MUST devolver error y nunca una ruta parcial.

#### Scenario: Ruta con segmentos relativos
- **WHEN** se canonicaliza `C:\Users\ana\Temp\..\Temp\a.tmp` y la ruta existe
- **THEN** el resultado es la ruta absoluta sin segmentos `..`

#### Scenario: Ruta imposible de canonicalizar
- **WHEN** se canonicaliza una ruta inexistente
- **THEN** el puerto devuelve un error

#### Scenario: Nombre corto expandido
- **WHEN** se canonicaliza una ruta que usa el nombre corto 8.3 de una carpeta existente
- **THEN** el resultado usa el nombre largo de esa carpeta

#### Scenario: Symlink como último componente
- **WHEN** se canonicaliza la ruta de un symlink hacia otra carpeta
- **THEN** el resultado es la ruta del symlink y no la de su destino

#### Scenario: Junction intermedia
- **WHEN** se canonicaliza `<tmp>\j\a.txt` donde `<tmp>\j` es una junction hacia `<tmp>\real`
- **THEN** el resultado es `<tmp>\real\a.txt`

### Requirement: Dobles de prueba con el mismo contrato
Cada puerto SHALL tener un doble en memoria para tests que cumpla el mismo contrato observable que su adaptador real; para la persistencia y el sistema de archivos, una misma batería de tests de contrato MUST pasar contra el doble y contra el adaptador real sobre directorios temporales.

#### Scenario: Batería de contrato compartida
- **WHEN** se ejecuta la batería de contrato de persistencia contra el doble en memoria y contra una base SQLite temporal
- **THEN** ambas pasan los mismos casos (seed, settings, escaneos, retención, log)

#### Scenario: Batería de contrato del sistema de archivos
- **WHEN** se ejecuta la batería de contrato de sistema de archivos contra el doble y contra el adaptador real sobre un directorio temporal con archivos, carpetas, una junction y un symlink
- **THEN** ambos pasan los mismos casos (metadata, listado, canonicalización, identidad, hard links, errores)

#### Scenario: Doble de sistema de archivos configurable
- **WHEN** un test configura en el doble una junction, un archivo bloqueado y un marcador en la nube
- **THEN** el doble responde a cada operación como lo haría el sistema real en esos casos, sin tocar el disco

#### Scenario: Dobles excluidos del binario
- **WHEN** se compila la app para producción
- **THEN** los dobles de prueba no forman parte del binario

## ADDED Requirements

### Requirement: Fijación de ancestros de una ruta
El puerto de sistema de archivos SHALL ofrecer fijar todos los ancestros de una ruta (desde la raíz del volumen hasta el padre) de modo que, mientras la fijación viva, ningún proceso pueda renombrarlos ni borrarlos; la fijación MUST fallar si algún ancestro es punto de reanálisis o no puede abrirse, y MUST NOT impedir mover el elemento objetivo.

#### Scenario: Ancestro fijado no se puede renombrar
- **WHEN** se fijan los ancestros de `<tmp>\a\b\f.txt` y otro código intenta renombrar `<tmp>\a`
- **THEN** el renombre falla mientras la fijación siga vigente

#### Scenario: El objetivo sigue siendo movible
- **WHEN** los ancestros de `<tmp>\a\b\f.txt` están fijados
- **THEN** `f.txt` puede renombrarse o moverse

#### Scenario: Liberación
- **WHEN** la fijación se libera
- **THEN** los ancestros vuelven a poder renombrarse

#### Scenario: Ancestro que es junction
- **WHEN** se intenta fijar los ancestros de `<tmp>\j\f.txt` y `<tmp>\j` es una junction
- **THEN** la fijación falla y no queda ningún ancestro fijado

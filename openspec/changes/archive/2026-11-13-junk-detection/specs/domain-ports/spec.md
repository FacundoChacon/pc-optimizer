# Spec Delta

## MODIFIED Requirements

### Requirement: Persistencia segregada por agregado
El acceso a la persistencia SHALL dividirse en contratos independientes para configuración, escaneos, log de limpieza y hallazgos de basura, de modo que cada consumidor dependa solo del que usa y los dobles de prueba no deban implementar operaciones ajenas.

#### Scenario: Consumidor de solo configuración
- **WHEN** un componente necesita leer y escribir configuración
- **THEN** puede recibir solo el contrato de configuración, sin depender de escaneos ni del log

#### Scenario: Consumidor de hallazgos
- **WHEN** la detección de basura guarda los resultados de un escaneo
- **THEN** depende solo de los contratos de escaneos y de hallazgos, sin depender de configuración ni del log de limpieza

#### Scenario: Hallazgos sin operaciones de borrado de archivos
- **WHEN** se revisan las operaciones del contrato de hallazgos
- **THEN** solo guardan y consultan registros de la base; ninguna actúa sobre archivos del disco

### Requirement: Dobles de prueba con el mismo contrato
Cada puerto SHALL tener un doble en memoria para tests que cumpla el mismo contrato observable que su adaptador real; para la persistencia y el sistema de archivos, una misma batería de tests de contrato MUST pasar contra el doble y contra el adaptador real sobre directorios temporales.

#### Scenario: Batería de contrato compartida
- **WHEN** se ejecuta la batería de contrato de persistencia contra el doble en memoria y contra una base SQLite temporal
- **THEN** ambas pasan los mismos casos (seed, settings, escaneos, retención, log, hallazgos, resumen por categoría, retención de hallazgos y escaneos interrumpidos)

#### Scenario: Batería de contrato del sistema de archivos
- **WHEN** se ejecuta la batería de contrato de sistema de archivos contra el doble y contra el adaptador real sobre un directorio temporal con archivos, carpetas, una junction y un symlink
- **THEN** ambos pasan los mismos casos (metadata, listado, canonicalización, identidad, hard links, errores)

#### Scenario: Doble de sistema de archivos configurable
- **WHEN** un test configura en el doble una junction, un archivo bloqueado y un marcador en la nube
- **THEN** el doble responde a cada operación como lo haría el sistema real en esos casos, sin tocar el disco

#### Scenario: Doble de la Papelera configurable
- **WHEN** un test configura el doble de consulta de la Papelera con un tamaño, una cantidad o un error
- **THEN** la consulta devuelve exactamente eso sin tocar la Papelera real

#### Scenario: Dobles excluidos del binario
- **WHEN** se compila la app para producción
- **THEN** los dobles de prueba no forman parte del binario

## ADDED Requirements

### Requirement: Consulta de la Papelera de solo lectura
El dominio SHALL declarar un contrato que, para la raíz de un volumen, devuelva la cantidad de elementos y el tamaño total de la Papelera del usuario del proceso; el contrato MUST NOT ofrecer operaciones que listen, restauren, borren o vacíen elementos.

#### Scenario: Consulta del volumen del sistema
- **WHEN** se consulta la Papelera de `C:\`
- **THEN** se obtienen la cantidad de elementos y los bytes totales, ambos mayores o iguales a cero

#### Scenario: Consulta fallida
- **WHEN** la consulta al sistema falla
- **THEN** el contrato devuelve un error tipado y no modifica la Papelera

#### Scenario: Superficie del contrato auditada
- **WHEN** se revisan las operaciones del contrato de consulta de la Papelera
- **THEN** su única operación es la consulta de cantidad y tamaño

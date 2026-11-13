# domain-ports Specification

## Purpose
Define los contratos (puertos) entre la lógica de dominio pura de pc-optimizer y el sistema operativo o la persistencia, de modo que toda regla de negocio pueda probarse con dobles en memoria sin tocar el disco, la Papelera, los sensores ni la base del usuario.

## Requirements

### Requirement: Puertos definidos en el dominio sin I/O
El dominio SHALL declarar como contratos abstractos el acceso al sistema de archivos, a la Papelera, a los sensores de rendimiento, al reloj y a la persistencia; las definiciones MUST NOT depender de APIs de archivos, procesos, red, Windows, SQLite ni Tauri.

#### Scenario: Contratos declarados en la capa de dominio
- **WHEN** la suite de arquitectura recorre los archivos de la capa de dominio, incluidos los contratos y sus dobles de prueba
- **THEN** no encuentra imports de `std::fs`, `std::process`, `std::net`, `windows`, `rusqlite` ni `tauri`

#### Scenario: Implementaciones reales fuera del dominio
- **WHEN** se busca la implementación real de un contrato de puerto
- **THEN** está en la capa de adaptadores y no en la de dominio

### Requirement: Operaciones de puerto síncronas y con errores tipados
Cada operación falible de un puerto SHALL ser síncrona y devolver un resultado con un error tipado que distinga al menos: no encontrado, acceso denegado, recurso en uso, operación no soportada y error de E/S genérico; MUST NOT abortar el proceso ante un fallo del sistema.

#### Scenario: Recurso inexistente
- **WHEN** se consulta la metadata de una ruta que no existe
- **THEN** el puerto devuelve el error "no encontrado" con la ruta en el detalle

#### Scenario: Archivo bloqueado al moverlo a la Papelera
- **WHEN** se intenta mover a la Papelera un archivo abierto en exclusiva por otro proceso
- **THEN** el puerto devuelve el error "recurso en uso" y el archivo no se modifica

### Requirement: Metadata sin seguir puntos de reanálisis
El puerto de sistema de archivos SHALL informar, para una ruta o para cada entrada de un directorio, su tipo, tamaño en bytes, fecha de modificación en UTC epoch ms y si es punto de reanálisis (junction/symlink) o marcador de posición en la nube, sin seguir el destino de los puntos de reanálisis.

#### Scenario: Junction informada como punto de reanálisis
- **WHEN** se lista un directorio que contiene una junction hacia otra carpeta
- **THEN** la entrada de la junction se informa como punto de reanálisis y su contenido de destino no aparece en el listado

#### Scenario: Marcador de posición en la nube
- **WHEN** se consulta la metadata de un archivo de OneDrive que solo existe en la nube
- **THEN** la metadata lo marca como marcador de posición en la nube

### Requirement: Canonicalización explícita
El puerto de sistema de archivos SHALL ofrecer la canonicalización de una ruta como operación propia que puede fallar; una ruta que no puede canonicalizarse MUST devolver error y nunca una ruta parcial.

#### Scenario: Ruta con segmentos relativos
- **WHEN** se canonicaliza `C:\Users\ana\Temp\..\Temp\a.tmp` y la ruta existe
- **THEN** el resultado es la ruta absoluta sin segmentos `..`

#### Scenario: Ruta imposible de canonicalizar
- **WHEN** se canonicaliza una ruta inexistente
- **THEN** el puerto devuelve un error

### Requirement: Papelera sin borrado permanente
El puerto de Papelera SHALL ofrecer únicamente mover un elemento a la Papelera; MUST NOT exponer ninguna operación de borrado permanente ni de vaciado (el vaciado se incorpora solo con su propio change aprobado).

#### Scenario: Elemento movido a la Papelera
- **WHEN** se mueve a la Papelera un archivo existente y no bloqueado
- **THEN** la operación tiene éxito y el archivo deja de estar en su ruta original

#### Scenario: Volumen sin Papelera
- **WHEN** se intenta mover a la Papelera un archivo de un volumen sin Papelera
- **THEN** el puerto devuelve el error "operación no soportada" y el archivo no se borra

#### Scenario: Superficie del contrato auditada
- **WHEN** se revisan las operaciones del contrato de Papelera
- **THEN** la única operación que modifica archivos es mover a la Papelera

### Requirement: Muestra de sensores
El puerto de sensores SHALL devolver una muestra instantánea con uso de CPU (porcentaje 0–100), memoria usada y total en bytes y actividad de disco (porcentaje 0–100) cuando esté disponible; una métrica no disponible MUST informarse como ausente y no como cero.

#### Scenario: Actividad de disco no disponible
- **WHEN** la fuente de datos no puede medir la actividad de disco
- **THEN** la muestra informa la actividad de disco como ausente y el resto de métricas con su valor

#### Scenario: Muestra completa
- **WHEN** todas las fuentes responden
- **THEN** la muestra contiene CPU, memoria usada, memoria total y actividad de disco dentro de sus rangos

### Requirement: Reloj en UTC epoch milisegundos
El tiempo que usan el dominio y la persistencia SHALL obtenerse de un puerto de reloj que devuelve el instante actual en milisegundos UTC desde la época Unix, de modo que los tests puedan fijarlo.

#### Scenario: Reloj fijo en tests
- **WHEN** un test usa un reloj fijado en `1700000000000`
- **THEN** todo timestamp generado a partir de ese reloj vale `1700000000000`

#### Scenario: Reloj del sistema
- **WHEN** se consulta el reloj real
- **THEN** devuelve un valor positivo en milisegundos UTC, independiente de la zona horaria configurada en Windows

### Requirement: Persistencia segregada por agregado
El acceso a la persistencia SHALL dividirse en contratos independientes para configuración, escaneos y log de limpieza, de modo que cada consumidor dependa solo del que usa y los dobles de prueba no deban implementar operaciones ajenas.

#### Scenario: Consumidor de solo configuración
- **WHEN** un componente necesita leer y escribir configuración
- **THEN** puede recibir solo el contrato de configuración, sin depender de escaneos ni del log

### Requirement: Dobles de prueba con el mismo contrato
Cada puerto SHALL tener un doble en memoria para tests que cumpla el mismo contrato observable que su adaptador real; para la persistencia, una misma batería de tests de contrato MUST pasar contra el doble y contra la base SQLite real.

#### Scenario: Batería de contrato compartida
- **WHEN** se ejecuta la batería de contrato de persistencia contra el doble en memoria y contra una base SQLite temporal
- **THEN** ambas pasan los mismos casos (seed, settings, escaneos, retención, log)

#### Scenario: Doble de sistema de archivos configurable
- **WHEN** un test configura en el doble una junction, un archivo bloqueado y un marcador en la nube
- **THEN** el doble responde a cada operación como lo haría el sistema real en esos casos, sin tocar el disco

#### Scenario: Dobles excluidos del binario
- **WHEN** se compila la app para producción
- **THEN** los dobles de prueba no forman parte del binario

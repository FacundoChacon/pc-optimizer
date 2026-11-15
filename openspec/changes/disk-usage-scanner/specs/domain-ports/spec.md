## ADDED Requirements

### Requirement: Contrato de persistencia del análisis de carpetas
La persistencia SHALL ofrecer al análisis de uso del disco un contrato propio, separado de basura y duplicados, para guardar de forma atómica un análisis terminado, consultar el último resumen y paginar carpetas y archivos grandes; ninguna de estas operaciones MUST actuar sobre archivos del disco.

#### Scenario: Consumidor del análisis
- **WHEN** el análisis de carpetas guarda sus resultados
- **THEN** depende solo de los contratos de escaneos y de carpetas, sin depender de configuración, hallazgos de basura, duplicados ni del log

#### Scenario: Batería de contrato compartida
- **WHEN** se ejecuta la batería de contrato del análisis de carpetas contra el doble en memoria y contra una base SQLite temporal
- **THEN** ambas pasan los mismos casos (guardado atómico, árbol inconsistente rechazado, purga del anterior, paginación y orden, escaneos interrumpidos)

### Requirement: Espacio del volumen de solo lectura
El dominio SHALL consultar el espacio total y libre de un volumen mediante un contrato propio de solo lectura que devuelve un error tipado si la consulta falla, con un doble configurable para tests.

#### Scenario: Consulta real
- **WHEN** se consulta el espacio del volumen de un directorio temporal con el adaptador real
- **THEN** el total es mayor que 0 y el libre no supera al total

#### Scenario: Doble con error
- **WHEN** un test configura el doble para fallar
- **THEN** la consulta devuelve el error configurado sin tocar el sistema

### Requirement: Selector de carpeta en un contrato propio
El dominio SHALL pedir la elección de una carpeta mediante un contrato propio que devuelve la ruta elegida, la cancelación o un error tipado, sin decidir su validez; el doble MUST permitir configurar cada resultado y registrar cuántas veces se abrió.

#### Scenario: Doble cancelado
- **WHEN** un test configura el doble para cancelar
- **THEN** la elección devuelve cancelación y el doble registra una apertura

#### Scenario: Doble con ruta
- **WHEN** un test configura el doble con `C:\Users\ana\Downloads`
- **THEN** la elección devuelve esa ruta y la validación queda a cargo del dominio

### Requirement: Explorador en un contrato propio sin ejecución
El dominio SHALL pedir mostrar un elemento en el Explorador mediante un contrato propio cuya única operación abre la carpeta contenedora con el elemento seleccionado; el contrato MUST NOT ofrecer abrir ni ejecutar elementos, y el doble MUST registrar las rutas pedidas sin abrir ventanas.

#### Scenario: Doble registra la ruta
- **WHEN** el dominio pide mostrar `C:\Users\ana\Videos\viaje.mp4` con el doble
- **THEN** el doble registra esa ruta y no se abre ninguna ventana

#### Scenario: Sin operaciones de apertura
- **WHEN** se revisan las operaciones del contrato del Explorador
- **THEN** solo existe la de mostrar el elemento seleccionado

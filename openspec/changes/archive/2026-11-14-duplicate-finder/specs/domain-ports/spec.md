## ADDED Requirements

### Requirement: Lectura de contenido en un contrato propio
El dominio SHALL declarar un contrato de solo lectura de contenido de archivos, separado del de metadatos y del de Papelera, que abre un archivo sin seguir puntos de reanálisis, sin provocar la recuperación de contenido en la nube y sin impedir que otros procesos lo lean, escriban o borren, e informa por el archivo abierto su identidad, tamaño, última escritura y si es contenido en la nube. El contrato MUST NOT ofrecer operaciones de escritura, renombre ni borrado.

#### Scenario: Lectura de un archivo común
- **WHEN** se abre un archivo de 3 MiB y se leen sus primeros 64 KiB y sus últimos 64 KiB
- **THEN** se obtienen exactamente esos bytes y los metadatos del archivo abierto, con su identidad

#### Scenario: Otro proceso escribe el archivo abierto
- **WHEN** el contenido está abierto para leer y otro proceso lo abre para escribir
- **THEN** el otro proceso puede hacerlo

#### Scenario: Punto de reanálisis
- **WHEN** se abre un symlink o una junction
- **THEN** se abre el enlace en sí, nunca su destino, y sus metadatos lo informan como punto de reanálisis

#### Scenario: Recurso en uso
- **WHEN** otro proceso tiene el archivo abierto sin compartir lectura
- **THEN** la apertura falla con el error "recurso en uso"

### Requirement: Doble de lectura de contenido
El contrato de lectura de contenido SHALL tener un doble en memoria configurable con el contenido de cada ruta, su identidad y marcas de nube, que registra cada apertura y puede simular un cambio de contenido o de metadatos durante la lectura; una misma batería de contrato MUST pasar contra el doble y contra el adaptador real sobre un directorio temporal.

#### Scenario: Cambio simulado durante la lectura
- **WHEN** un test configura que el tamaño de `a.bin` cambie después de la primera lectura
- **THEN** los metadatos leídos por el archivo abierto al terminar informan el tamaño nuevo

#### Scenario: Marcador de nube nunca abierto
- **WHEN** un consumidor recibe una ruta marcada como contenido en la nube en el doble de metadatos
- **THEN** el registro de aperturas del doble de contenido no contiene esa ruta

### Requirement: Contrato de persistencia de duplicados
La persistencia SHALL ofrecer a la búsqueda de duplicados un contrato propio, separado de los hallazgos de basura, para guardar de forma atómica el resultado de un escaneo de duplicados, consultar el último resumen, listar grupos con sus copias y resolver grupos; ninguna de estas operaciones MUST actuar sobre archivos del disco, y la única que escribe la marca de conservada es la resolución.

#### Scenario: Consumidor de duplicados
- **WHEN** la búsqueda de duplicados guarda los resultados de un escaneo
- **THEN** depende solo de los contratos de escaneos y de duplicados, sin depender de configuración, hallazgos de basura ni del log

#### Scenario: Batería de contrato compartida
- **WHEN** se ejecuta la batería de contrato de duplicados contra el doble en memoria y contra una base SQLite temporal
- **THEN** ambas pasan los mismos casos (guardado atómico, retención, paginación y filtros, resolución, escaneos interrumpidos)

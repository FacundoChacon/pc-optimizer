## ADDED Requirements

### Requirement: Lectura agrupada del log de limpieza
La persistencia SHALL leer `cleanup_log` agrupado en limpiezas: las filas con el mismo identificador de escaneo forman una limpieza y cada fila sin identificador de escaneo es una limpieza propia. Cada limpieza informa su clave, primera y última fecha, cantidad de filas, conteo por resultado, bytes movidos, bytes vaciados y categorías presentes, sin rutas. La lectura MUST NOT modificar el log.

#### Scenario: Filas de un mismo escaneo
- **WHEN** el log tiene 3 filas del escaneo 7 (2 `moved_to_trash` de 100 y 200 bytes y 1 `skipped_locked`)
- **THEN** se obtiene una limpieza con clave del escaneo 7, 3 filas, 2 movidas, 300 bytes movidos y 1 omitida en uso

#### Scenario: Vaciados de la Papelera separados
- **WHEN** el log tiene dos filas `recycle_bin_emptied` sin escaneo, de 1 000 y 2 000 bytes
- **THEN** se obtienen dos limpiezas distintas, cada una con la clave de su fila y sus bytes vaciados

#### Scenario: Sin filas
- **WHEN** el log está vacío
- **THEN** la lectura devuelve una lista vacía sin error

### Requirement: Limpiezas ordenadas y paginadas por cursor
Las limpiezas SHALL ordenarse de la más reciente a la más antigua por la fecha de su última fila, con desempate estable por clave, y leerse en páginas de tamaño acotado a partir de un cursor (fecha y clave de la última limpieza vista) sin repetir ni saltear limpiezas existentes.

#### Scenario: Dos páginas
- **WHEN** hay 5 limpiezas y se leen páginas de 2 usando el cursor de cada página
- **THEN** las tres páginas devuelven 2, 2 y 1 limpiezas, sin repetidas, en orden de la más reciente a la más antigua

#### Scenario: Misma fecha
- **WHEN** dos limpiezas terminan en el mismo milisegundo
- **THEN** su orden es siempre el mismo y la paginación no pierde ninguna

### Requirement: Filas de una limpieza paginadas
La persistencia SHALL leer las filas de una limpieza por su clave, en orden de inserción, con desplazamiento y límite, junto con el total de filas de esa limpieza; una clave inexistente MUST devolver una página vacía con total cero.

#### Scenario: Página de filas
- **WHEN** la limpieza del escaneo 7 tiene 250 filas y se pide desplazamiento 200 con límite 100
- **THEN** se obtienen las 50 últimas filas en orden de inserción y total 250

#### Scenario: Clave inexistente
- **WHEN** se piden las filas del escaneo 999, que no tiene filas en el log
- **THEN** se obtiene una página vacía con total 0

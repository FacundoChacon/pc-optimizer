# Spec Delta

## MODIFIED Requirements

### Requirement: Un escaneo de basura a la vez con progreso
El sistema SHALL ejecutar como máximo un escaneo a la vez sin bloquear la interfaz e informar progreso durante el recorrido (categoría actual, archivos vistos y bytes elegibles acumulados); pedir otro escaneo mientras uno está en curso MUST fallar con el error `scan_in_progress` sin afectar al que corre, y pedirlo mientras una limpieza está en curso MUST fallar con `cleanup_in_progress` sin afectar a la limpieza.

#### Scenario: Progreso
- **WHEN** se inicia un escaneo
- **THEN** la interfaz recibe al menos un aviso de inicio con el identificador del escaneo, avisos de progreso por categoría y el resultado final al terminar

#### Scenario: Segundo escaneo
- **WHEN** se pide un escaneo mientras otro está en curso
- **THEN** la petición falla con `scan_in_progress` y el primero termina normalmente

#### Scenario: Escaneo durante una limpieza
- **WHEN** se pide un escaneo mientras una limpieza está en curso
- **THEN** la petición falla con `cleanup_in_progress`, no se crea ningún escaneo y la limpieza termina normalmente

### Requirement: Consulta del último escaneo por IPC
El backend SHALL exponer el resumen del último escaneo de basura terminado `done` (o ninguno), con la fecha de su última limpieza o vacío si nunca se limpió, y una consulta paginada de sus hallazgos por categoría (máximo 500 por página, con total); las respuestas MUST NOT incluir SIDs ni nombres de cuenta, y las rutas solo aparecen en la consulta de hallazgos.

#### Scenario: Sin escaneos
- **WHEN** nunca se escaneó
- **THEN** la consulta del último escaneo devuelve vacío sin error

#### Scenario: Escaneo sin limpiar
- **WHEN** el último escaneo `done` no tiene limpiezas registradas
- **THEN** su resumen informa la fecha de limpieza vacía

#### Scenario: Escaneo limpiado
- **WHEN** sobre el último escaneo `done` se ejecutó una limpieza que registró ítems
- **THEN** su resumen informa como fecha de limpieza la del último ítem registrado y conserva sus totales del escaneo

#### Scenario: Página de hallazgos
- **WHEN** `temp_user` tiene 1200 hallazgos y se pide la página desde 1000 con límite 500
- **THEN** se devuelven 200 hallazgos ordenados por tamaño descendente y el total 1200

#### Scenario: Límite fuera de rango
- **WHEN** se piden hallazgos con límite 0 o 501
- **THEN** la petición falla con un error de argumento inválido

#### Scenario: Escaneo inexistente
- **WHEN** se piden hallazgos de un escaneo que no existe o no es el último escaneo de basura terminado
- **THEN** la petición falla con `scan_not_found`

### Requirement: Pantalla de limpieza en modo simple
La pantalla Limpieza SHALL ofrecer una acción para buscar basura y, tras el escaneo, una tarjeta por categoría con su nombre llano, tamaño legible y estado con icono y texto; la única acción que limpia MUST ser "Limpiar", que abre el preview con confirmación explícita, y MUST NOT existir ninguna acción que mueva o borre sin ese preview.

#### Scenario: Primer uso
- **WHEN** el usuario abre Limpieza sin escaneos previos
- **THEN** ve un estado vacío explicativo y el botón "Buscar basura" como única acción primaria

#### Scenario: Resultado
- **WHEN** termina un escaneo con `temp_user` en 1,2 GB
- **THEN** la tarjeta de archivos temporales muestra "1,2 GB" y el total liberable del escaneo, y "Limpiar" es la acción primaria

#### Scenario: Último escaneo al volver
- **WHEN** el usuario vuelve a la pantalla tras un escaneo terminado
- **THEN** ve el resumen del último escaneo sin volver a escanear

#### Scenario: Nada para limpiar
- **WHEN** el último escaneo no tiene hallazgos elegibles en categorías limpiables
- **THEN** "Limpiar" no está disponible y "Buscar basura" sigue siendo la acción primaria

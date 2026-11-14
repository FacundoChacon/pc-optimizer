## MODIFIED Requirements

### Requirement: Un escaneo de basura a la vez con progreso
El sistema SHALL ejecutar como máximo un escaneo a la vez sin bloquear la interfaz e informar progreso durante el recorrido (categoría actual, archivos vistos y bytes elegibles acumulados); pedir otro escaneo mientras uno está en curso MUST fallar con el error `scan_in_progress` sin afectar al que corre, pedirlo mientras una limpieza está en curso MUST fallar con `cleanup_in_progress` sin afectar a la limpieza, y pedirlo mientras se vacía la Papelera MUST fallar con `recycle_bin_empty_in_progress` sin afectar al vaciado.

#### Scenario: Progreso
- **WHEN** se inicia un escaneo
- **THEN** la interfaz recibe al menos un aviso de inicio con el identificador del escaneo, avisos de progreso por categoría y el resultado final al terminar

#### Scenario: Segundo escaneo
- **WHEN** se pide un escaneo mientras otro está en curso
- **THEN** la petición falla con `scan_in_progress` y el primero termina normalmente

#### Scenario: Escaneo durante una limpieza
- **WHEN** se pide un escaneo mientras una limpieza está en curso
- **THEN** la petición falla con `cleanup_in_progress`, no se crea ningún escaneo y la limpieza termina normalmente

#### Scenario: Escaneo durante el vaciado de la Papelera
- **WHEN** se pide un escaneo mientras la Papelera se está vaciando
- **THEN** la petición falla con `recycle_bin_empty_in_progress`, no se crea ningún escaneo y el vaciado termina normalmente

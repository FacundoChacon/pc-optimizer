## MODIFIED Requirements

### Requirement: Muestra de sensores
El puerto de sensores SHALL devolver una lectura instantánea con uso de CPU (porcentaje 0–100), memoria usada y total en bytes, espacio total y libre del volumen del sistema en bytes, contadores acumulados de bytes leídos y escritos de ese volumen y tiempo activo del disco (porcentaje 0–100); cada métrica MUST poder faltar por separado con un motivo (en calentamiento, falla de la fuente o no soportada) y una métrica no disponible MUST informarse como ausente y no como cero.

#### Scenario: Actividad de disco no disponible
- **WHEN** la fuente de datos no puede medir la actividad de disco
- **THEN** la lectura informa la actividad de disco como ausente con motivo de falla o no soportada y el resto de métricas con su valor

#### Scenario: Muestra completa
- **WHEN** todas las fuentes responden
- **THEN** la lectura contiene CPU, memoria usada, memoria total, espacio del volumen, contadores de disco y actividad de disco dentro de sus rangos

#### Scenario: Primera lectura en calentamiento
- **WHEN** la fuente necesita dos lecturas para calcular la CPU y es la primera
- **THEN** la CPU se informa ausente con motivo "en calentamiento" y nunca como 0

#### Scenario: Doble guionado
- **WHEN** un test guiona dos lecturas con valores conocidos para el doble
- **THEN** el doble las devuelve en orden sin tocar el sistema

## ADDED Requirements

### Requirement: Reloj monotónico para intervalos
El dominio SHALL medir intervalos con un puerto de reloj monotónico en milisegundos que nunca retrocede, separado del reloj de pared, con un doble que avanza solo cuando el test lo pide.

#### Scenario: Reloj monotónico real
- **WHEN** se consulta dos veces el reloj monotónico real
- **THEN** el segundo valor no es menor que el primero

#### Scenario: Doble controlado
- **WHEN** un test avanza el doble 1 500 ms
- **THEN** la siguiente consulta devuelve exactamente 1 500 ms más que la anterior

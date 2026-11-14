# Spec Delta

## MODIFIED Requirements

### Requirement: Papelera sin borrado permanente
El puerto de Papelera SHALL ofrecer únicamente mover a la Papelera un elemento ya re-verificado (el objetivo que entrega la re-verificación anti-TOCTOU, nunca una ruta suelta); MUST NOT exponer ninguna operación de borrado permanente ni de vaciado (el vaciado se incorpora solo con su propio change aprobado), y su error MUST distinguir: en uso, Papelera no disponible, elemento demasiado grande, ruta demasiado larga, acceso denegado, no encontrado, mecanismo inutilizable y otro fallo.

#### Scenario: Elemento movido a la Papelera
- **WHEN** se mueve a la Papelera un archivo existente y no bloqueado
- **THEN** la operación tiene éxito y el archivo deja de estar en su ruta original

#### Scenario: Volumen sin Papelera
- **WHEN** se intenta mover a la Papelera un archivo de un volumen cuya Papelera no está disponible o está deshabilitada
- **THEN** el puerto devuelve el error "Papelera no disponible" y el archivo no se borra

#### Scenario: Superficie del contrato auditada
- **WHEN** se revisan las operaciones del contrato de Papelera
- **THEN** la única operación que modifica archivos es mover a la Papelera

#### Scenario: Solo objetivos re-verificados
- **WHEN** un componente intenta mover a la Papelera una ruta que no pasó por la re-verificación
- **THEN** el contrato no lo permite: la única entrada aceptada es el objetivo re-verificado

#### Scenario: Último componente no seguido
- **WHEN** el objetivo resulta ser una junction o un symlink en el instante de moverlo
- **THEN** se mueve el enlace y su destino queda intacto

#### Scenario: Fallo interno del mecanismo
- **WHEN** el mecanismo de Papelera del sistema no puede inicializarse o falla de forma interna
- **THEN** el puerto devuelve el error "mecanismo inutilizable" y el proceso sigue funcionando

## ADDED Requirements

### Requirement: Doble de la Papelera para mover
El doble de la Papelera SHALL permitir configurar por ruta cada error del puerto y ejecutar una acción justo antes de mover (para simular un reemplazo en la ventana residual), registrar lo movido y quitarlo del doble de sistema de archivos compartido, sin tocar el disco ni la Papelera reales.

#### Scenario: Error configurado
- **WHEN** un test configura "en uso" para `C:\t\a.tmp` y se intenta moverlo
- **THEN** el doble devuelve "en uso", no registra el movimiento y el archivo sigue en el doble de sistema de archivos

#### Scenario: Reemplazo justo antes de mover
- **WHEN** un test configura que antes de mover `C:\t\a.tmp` se reemplace por un enlace
- **THEN** el doble mueve el enlace y el destino del enlace sigue existiendo

### Requirement: Contratos de persistencia de la limpieza
La persistencia SHALL ofrecer a la limpieza, en contratos separados, la lectura de los hallazgos elegibles de un escaneo por categorías y la fecha de limpieza de un escaneo; la limpieza MUST depender solo de los contratos de escaneos, hallazgos y log, y ninguna de estas operaciones actúa sobre archivos del disco.

#### Scenario: Consumidor de la limpieza
- **WHEN** la limpieza arma un preview y registra resultados
- **THEN** depende solo de los contratos de escaneos, hallazgos y log de limpieza, sin depender de configuración

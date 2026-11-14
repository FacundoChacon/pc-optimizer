## ADDED Requirements

### Requirement: Vaciado de la Papelera en un contrato propio
El dominio SHALL declarar un contrato de vaciado separado de los de mover a la Papelera y de medirla, con una única operación: vaciar la Papelera del usuario del proceso en la raíz del volumen del sistema. La raíz MUST ser un valor que solo representa la raíz de una unidad (nunca vacía ni nula, nunca una carpeta), y el error MUST distinguir acceso denegado, mecanismo inutilizable y otro fallo con código estable.

#### Scenario: Raíz obligatoria
- **WHEN** se revisa la firma del contrato de vaciado
- **THEN** solo acepta la raíz de una unidad; no existe forma de pedir "todas las Papeleras" ni una carpeta

#### Scenario: Contratos existentes sin vaciado
- **WHEN** se revisan los contratos de mover a la Papelera y de medirla
- **THEN** ninguno ofrece vaciar, listar, restaurar ni borrar elementos

#### Scenario: Fallo interno del mecanismo de vaciado
- **WHEN** el mecanismo del sistema no puede inicializarse o falla de forma interna
- **THEN** el contrato devuelve el error "mecanismo inutilizable" y el proceso sigue funcionando

### Requirement: Doble del vaciado de la Papelera
El doble del vaciado SHALL registrar cada raíz pedida, devolver un resultado o error configurable y dejar en el doble de medición compartido el estado posterior configurado (vacía, parcial o sin cambios), sin tocar la Papelera real; MUST excluirse del binario de producción como los demás dobles.

#### Scenario: Vaciado parcial simulado
- **WHEN** un test configura el doble para dejar 2 elementos de 40 MB tras vaciar una Papelera de 12 elementos
- **THEN** después de vaciar, la medición del doble devuelve 2 elementos y 40 MB, y el doble registra una sola llamada con la raíz pedida

#### Scenario: Sin llamadas
- **WHEN** un flujo rechaza el vaciado antes de ejecutarlo
- **THEN** el doble no registra ninguna llamada

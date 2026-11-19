## MODIFIED Requirements

### Requirement: Solo a la Papelera, nunca borrado permanente
Todo ítem que se limpie SHALL moverse a la Papelera; el sistema MUST NOT borrarlo de forma permanente por ningún camino, tampoco si la Papelera no está disponible o el ítem no entra en ella, y MUST NOT vaciar la Papelera en este flujo (vaciarla es la única excepción permanente declarada y vive en su propio flujo de doble confirmación).

#### Scenario: Ítem movido
- **WHEN** un ítem pasa todas las verificaciones y la Papelera lo acepta
- **THEN** queda `moved_to_trash`, deja de estar en su ruta original y puede restaurarse desde la Papelera

#### Scenario: Papelera deshabilitada en el volumen
- **WHEN** la Papelera del volumen del sistema está deshabilitada y se confirma una limpieza
- **THEN** el archivo sigue en su lugar, no se destruye nada y la ejecución se aborta con `recycle_bin_unavailable` sin intentar los demás (por la comprobación previa, o por el control por ítem del adaptador si la política cambió a mitad de la ejecución)

#### Scenario: Ítem más grande que la Papelera
- **WHEN** la Papelera rechaza un ítem por tamaño
- **THEN** el ítem queda `error` con detalle `recycle_bin_too_small`, el archivo sigue en su lugar y el resto continúa

#### Scenario: Papelera deshabilitada detectada antes de mover
- **WHEN** la política de la Papelera del volumen del sistema la declara deshabilitada (`NukeOnDelete`, `NoRecycleFiles`) o no puede leerse, y se confirma una limpieza
- **THEN** la ejecución se aborta con `recycle_bin_unavailable` antes de mover el primer ítem, no se mueve ni se registra ningún ítem y todos los archivos siguen en su lugar

#### Scenario: Control por ítem dentro del adaptador
- **WHEN** la política cambia a deshabilitada o ilegible entre el preflight y un ítem
- **THEN** el adaptador de Papelera se niega a moverlo sin invocar al shell, el ítem queda `error` con detalle `recycle_bin_unavailable` y la ejecución se aborta

#### Scenario: Ítem mayor que la capacidad máxima de la Papelera
- **WHEN** el tamaño verificado de un ítem supera la capacidad máxima configurada de la Papelera del volumen
- **THEN** el adaptador se niega a moverlo sin invocar al shell, el ítem queda `error` con detalle `recycle_bin_too_small` y el resto continúa

#### Scenario: Aviso en el preview
- **WHEN** se arma el preview con la Papelera deshabilitada o con política ilegible
- **THEN** el preview informa que la Papelera no está disponible para que la interfaz advierta y no ofrezca confirmar

#### Scenario: Ninguna ruta de código borra de forma permanente
- **WHEN** se revisa el código de producción
- **THEN** no contiene llamadas de borrado permanente de archivos, y la única llamada de vaciado de la Papelera es la del adaptador del vaciado con doble confirmación (capacidad `recycle-bin-empty`), en un solo archivo y fuera de este flujo

### Requirement: Escaneo y limpieza mutuamente excluyentes
Como máximo SHALL correr un escaneo, una limpieza o un vaciado de la Papelera a la vez: iniciar un escaneo durante una limpieza MUST fallar con `cleanup_in_progress`; pedir un preview o confirmar durante un escaneo MUST fallar con `scan_in_progress`; una segunda confirmación durante una limpieza MUST fallar con `cleanup_in_progress`; pedir un preview o confirmar durante un vaciado MUST fallar con `recycle_bin_empty_in_progress`.

#### Scenario: Escaneo durante la limpieza
- **WHEN** se pide un escaneo mientras una limpieza está en curso
- **THEN** la petición falla con `cleanup_in_progress` y la limpieza continúa

#### Scenario: Preview durante un escaneo
- **WHEN** se pide un preview mientras un escaneo está en curso
- **THEN** la petición falla con `scan_in_progress`

#### Scenario: Limpieza durante el vaciado
- **WHEN** se pide un preview o se confirma una limpieza mientras la Papelera se está vaciando
- **THEN** la petición falla con `recycle_bin_empty_in_progress` y no se mueve nada

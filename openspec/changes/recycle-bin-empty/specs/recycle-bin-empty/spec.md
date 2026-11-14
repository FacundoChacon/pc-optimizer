# Spec Delta

## Purpose

Vaciar la Papelera del usuario en el volumen del sistema como la única acción permanente del producto: solo a pedido explícito del usuario, con una doble confirmación que el backend exige paso por paso, bloqueada en mismatch de privilegios, exclusiva con escaneos y limpiezas, y registrada en el log append-only.

## ADDED Requirements

### Requirement: Única acción permanente, solo a pedido del usuario
Vaciar la Papelera SHALL ocurrir únicamente al completar el flujo de doble confirmación iniciado por el usuario desde la interfaz. Ningún escaneo, limpieza, tarea programada, mejora del sistema, arranque de la app ni acción de la bandeja MUST poder vaciarla, y el código de producción MUST contener una sola llamada al mecanismo del sistema que vacía la Papelera.

#### Scenario: Escaneo o limpieza terminados
- **WHEN** termina un escaneo de basura o una limpieza, con o sin elementos en la Papelera
- **THEN** la Papelera conserva todos sus elementos

#### Scenario: Arranque de la app
- **WHEN** la app se cierra con una solicitud de vaciado preparada o reconocida y se vuelve a abrir
- **THEN** no existe ninguna solicitud vigente, vaciar con la anterior falla con `empty_request_not_found` y la Papelera no se modifica

#### Scenario: Código de producción auditado
- **WHEN** la suite de tests revisa el código de producción
- **THEN** la llamada al mecanismo del sistema que vacía la Papelera aparece en un único archivo del adaptador, y el contrato de vaciado solo se usa desde el flujo de los comandos de vaciado

### Requirement: Solo la Papelera del volumen del sistema
El sistema SHALL medir y vaciar únicamente la Papelera del usuario del proceso en la raíz del volumen del sistema, la misma que mide la detección de basura; MUST NOT vaciar las Papeleras de otros volúmenes ni pedir al sistema "todas las Papeleras". Si la unidad del sistema no puede determinarse, la solicitud MUST fallar con `recycle_bin_unavailable`.

#### Scenario: Raíz usada
- **WHEN** el sistema está instalado en `C:` y se vacía la Papelera
- **THEN** la medición y el vaciado se piden para `C:\` y para ninguna otra raíz

#### Scenario: Unidad del sistema desconocida
- **WHEN** no puede determinarse la unidad del sistema
- **THEN** la preparación falla con `recycle_bin_unavailable` y no se vacía nada

### Requirement: Solicitud de vaciado medida y de un solo uso
Preparar el vaciado SHALL medir en ese momento la cantidad de elementos y el tamaño de la Papelera y devolver una solicitud con identificador, cantidad, tamaño, fecha de creación y vencimiento a los 2 minutos, sin modificar nada. Crear otra solicitud MUST invalidar la anterior. Con la Papelera vacía MUST fallar con `recycle_bin_already_empty`; si la medición falla, con `recycle_bin_unavailable`.

#### Scenario: Papelera con contenido
- **WHEN** la Papelera del volumen del sistema tiene 12 elementos y 340 MB y se prepara el vaciado
- **THEN** se devuelve una solicitud con 12 elementos, 340 MB y vencimiento 2 minutos después de su creación, y la Papelera no cambia

#### Scenario: Papelera vacía
- **WHEN** la Papelera tiene 0 elementos y se prepara el vaciado
- **THEN** la preparación falla con `recycle_bin_already_empty` y no se crea ninguna solicitud

#### Scenario: Solicitud reemplazada
- **WHEN** se prepara la solicitud 1, luego la solicitud 2, y se intenta reconocer la 1
- **THEN** la operación falla con `empty_request_not_found`

### Requirement: Doble confirmación exigida por el backend
El backend SHALL exigir dos confirmaciones en orden: reconocer una solicitud preparada (primera) devuelve un token de segundo paso; vaciar (segunda) exige la solicitud reconocida y ese token. Un vaciado sin solicitud, sin reconocer, con otro token o con una solicitud reemplazada o ya usada MUST fallar sin vaciar nada; reconocer dos veces MUST fallar. Todo intento de vaciado que no se rechace por mismatch u otra operación en curso MUST consumir la solicitud.

#### Scenario: Llamada suelta
- **WHEN** se pide vaciar sin haber preparado una solicitud
- **THEN** la operación falla con `empty_request_not_found` y la Papelera no cambia

#### Scenario: Sin la primera confirmación
- **WHEN** se prepara una solicitud y se pide vaciarla sin reconocerla
- **THEN** la operación falla con `empty_not_acknowledged`, la solicitud se descarta y la Papelera no cambia

#### Scenario: Token de segundo paso equivocado
- **WHEN** se reconoce la solicitud y se pide vaciarla con un token distinto del recibido
- **THEN** la operación falla con `empty_request_not_found`, la solicitud se descarta y la Papelera no cambia

#### Scenario: Reconocer dos veces
- **WHEN** se reconoce una solicitud ya reconocida
- **THEN** la operación falla con `empty_request_not_found`

#### Scenario: Ambas confirmaciones en orden
- **WHEN** se prepara, se reconoce y se pide vaciar con el token recibido, sin cambios en la Papelera
- **THEN** la Papelera se vacía

#### Scenario: Reutilizar una solicitud
- **WHEN** terminado un vaciado se vuelve a pedir con la misma solicitud y el mismo token
- **THEN** la operación falla con `empty_request_not_found`

### Requirement: Vencimiento de la solicitud
Una solicitud SHALL vencer 2 minutos después de su creación; reconocerla o vaciarla después MUST fallar con `empty_request_expired` sin vaciar nada.

#### Scenario: Solicitud vencida al vaciar
- **WHEN** se reconoce una solicitud y se pide vaciarla 3 minutos después de prepararla
- **THEN** la operación falla con `empty_request_expired` y la Papelera no cambia

### Requirement: Re-medición antes de vaciar
Inmediatamente antes de vaciar, el sistema SHALL volver a medir la Papelera; si la cantidad de elementos o el tamaño difieren de los de la solicitud, MUST fallar con `recycle_bin_changed` sin vaciar nada, porque la confirmación vale solo para lo que el usuario vio.

#### Scenario: Elemento agregado entre la confirmación y el vaciado
- **WHEN** la solicitud se preparó con 12 elementos y al vaciar la Papelera tiene 13
- **THEN** la operación falla con `recycle_bin_changed` y la Papelera conserva los 13

#### Scenario: Elemento restaurado
- **WHEN** la solicitud se preparó con 12 elementos y al vaciar la Papelera tiene 11
- **THEN** la operación falla con `recycle_bin_changed` y la Papelera conserva los 11

### Requirement: Bloqueo en mismatch de privilegios
Con el contexto de privilegios en `mismatch` (otro usuario u indeterminado), preparar, reconocer y vaciar SHALL fallar con `recycle_bin_blocked` indicando el motivo del mismatch, sin medir ni vaciar la Papelera, porque la Papelera del proceso no es la del usuario de la sesión.

#### Scenario: Over-the-shoulder
- **WHEN** el contexto es `mismatch/different_user` y se prepara el vaciado
- **THEN** la operación falla con `recycle_bin_blocked` y motivo `different_user`, y la Papelera no se mide ni se vacía

#### Scenario: Usuario indeterminado
- **WHEN** el contexto es `mismatch/undetermined` y se pide vaciar
- **THEN** la operación falla con `recycle_bin_blocked` y motivo `undetermined`

### Requirement: Vaciado exclusivo con escaneo y limpieza
Como máximo SHALL correr un escaneo, una limpieza o un vaciado a la vez. Preparar o vaciar durante un escaneo MUST fallar con `scan_in_progress` y durante una limpieza con `cleanup_in_progress`; iniciar un escaneo, pedir un preview o confirmar una limpieza durante un vaciado MUST fallar con `recycle_bin_empty_in_progress`, igual que un segundo vaciado.

#### Scenario: Vaciar durante una limpieza
- **WHEN** se pide vaciar mientras una limpieza mueve archivos a la Papelera
- **THEN** la operación falla con `cleanup_in_progress`, la solicitud se conserva y la limpieza continúa

#### Scenario: Escaneo durante el vaciado
- **WHEN** se pide un escaneo mientras la Papelera se está vaciando
- **THEN** la operación falla con `recycle_bin_empty_in_progress`

### Requirement: Sin vaciado si no puede registrarse
Si la base local no está disponible al pedir el vaciado, la operación SHALL fallar con el error de persistencia correspondiente sin vaciar nada.

#### Scenario: Base no disponible
- **WHEN** la base está en estado no disponible y se pide vaciar con una solicitud reconocida
- **THEN** la operación falla con un error de persistencia y la Papelera no cambia

### Requirement: Resultado decidido por medición
Tras pedir el vaciado, el sistema SHALL volver a medir la Papelera y clasificar el resultado: `emptied` si quedó sin elementos; `partial` si se liberó algo pero quedan elementos; `failed` si no se liberó nada; `unverified` si el sistema informó éxito pero la segunda medición falló. El tamaño liberado MUST ser la diferencia entre la medición previa y la posterior (nunca negativa).

#### Scenario: Vaciado completo
- **WHEN** se vacía una Papelera de 12 elementos y 340 MB y después tiene 0 elementos
- **THEN** el resultado es `emptied` con 340 MB liberados

#### Scenario: Vaciado parcial
- **WHEN** después de vaciar quedan 2 elementos de 40 MB de los 12 de 340 MB
- **THEN** el resultado es `partial` con 300 MB liberados y 2 elementos restantes

#### Scenario: Fallo del sistema sin cambios
- **WHEN** el sistema informa un error y la Papelera conserva sus 12 elementos
- **THEN** el resultado es `failed` con 0 bytes liberados y un código de detalle estable

#### Scenario: Éxito informado pero Papelera ya vacía
- **WHEN** el sistema informa un error pero la segunda medición da 0 elementos
- **THEN** el resultado es `emptied`

### Requirement: Registro del vaciado en el log append-only
Cada vaciado pedido al sistema SHALL agregar una sola entrada a `cleanup_log` con categoría `recycle_bin`, la raíz del volumen como ruta, el tamaño liberado, outcome `recycle_bin_emptied` (resultados `emptied`, `partial` y `unverified`) o `error` (`failed`), un código de detalle estable salvo en `emptied` y sin escaneo asociado. Los rechazos previos MUST NOT registrarse.

#### Scenario: Entrada de un vaciado completo
- **WHEN** termina un vaciado `emptied` de 340 MB en `C:\`
- **THEN** el log tiene una entrada nueva con categoría `recycle_bin`, ruta `C:\`, 340 MB, outcome `recycle_bin_emptied`, sin detalle y sin escaneo

#### Scenario: Entrada de un vaciado parcial
- **WHEN** termina un vaciado `partial`
- **THEN** la entrada tiene outcome `recycle_bin_emptied` y detalle `partial`

#### Scenario: Rechazo sin entrada
- **WHEN** un vaciado falla con `recycle_bin_changed`, `empty_not_acknowledged` o `recycle_bin_blocked`
- **THEN** el log no tiene entradas nuevas

#### Scenario: El escaneo de basura no queda limpiado
- **WHEN** se vacía la Papelera después de un escaneo de basura sin limpiar
- **THEN** ese escaneo sigue sin fecha de limpieza y admite un preview de limpieza

#### Scenario: Registro que falla después de vaciar
- **WHEN** la Papelera se vació pero la entrada del log no pudo escribirse
- **THEN** la respuesta informa el resultado real del vaciado e indica que no quedó registrado

### Requirement: Comandos IPC de vaciado sin rutas
El backend SHALL exponer preparar (sin argumentos), reconocer (identificador de solicitud) y vaciar (identificador y token de segundo paso). Las respuestas MUST informar cantidades, tamaños, fechas, estado, código de detalle y si quedó registrado, y MUST NOT incluir rutas, SIDs ni nombres de cuenta; ningún argumento MUST aceptar rutas ni raíces de volumen.

#### Scenario: Firma de los comandos
- **WHEN** se revisan los argumentos de los comandos de vaciado
- **THEN** solo aceptan el identificador de la solicitud y el token de segundo paso; la raíz la decide el backend

#### Scenario: Respuesta del vaciado
- **WHEN** termina un vaciado
- **THEN** la respuesta incluye estado, elementos y bytes antes y después, bytes liberados, código de detalle y si quedó registrado, sin rutas

### Requirement: Acción "Vaciar Papelera" en la interfaz
La pantalla Limpieza SHALL ofrecer "Vaciar Papelera…" en la tarjeta de la Papelera cuando la última medición tiene elementos, y en el resumen de una limpieza que movió archivos; MUST NOT ofrecerla mientras corre un escaneo, una limpieza o un vaciado. En `mismatch` MUST NOT ofrecerla y MUST explicar que la Papelera pertenece a otra cuenta.

#### Scenario: Tarjeta con contenido
- **WHEN** el último escaneo midió la Papelera con 12 elementos y no hay operaciones en curso
- **THEN** la tarjeta de la Papelera muestra "Vaciar Papelera…"

#### Scenario: Resumen de limpieza
- **WHEN** termina una limpieza que movió 10 archivos a la Papelera
- **THEN** el resumen ofrece "Vaciar Papelera…" junto a la nota de que el espacio se libera al vaciarla

#### Scenario: Mismatch
- **WHEN** el contexto de privilegios está en `mismatch`
- **THEN** no se ofrece vaciar la Papelera y se explica el motivo

### Requirement: Primer paso de confirmación
Al pulsar "Vaciar Papelera…" la interfaz SHALL preparar la solicitud y abrir un diálogo modal con la cantidad de elementos y el tamaño que se van a eliminar para siempre, el aviso de que no se puede deshacer, el foco inicial en "Cancelar" y el botón "Continuar", que reconoce la solicitud. Cancelar, cerrar o pulsar Esc MUST NOT vaciar ni reconocer nada.

#### Scenario: Apertura del primer paso
- **WHEN** el usuario pulsa "Vaciar Papelera…" con 12 elementos y 340 MB en la Papelera
- **THEN** se abre el primer diálogo con "12 elementos" y "340 MB", el aviso de irreversibilidad y el foco en "Cancelar"

#### Scenario: Escape en el primer paso
- **WHEN** el usuario pulsa Esc en el primer diálogo
- **THEN** el diálogo se cierra, el foco vuelve a "Vaciar Papelera…" y no se reconoce ni se vacía nada

### Requirement: Segundo paso de confirmación distinto
Tras "Continuar" la interfaz SHALL mostrar un segundo diálogo distinto del primero, con título propio, la misma cantidad y tamaño, una casilla "Entiendo que no se puede deshacer" sin marcar y un botón destructivo deshabilitado hasta marcarla, con foco inicial en "Cancelar". Solo ese botón MUST pedir el vaciado; cancelar, cerrar o pulsar Esc MUST NOT vaciar nada.

#### Scenario: Botón bloqueado sin la casilla
- **WHEN** se abre el segundo diálogo
- **THEN** el botón "Eliminar para siempre" está deshabilitado hasta marcar la casilla y el foco está en "Cancelar"

#### Scenario: Doble clic en "Continuar"
- **WHEN** el usuario pulsa dos veces seguidas "Continuar" en el primer diálogo
- **THEN** se muestra el segundo diálogo y no se pide ningún vaciado

#### Scenario: Cancelar en el segundo paso
- **WHEN** el usuario pulsa "Cancelar" en el segundo diálogo
- **THEN** no se pide el vaciado y la Papelera conserva todos sus elementos

### Requirement: Ejecución y resultado accesibles
Durante el vaciado la interfaz SHALL anunciar "Vaciando la Papelera" en una región `aria-live` educada sin ofrecer otras operaciones; al terminar SHALL mostrar el resultado en lenguaje llano según el estado (liberado, parcial con elementos restantes, sin cambios, sin verificar, sin registro) y actualizar la tarjeta de la Papelera con la medición posterior. Ante `recycle_bin_changed` MUST ofrecer empezar de nuevo desde el primer paso.

#### Scenario: Vaciado completo
- **WHEN** termina un vaciado `emptied` de 340 MB
- **THEN** la interfaz dice que se liberaron 340 MB y la tarjeta de la Papelera muestra 0 elementos

#### Scenario: Papelera cambiada
- **WHEN** el vaciado falla con `recycle_bin_changed`
- **THEN** la interfaz explica que la Papelera cambió, no vacía nada y ofrece revisar de nuevo desde el primer paso

### Requirement: Tests sin la Papelera real
Los tests del vaciado SHALL usar dobles de medición y de vaciado; el test que vacía la Papelera real MUST quedar excluido de la ejecución por defecto, documentando que vacía la Papelera del usuario, y correrse solo en el smoke guiado con el dueño.

#### Scenario: Suite por defecto
- **WHEN** se ejecuta la suite de tests por defecto
- **THEN** ningún test mide ni vacía la Papelera real

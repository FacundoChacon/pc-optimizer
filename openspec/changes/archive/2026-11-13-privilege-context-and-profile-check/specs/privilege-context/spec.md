# Spec Delta

## Purpose

Determina si la app corre con la cuenta del usuario de la sesión interactiva o con otra cuenta (over-the-shoulder), de forma fail-closed, y ofrece el contrato único con el que las funciones por-usuario y las acciones sobre HKCU se bloquean en mismatch sin deshabilitar el resto.

## ADDED Requirements

### Requirement: Usuarios comparados por identificador de seguridad
El sistema SHALL comparar el usuario del token del proceso con el usuario de la sesión interactiva del propio proceso por su identificador de seguridad (SID), nunca por nombre de cuenta ni por variables de entorno.

#### Scenario: Mismo SID con distinto nombre mostrado
- **WHEN** el token del proceso y la sesión interactiva informan el mismo SID aunque los nombres de cuenta difieran en mayúsculas o formato
- **THEN** los usuarios se consideran el mismo

#### Scenario: Variables de entorno falseadas
- **WHEN** `USERNAME` o `USERPROFILE` indican otra cuenta distinta de la del token
- **THEN** la decisión no cambia, porque no se basa en variables de entorno

### Requirement: Usuario de la sesión interactiva por señales independientes
El usuario de la sesión interactiva SHALL obtenerse de la misma sesión de Windows del proceso mediante dos señales independientes: el dueño del proceso de shell de esa sesión y el usuario de logon de esa sesión. Una señal que no puede obtenerse, o cuyo proceso de shell pertenece a otra sesión, MUST tratarse como ausente.

#### Scenario: Shell de otra sesión
- **WHEN** el proceso de shell encontrado pertenece a una sesión distinta de la del proceso
- **THEN** esa señal se descarta y la decisión usa solo la señal restante

#### Scenario: Sin shell en ejecución
- **WHEN** no hay proceso de shell en la sesión y el usuario de logon de la sesión coincide con el del token
- **THEN** la decisión es `match`

### Requirement: Decisión de coincidencia fail-closed
El sistema SHALL decidir `match` solo si el usuario del token es legible, hay al menos una señal de sesión disponible y todas las señales disponibles coinciden con él. Si las señales coinciden entre sí pero difieren del token MUST decidir `mismatch` con motivo `different_user`; en cualquier otro caso (token ilegible, sin señales, señales contradictorias) MUST decidir `mismatch` con motivo `undetermined`.

#### Scenario: Mismo usuario
- **WHEN** el token del proceso es de `ana` y ambas señales de la sesión indican `ana`
- **THEN** la decisión es `match`

#### Scenario: Over-the-shoulder
- **WHEN** el token del proceso es de `admin` y ambas señales de la sesión indican `ana`
- **THEN** la decisión es `mismatch` con motivo `different_user`

#### Scenario: Sin señales de sesión
- **WHEN** ninguna señal de la sesión interactiva puede obtenerse
- **THEN** la decisión es `mismatch` con motivo `undetermined`

#### Scenario: Token ilegible
- **WHEN** no puede leerse el usuario del token del proceso
- **THEN** la decisión es `mismatch` con motivo `undetermined`, aunque las señales de sesión estén disponibles

#### Scenario: Señales contradictorias
- **WHEN** el dueño del shell es `ana` y el usuario de logon de la sesión es `luis`
- **THEN** la decisión es `mismatch` con motivo `undetermined`

#### Scenario: Sesión sin usuario interactivo
- **WHEN** el proceso corre en una sesión sin usuario interactivo (p. ej. la sesión 0 de servicios)
- **THEN** la decisión es `mismatch` con motivo `undetermined`

### Requirement: Contexto estable durante la ejecución
El contexto de privilegios SHALL calcularse una sola vez al iniciar la app, antes de atender comandos que dependan de él, y MUST permanecer igual durante toda la vida del proceso; la detección MUST NOT impedir el arranque.

#### Scenario: Consultas repetidas
- **WHEN** la interfaz consulta el contexto dos veces durante la misma ejecución
- **THEN** ambas respuestas son idénticas

#### Scenario: Fallo de la detección
- **WHEN** todas las consultas al sistema operativo fallan al iniciar
- **THEN** la app arranca igual con el contexto `mismatch` y motivo `undetermined`

### Requirement: Elevación informada sin efectos de bloqueo
El contexto SHALL informar si el token del proceso está elevado; un token no elevado o cuya elevación no puede leerse MUST informarse como no elevado. La elevación MUST NOT cambiar la decisión de coincidencia ni ampliar lo que puede limpiarse (RN-PRIV-05).

#### Scenario: Build de desarrollo sin elevar
- **WHEN** la app corre sin elevar con la cuenta de la sesión interactiva
- **THEN** el contexto es `match` con `elevated = false`

#### Scenario: Elevación ilegible
- **WHEN** no puede leerse el estado de elevación del token
- **THEN** el contexto informa `elevated = false`

### Requirement: Consulta del contexto de privilegios por IPC
El backend SHALL exponer el contexto por IPC como `{ "status": "match", "elevated": <bool> }` o `{ "status": "mismatch", "reason": "different_user" | "undetermined", "elevated": <bool> }`; la respuesta MUST NOT incluir SIDs, nombres de cuenta ni rutas.

#### Scenario: Respuesta en over-the-shoulder
- **WHEN** la interfaz consulta el contexto y la decisión fue `different_user` con el token elevado
- **THEN** recibe exactamente `{ "status": "mismatch", "reason": "different_user", "elevated": true }`

#### Scenario: Respuesta en coincidencia
- **WHEN** la interfaz consulta el contexto y la decisión fue `match` con el token elevado
- **THEN** recibe exactamente `{ "status": "match", "elevated": true }`

### Requirement: Bloqueo solo del alcance por-usuario
Toda función que actúe sobre el perfil o el registro del usuario actual (`%TEMP%` del usuario, cachés de navegador, Papelera, miniaturas, claves HKCU) SHALL declarar alcance por-usuario; el resto declara alcance de máquina. En `mismatch` el alcance por-usuario MUST quedar bloqueado con el motivo del mismatch y el de máquina MUST seguir permitido; en `match` ambos están permitidos.

#### Scenario: Categoría por-usuario en over-the-shoulder
- **WHEN** el contexto es `mismatch/different_user` y se consulta el acceso para el alcance por-usuario
- **THEN** el acceso es bloqueado con motivo `different_user`

#### Scenario: Categoría de máquina en over-the-shoulder
- **WHEN** el contexto es `mismatch/different_user` y se consulta el acceso para el alcance de máquina
- **THEN** el acceso es permitido

#### Scenario: Contexto indeterminado
- **WHEN** el contexto es `mismatch/undetermined`
- **THEN** el alcance por-usuario está bloqueado con motivo `undetermined` y el de máquina permitido

#### Scenario: Coincidencia
- **WHEN** el contexto es `match`
- **THEN** ambos alcances están permitidos

#### Scenario: Acción sobre HKCU
- **WHEN** una acción modifica claves de HKCU y el contexto es `mismatch`
- **THEN** la acción tiene alcance por-usuario y queda bloqueada

### Requirement: Contexto de protección sin usuario actual en mismatch
Cuando el contexto de privilegios es `mismatch`, el contexto de protección de rutas SHALL construirse sin usuario actual, de modo que toda ruta dentro de la raíz de perfiles resulte protegida; las ubicaciones del sistema MUST conservarse sin cambios. En `match` el contexto de protección MUST quedar intacto.

#### Scenario: Ruta del perfil en over-the-shoulder
- **WHEN** el contexto es `mismatch` y se evalúa `C:\Users\admin\AppData\Local\Temp\a.tmp`, perfil del usuario del token
- **THEN** la decisión es `protected` con motivo `other_user_profile`

#### Scenario: Ruta del sistema en over-the-shoulder
- **WHEN** el contexto es `mismatch` y se evalúa `C:\Windows\Temp\a.tmp`
- **THEN** la decisión es la misma que en `match`

#### Scenario: Coincidencia
- **WHEN** el contexto es `match`
- **THEN** el contexto de protección conserva las carpetas del usuario actual

### Requirement: Tests sin depender de la cuenta real
La decisión de coincidencia, la elevación informada y el bloqueo por alcance SHALL probarse con dobles de prueba de la identidad de la sesión (casos match, mismatch y desconocido); los tests que consulten el token o la sesión reales de la máquina MUST quedar ignorados por defecto con el motivo indicado.

#### Scenario: Suite por defecto
- **WHEN** se ejecuta la suite de tests sin opciones
- **THEN** ningún test consulta el token, la sesión ni el shell reales de la máquina

#### Scenario: Prueba real a mano
- **WHEN** se ejecutan los tests ignorados con `--ignored` desde la sesión del propio usuario
- **THEN** el adaptador real informa `match`

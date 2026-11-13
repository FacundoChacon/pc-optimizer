# Spec Delta

## MODIFIED Requirements

### Requirement: Comandos IPC habilitados explícitamente
Cada comando IPC propio SHALL quedar sujeto al control de acceso del runtime y estar habilitado de forma explícita, por nombre, en las capabilities de la ventana principal; las capabilities MUST NOT otorgar comandos propios que no existan.

#### Scenario: Comando permitido
- **WHEN** la ventana principal invoca `get_app_info`
- **THEN** el runtime permite la invocación y devuelve la respuesta del comando

#### Scenario: Lista exacta de comandos habilitados
- **WHEN** la suite de tests compara los comandos propios declarados en el backend con los permisos de comandos de las capabilities
- **THEN** ambos conjuntos coinciden exactamente (hoy: `get_app_info`, `get_settings`, `set_setting`, `get_store_status`, `recover_store` y `get_privilege_context`)

## ADDED Requirements

### Requirement: Aviso global de contexto de privilegios
Mientras el contexto de privilegios sea `mismatch`, el shell SHALL mostrar en todas las pantallas un aviso persistente con icono y texto del catálogo que explica la situación según el motivo, qué funciones quedan bloqueadas (las que actúan sobre el perfil y los ajustes de Windows del usuario), que el resto sigue disponible y cómo resolverlo. Con `match` MUST NOT mostrarse.

#### Scenario: Over-the-shoulder
- **WHEN** la app arranca y el contexto es `mismatch` con motivo `different_user`
- **THEN** en Inicio y en Configuración se ve el aviso con el texto del catálogo para "otra cuenta", la lista de lo bloqueado y la indicación de abrir la app con la propia cuenta de administrador

#### Scenario: Cuenta no verificable
- **WHEN** el contexto es `mismatch` con motivo `undetermined`
- **THEN** se ve el aviso con el texto del catálogo para "no se pudo verificar la cuenta"

#### Scenario: Coincidencia
- **WHEN** el contexto es `match`
- **THEN** no se muestra ningún aviso de privilegios

#### Scenario: Consulta fallida
- **WHEN** la consulta del contexto de privilegios falla o devuelve una respuesta inválida
- **THEN** se muestra el aviso de "no se pudo verificar la cuenta" (fail-closed)

#### Scenario: Aviso sin cierre
- **WHEN** el aviso de privilegios está visible
- **THEN** no ofrece acción para ocultarlo y sigue visible al navegar entre pantallas y al cambiar de modo

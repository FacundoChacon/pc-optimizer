# Spec Delta

## MODIFIED Requirements

### Requirement: Comandos IPC habilitados explícitamente
Cada comando IPC propio SHALL quedar sujeto al control de acceso del runtime y estar habilitado de forma explícita, por nombre, en las capabilities de la ventana principal; las capabilities MUST NOT otorgar comandos propios que no existan.

#### Scenario: Comando permitido
- **WHEN** la ventana principal invoca `get_app_info`
- **THEN** el runtime permite la invocación y devuelve la respuesta del comando

#### Scenario: Lista exacta de comandos habilitados
- **WHEN** la suite de tests compara los comandos propios declarados en el backend con los permisos de comandos de las capabilities
- **THEN** ambos conjuntos coinciden exactamente (hoy: `get_app_info`, `get_settings`, `set_setting`, `get_store_status`, `recover_store`, `get_privilege_context`, `start_scan`, `cancel_scan`, `get_last_junk_scan`, `list_junk_findings`, `preview_cleanup`, `confirm_cleanup` y `cancel_cleanup`)

#### Scenario: Progreso sin permisos de eventos
- **WHEN** se revisan las capabilities tras agregar el escaneo de basura y la limpieza
- **THEN** no contienen permisos `core:*` (de eventos ni de otro tipo) ni de plugins; el progreso viaja por el canal de la propia invocación

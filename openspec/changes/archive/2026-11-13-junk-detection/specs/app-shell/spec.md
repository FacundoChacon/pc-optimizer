# Spec Delta

## MODIFIED Requirements

### Requirement: Comandos IPC habilitados explícitamente
Cada comando IPC propio SHALL quedar sujeto al control de acceso del runtime y estar habilitado de forma explícita, por nombre, en las capabilities de la ventana principal; las capabilities MUST NOT otorgar comandos propios que no existan.

#### Scenario: Comando permitido
- **WHEN** la ventana principal invoca `get_app_info`
- **THEN** el runtime permite la invocación y devuelve la respuesta del comando

#### Scenario: Lista exacta de comandos habilitados
- **WHEN** la suite de tests compara los comandos propios declarados en el backend con los permisos de comandos de las capabilities
- **THEN** ambos conjuntos coinciden exactamente (hoy: `get_app_info`, `get_settings`, `set_setting`, `get_store_status`, `recover_store`, `get_privilege_context`, `start_scan`, `cancel_scan`, `get_last_junk_scan` y `list_junk_findings`)

#### Scenario: Progreso sin permisos de eventos
- **WHEN** se revisan las capabilities tras agregar el escaneo de basura
- **THEN** no contienen permisos `core:*` (de eventos ni de otro tipo) ni de plugins; el progreso viaja por el canal de la propia invocación

### Requirement: Navegación entre pantallas del shell
La interfaz SHALL ofrecer un shell común con navegación entre las pantallas registradas (hoy: Inicio, Limpieza y Configuración, en ese orden), que muestra una sola pantalla a la vez, arranca en Inicio y marca la pantalla activa de forma accesible.

#### Scenario: Arranque en Inicio
- **WHEN** la app termina de cargar la configuración
- **THEN** se muestra la pantalla Inicio y su entrada de navegación está marcada como actual (`aria-current="page"`)

#### Scenario: Ir a Configuración
- **WHEN** el usuario activa la entrada "Configuración" de la navegación
- **THEN** se muestra la pantalla Configuración con su encabezado principal y la entrada queda marcada como actual

#### Scenario: Ir a Limpieza
- **WHEN** el usuario activa la entrada "Limpieza" de la navegación
- **THEN** se muestra la pantalla Limpieza con su encabezado principal y la entrada queda marcada como actual

#### Scenario: Solo pantallas implementadas
- **WHEN** se renderiza la navegación
- **THEN** solo contiene entradas para pantallas registradas e implementadas, sin entradas vacías ni "próximamente"

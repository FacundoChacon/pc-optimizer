## MODIFIED Requirements

### Requirement: Comandos IPC habilitados explícitamente
Cada comando IPC propio SHALL quedar sujeto al control de acceso del runtime y estar habilitado de forma explícita, por nombre, en las capabilities de la ventana principal; las capabilities MUST NOT otorgar comandos propios que no existan.

#### Scenario: Comando permitido
- **WHEN** la ventana principal invoca `get_app_info`
- **THEN** el runtime permite la invocación y devuelve la respuesta del comando

#### Scenario: Lista exacta de comandos habilitados
- **WHEN** la suite de tests compara los comandos propios declarados en el backend con los permisos de comandos de las capabilities
- **THEN** ambos conjuntos coinciden exactamente (hoy: `get_app_info`, `get_settings`, `set_setting`, `get_store_status`, `recover_store`, `get_privilege_context`, `start_scan`, `cancel_scan`, `get_last_junk_scan`, `list_junk_findings`, `preview_cleanup`, `confirm_cleanup`, `cancel_cleanup`, `prepare_empty_recycle_bin`, `acknowledge_empty_recycle_bin`, `empty_recycle_bin`, `start_duplicate_scan`, `get_last_duplicate_scan`, `list_duplicate_groups`, `preview_duplicate_cleanup`, `confirm_duplicate_cleanup`, `start_folder_scan`, `get_last_folder_scan`, `list_folder_children`, `list_large_files`, `pick_folder_root`, `reveal_in_explorer`, `start_monitor_stream`, `stop_monitor_stream`, `list_cleanup_runs` y `list_cleanup_entries`)

#### Scenario: Progreso sin permisos de eventos
- **WHEN** se revisan las capabilities tras agregar el escaneo de basura, la limpieza, el vaciado de la Papelera, la búsqueda de duplicados, el análisis de uso del disco, el monitor en vivo y el historial de limpieza
- **THEN** no contienen permisos `core:*` (de eventos ni de otro tipo) ni de plugins; el progreso y el flujo de muestras viajan por el canal de la propia invocación

#### Scenario: Selector y Explorador sin plugins
- **WHEN** se revisan las capabilities tras agregar el selector de carpeta y "Mostrar en el Explorador"
- **THEN** no contienen permisos de los plugins `dialog`, `shell`, `opener` ni `fs`; ambas acciones son comandos propios del backend

### Requirement: Navegación entre pantallas del shell
La interfaz SHALL ofrecer un shell común con navegación entre las pantallas registradas (hoy: Inicio, Espacio en disco, Limpieza, Duplicados, Historial, Monitor y Configuración, en ese orden), que muestra una sola pantalla a la vez, arranca en Inicio y marca la pantalla activa de forma accesible.

#### Scenario: Arranque en Inicio
- **WHEN** la app termina de cargar la configuración
- **THEN** se muestra la pantalla Inicio y su entrada de navegación está marcada como actual (`aria-current="page"`)

#### Scenario: Ir a Configuración
- **WHEN** el usuario activa la entrada "Configuración" de la navegación
- **THEN** se muestra la pantalla Configuración con su encabezado principal y la entrada queda marcada como actual

#### Scenario: Ir a Limpieza
- **WHEN** el usuario activa la entrada "Limpieza" de la navegación
- **THEN** se muestra la pantalla Limpieza con su encabezado principal y la entrada queda marcada como actual

#### Scenario: Ir a Duplicados
- **WHEN** el usuario activa la entrada "Duplicados" de la navegación
- **THEN** se muestra la pantalla Duplicados con su encabezado principal y la entrada queda marcada como actual

#### Scenario: Ir a Espacio en disco
- **WHEN** el usuario activa la entrada "Espacio en disco" de la navegación
- **THEN** se muestra la pantalla Espacio en disco con su encabezado principal y la entrada queda marcada como actual

#### Scenario: Ir a Monitor
- **WHEN** el usuario activa la entrada "Monitor" de la navegación
- **THEN** se muestra la pantalla Monitor con su encabezado principal y la entrada queda marcada como actual

#### Scenario: Ir a Historial
- **WHEN** el usuario activa la entrada "Historial" de la navegación
- **THEN** se muestra la pantalla Historial con su encabezado principal y la entrada queda marcada como actual

#### Scenario: Solo pantallas implementadas
- **WHEN** se renderiza la navegación
- **THEN** solo contiene entradas para pantallas registradas e implementadas, sin entradas vacías ni "próximamente"

## ADDED Requirements

### Requirement: Navegación sin mover el foco a pedido de la pantalla destino
La navegación SHALL mover el foco al contenido principal tras cambiar de pantalla, salvo cuando quien navega pide que la pantalla destino maneje el foco (por ejemplo, para abrir un diálogo al llegar); en ese caso MUST NOT mover el foco al contenido principal.

#### Scenario: Navegación por la barra
- **WHEN** el usuario activa una entrada de la navegación
- **THEN** el foco pasa al contenido principal de la pantalla nueva

#### Scenario: Navegación que abre un diálogo
- **WHEN** Inicio navega a Limpieza pidiendo abrir el preview de la limpieza
- **THEN** el foco queda en "Cancelar" del diálogo y no en el contenido principal

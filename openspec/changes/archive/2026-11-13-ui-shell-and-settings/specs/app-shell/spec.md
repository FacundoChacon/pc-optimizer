## MODIFIED Requirements

### Requirement: Textos de interfaz desde el catálogo en español
Todo texto visible de la interfaz SHALL provenir de un catálogo centralizado de strings en español; los componentes MUST NOT contener literales de texto visibles. La regla alcanza también a los nombres y descripciones accesibles (`aria-label`, `title`, `alt`, `placeholder`) y una verificación automática MUST fallar si algún componente los define como literales.

#### Scenario: Título desde el catálogo
- **WHEN** se renderiza la pantalla inicial
- **THEN** el título y los mensajes mostrados coinciden con las entradas del catálogo en español

#### Scenario: Literal visible introducido en un componente
- **WHEN** un componente de la interfaz contiene texto JSX literal (p. ej. `<p>Hola</p>`)
- **THEN** la suite de tests falla indicando archivo y línea

#### Scenario: Nombre accesible literal
- **WHEN** un componente define `aria-label="Cerrar"` como literal en lugar de tomarlo del catálogo
- **THEN** la suite de tests falla indicando archivo y atributo

#### Scenario: Textos con datos variables
- **WHEN** un mensaje del catálogo incluye un dato variable (p. ej. el nombre del archivo de respaldo)
- **THEN** el texto mostrado es la plantilla del catálogo con el dato insertado, sin concatenar literales en el componente

### Requirement: Comandos IPC habilitados explícitamente
Cada comando IPC propio SHALL quedar sujeto al control de acceso del runtime y estar habilitado de forma explícita, por nombre, en las capabilities de la ventana principal; las capabilities MUST NOT otorgar comandos propios que no existan.

#### Scenario: Comando permitido
- **WHEN** la ventana principal invoca `get_app_info`
- **THEN** el runtime permite la invocación y devuelve la respuesta del comando

#### Scenario: Lista exacta de comandos habilitados
- **WHEN** la suite de tests compara los comandos propios declarados en el backend con los permisos de comandos de las capabilities
- **THEN** ambos conjuntos coinciden exactamente (hoy: `get_app_info`, `get_settings`, `set_setting`, `get_store_status` y `recover_store`)

## ADDED Requirements

### Requirement: Navegación entre pantallas del shell
La interfaz SHALL ofrecer un shell común con navegación entre las pantallas registradas (en v0.1 de este change: Inicio y Configuración), que muestra una sola pantalla a la vez, arranca en Inicio y marca la pantalla activa de forma accesible.

#### Scenario: Arranque en Inicio
- **WHEN** la app termina de cargar la configuración
- **THEN** se muestra la pantalla Inicio y su entrada de navegación está marcada como actual (`aria-current="page"`)

#### Scenario: Ir a Configuración
- **WHEN** el usuario activa la entrada "Configuración" de la navegación
- **THEN** se muestra la pantalla Configuración con su encabezado principal y la entrada queda marcada como actual

#### Scenario: Solo pantallas implementadas
- **WHEN** se renderiza la navegación
- **THEN** solo contiene entradas para pantallas registradas e implementadas, sin entradas vacías ni "próximamente"

### Requirement: Mensajes de error desde el catálogo por tipo
La interfaz SHALL mostrar cada error del backend con un mensaje del catálogo elegido por su `kind`, con un mensaje genérico del catálogo para tipos desconocidos; el `message` técnico del backend MUST NOT mostrarse al usuario.

#### Scenario: Tipo conocido
- **WHEN** una operación falla con `kind = "store_corrupt"` y un `message` técnico en inglés
- **THEN** la interfaz muestra el mensaje del catálogo para base dañada y no muestra el texto técnico

#### Scenario: Tipo desconocido
- **WHEN** una operación falla con `kind = "algo_nuevo"`
- **THEN** la interfaz muestra el mensaje genérico de error del catálogo

### Requirement: Aviso global de base no disponible
Mientras la persistencia esté no disponible, el shell SHALL mostrar en todas las pantallas un aviso con icono y texto que explica el problema y ofrece ir a Configuración; con la persistencia lista MUST NOT mostrarse.

#### Scenario: Base dañada al iniciar
- **WHEN** la app arranca y el estado de la persistencia es "no disponible" por base dañada
- **THEN** se muestra el aviso con el mensaje del catálogo para base dañada y una acción para ir a Configuración

#### Scenario: Base lista
- **WHEN** el estado de la persistencia es "lista"
- **THEN** no se muestra ningún aviso de base

#### Scenario: Aviso tras recuperar
- **WHEN** el usuario recupera la base con éxito
- **THEN** el aviso desaparece sin reiniciar la app

### Requirement: Navegación completa por teclado con foco visible
Toda acción de la interfaz SHALL poder ejecutarse solo con teclado, todo elemento enfocable MUST mostrar un indicador de foco visible, y el shell SHALL ofrecer como primer elemento enfocable un enlace para saltar al contenido principal.

#### Scenario: Saltar al contenido
- **WHEN** el usuario presiona Tab al cargar la app y activa el primer elemento enfocado
- **THEN** el foco pasa al contenido principal de la pantalla actual

#### Scenario: Cambio de pantalla por teclado
- **WHEN** el usuario enfoca la entrada "Configuración" con Tab y presiona Enter
- **THEN** se muestra la pantalla Configuración

#### Scenario: Indicador de foco
- **WHEN** un botón, enlace o control de formulario recibe el foco por teclado
- **THEN** muestra un contorno de foco con el color de foco del tema

### Requirement: Movimiento reducido
Cuando el sistema indique preferencia de movimiento reducido, la interfaz MUST NOT usar animaciones ni transiciones.

#### Scenario: Preferencia de movimiento reducido activa
- **WHEN** el sistema operativo informa `prefers-reduced-motion: reduce`
- **THEN** las transiciones y animaciones de la interfaz quedan desactivadas

### Requirement: Estado con icono y texto
Todo indicador de estado (éxito, advertencia, error, no disponible) SHALL mostrarse con un icono y un texto del catálogo, nunca solo con color.

#### Scenario: Estado de la base
- **WHEN** se muestra el estado "no disponible" de la base
- **THEN** el indicador incluye un icono y el texto del catálogo para ese estado

### Requirement: Contraste del tema oscuro
Los pares de colores de texto sobre fondo del tema SHALL tener una relación de contraste de al menos 4.5:1, y los indicadores de foco y bordes de controles al menos 3:1, verificados automáticamente.

#### Scenario: Tokens del tema
- **WHEN** la suite de tests calcula el contraste de los pares de tokens del tema
- **THEN** cada par de texto alcanza al menos 4.5:1 y el color de foco al menos 3:1 sobre los fondos

#### Scenario: Token con contraste insuficiente
- **WHEN** alguien cambia un token de texto a un color con contraste menor a 4.5:1 sobre su fondo
- **THEN** la suite de tests falla indicando el par infractor

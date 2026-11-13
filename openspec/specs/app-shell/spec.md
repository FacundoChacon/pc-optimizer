# app-shell Specification

## Purpose
Define el arranque de la aplicación de escritorio pc-optimizer, su ventana principal y el contrato mínimo de comunicación (IPC y errores) entre el backend y la interfaz, sobre el que se construyen todas las funcionalidades posteriores.

## Requirements

### Requirement: La aplicación arranca en una única ventana principal
La aplicación SHALL iniciar mostrando una única ventana principal titulada "PC Optimizer" que carga la interfaz empaquetada localmente, sin cargar contenido remoto.

#### Scenario: Arranque en desarrollo
- **WHEN** un desarrollador ejecuta la app en modo desarrollo desde una terminal NO elevada
- **THEN** se abre una ventana principal titulada "PC Optimizer" que muestra la pantalla inicial sin errores de CSP ni de IPC en la consola

#### Scenario: Interfaz solo local
- **WHEN** la app se ejecuta en un build de producción
- **THEN** todo el HTML, JS, CSS e imágenes de la interfaz se sirven desde los assets empaquetados y ningún recurso se solicita a un origen remoto

### Requirement: Tema oscuro base
La interfaz SHALL renderizarse con un tema oscuro base (fondo oscuro, texto claro y `color-scheme: dark`) desde la primera pantalla.

#### Scenario: Pantalla inicial en tema oscuro
- **WHEN** se renderiza la pantalla inicial
- **THEN** el contenedor raíz usa la paleta oscura base del tema

### Requirement: Textos de interfaz desde el catálogo en español
Todo texto visible de la interfaz SHALL provenir de un catálogo centralizado de strings en español; los componentes MUST NOT contener literales de texto visibles.

#### Scenario: Título desde el catálogo
- **WHEN** se renderiza la pantalla inicial
- **THEN** el título y los mensajes mostrados coinciden con las entradas del catálogo en español

### Requirement: Consulta de información de la aplicación por IPC
El backend SHALL exponer un comando IPC de solo lectura que devuelve el nombre y la versión de la aplicación, y la interfaz SHALL mostrarlos en la pantalla inicial obteniéndolos exclusivamente a través del cliente IPC tipado.

#### Scenario: Versión mostrada en la pantalla inicial
- **WHEN** la pantalla inicial se monta y el comando responde con nombre y versión
- **THEN** la interfaz muestra la versión devuelta por el backend

#### Scenario: Fallo del IPC
- **WHEN** el comando de información falla o el backend no está disponible
- **THEN** la interfaz muestra el mensaje de error del catálogo en español en lugar de la versión, sin romper el render

### Requirement: Contrato de error serializable
Todo comando IPC del backend SHALL devolver errores como un objeto serializado con un código estable (`kind`) y un mensaje legible (`message`), y el cliente IPC de la interfaz SHALL convertir cualquier rechazo en ese mismo tipo tipado.

#### Scenario: Error del backend con forma conocida
- **WHEN** un comando falla en el backend
- **THEN** la interfaz recibe un objeto con los campos `kind` y `message`

#### Scenario: Rechazo con forma desconocida
- **WHEN** la llamada IPC se rechaza con un valor que no tiene la forma `{ kind, message }`
- **THEN** el cliente IPC lo normaliza a un error de tipo `unknown` con un mensaje no vacío

### Requirement: Comandos IPC habilitados explícitamente
Cada comando IPC propio SHALL quedar sujeto al control de acceso del runtime y estar habilitado de forma explícita, por nombre, en las capabilities de la ventana principal; las capabilities MUST NOT otorgar comandos propios que no existan.

#### Scenario: Comando permitido
- **WHEN** la ventana principal invoca `get_app_info`
- **THEN** el runtime permite la invocación y devuelve la respuesta del comando

#### Scenario: Lista exacta de comandos habilitados
- **WHEN** la suite de tests compara los comandos propios declarados en el backend con los permisos de comandos de las capabilities
- **THEN** ambos conjuntos coinciden exactamente (hoy: solo `get_app_info`)

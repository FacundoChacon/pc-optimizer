## Purpose

Define la configuración de usuario de pc-optimizer: qué ajustes existen, cómo se leen y se modifican de forma validada a través del backend, y cómo se presentan en la pantalla Configuración, incluida la recuperación de la base local cuando está dañada.

## ADDED Requirements

### Requirement: Lectura de la configuración
El backend SHALL exponer por IPC la configuración actual con los campos `uiMode` (`simple` | `advanced`), `autostartTray` (booleano) y `language` (`es`).

#### Scenario: Configuración inicial
- **WHEN** la base fue recién creada y se consulta la configuración
- **THEN** la respuesta es `{ "uiMode": "simple", "autostartTray": false, "language": "es" }`

#### Scenario: Modo cambiado previamente
- **WHEN** `ui_mode` está guardado como `"advanced"` y se consulta la configuración
- **THEN** la respuesta tiene `uiMode = "advanced"`

### Requirement: Lectura tolerante con valores por defecto
Si un ajuste conocido falta en la base o tiene un valor de tipo o contenido inválido, la lectura SHALL devolver su valor por defecto (`simple`, `false`, `es`) en lugar de fallar, y MUST NOT modificar lo guardado.

#### Scenario: Modo con valor desconocido
- **WHEN** `ui_mode` está guardado como `"turbo"` y se consulta la configuración
- **THEN** la respuesta tiene `uiMode = "simple"` y el valor guardado sigue siendo `"turbo"`

#### Scenario: Ajuste ausente
- **WHEN** la clave `autostart_tray` no existe en la base y se consulta la configuración
- **THEN** la respuesta tiene `autostartTray = false`

#### Scenario: Tipo incorrecto
- **WHEN** `autostart_tray` está guardado como `"si"` (texto) y se consulta la configuración
- **THEN** la respuesta tiene `autostartTray = false`

### Requirement: Configuración con persistencia no disponible
Si la persistencia no está disponible, la lectura y la escritura de configuración MUST fallar con el error tipado de la persistencia (`store_corrupt`, `store_schema_too_new`, `store_migration_failed` o `store_unavailable`).

#### Scenario: Lectura con base dañada
- **WHEN** la base está dañada y se consulta la configuración
- **THEN** la respuesta es un error con `kind = "store_corrupt"`

#### Scenario: Escritura con base no disponible
- **WHEN** no se pudo resolver el directorio de datos y se intenta cambiar `ui_mode`
- **THEN** la respuesta es un error con `kind = "store_unavailable"`

### Requirement: Escritura validada de un ajuste
El backend SHALL exponer por IPC la escritura de un ajuste por clave y valor; solo las claves escribibles de la versión vigente (hoy únicamente `ui_mode`) con un valor válido se guardan, y la respuesta MUST ser la configuración completa resultante.

#### Scenario: Cambio a modo avanzado
- **WHEN** se escribe `ui_mode = "advanced"`
- **THEN** la respuesta tiene `uiMode = "advanced"` y una lectura posterior devuelve el mismo valor

#### Scenario: Vuelta a modo simple
- **WHEN** con `ui_mode = "advanced"` se escribe `ui_mode = "simple"`
- **THEN** la respuesta tiene `uiMode = "simple"`

### Requirement: Valores y claves inválidos rechazados
La escritura MUST rechazarse con el error `setting_invalid`, sin modificar la base, cuando la clave no existe o el valor no es válido para la clave.

#### Scenario: Valor fuera del enum
- **WHEN** se escribe `ui_mode = "turbo"`
- **THEN** la respuesta es un error con `kind = "setting_invalid"` y `ui_mode` conserva su valor previo

#### Scenario: Valor de tipo incorrecto
- **WHEN** se escribe `ui_mode = 1` o `ui_mode = null`
- **THEN** la respuesta es un error con `kind = "setting_invalid"`

#### Scenario: Clave desconocida
- **WHEN** se escribe `scan_roots = []` o una clave vacía
- **THEN** la respuesta es un error con `kind = "setting_invalid"` y no se crea ninguna fila nueva

### Requirement: Ajustes de solo lectura
Los ajustes conocidos que la versión vigente no permite modificar (`autostart_tray` hasta que exista el autostart, y `language`) MUST rechazarse con el error `setting_read_only`, sin modificar la base.

#### Scenario: Intento de activar el autostart
- **WHEN** se escribe `autostart_tray = true`
- **THEN** la respuesta es un error con `kind = "setting_read_only"` y `autostart_tray` sigue en `false`

#### Scenario: Intento de cambiar el idioma
- **WHEN** se escribe `language = "en"`
- **THEN** la respuesta es un error con `kind = "setting_read_only"`

### Requirement: Pantalla Configuración
La interfaz SHALL ofrecer una pantalla Configuración con las secciones Apariencia (modo de la interfaz), Inicio con Windows (autostart), Base de datos (estado y recuperación) y Acerca de (nombre y versión de la app), con textos del catálogo.

#### Scenario: Secciones visibles
- **WHEN** el usuario abre Configuración con la persistencia lista
- **THEN** ve las cuatro secciones con sus encabezados del catálogo y la versión devuelta por el backend

#### Scenario: Modo actual seleccionado
- **WHEN** el usuario abre Configuración con `uiMode = "advanced"`
- **THEN** el selector de modo muestra "Avanzado" como opción seleccionada

### Requirement: Autostart visible pero deshabilitado
El control de autostart SHALL mostrarse deshabilitado, con su estado actual y una explicación del catálogo de que estará disponible en una versión próxima, y MUST NOT enviar ninguna escritura al activarlo.

#### Scenario: Control deshabilitado
- **WHEN** el usuario abre Configuración
- **THEN** el control de autostart está deshabilitado, apagado y asociado a su texto explicativo

#### Scenario: Interacción sin efecto
- **WHEN** el usuario intenta activar el control de autostart con mouse o teclado
- **THEN** no se invoca ninguna escritura de configuración y el control sigue apagado

### Requirement: Estado de la base en Configuración
La sección Base de datos SHALL mostrar el estado de la persistencia con icono y texto del catálogo ("Lista" o "No disponible" con el motivo explicado en lenguaje llano) y ofrecer la acción "Respaldar y recrear" solo cuando el estado es recuperable.

#### Scenario: Base lista
- **WHEN** la persistencia está lista
- **THEN** la sección muestra el estado "Lista" y no ofrece la acción de recuperación

#### Scenario: Base dañada
- **WHEN** la persistencia no está disponible por base dañada
- **THEN** la sección muestra "No disponible", la explicación del catálogo para base dañada y el botón "Respaldar y recrear"

#### Scenario: Base de una versión más nueva
- **WHEN** la persistencia no está disponible por esquema más nuevo
- **THEN** la sección explica que la base fue creada por una versión más nueva de la app y no ofrece la acción de recuperación

### Requirement: Confirmación antes de recuperar la base
La acción "Respaldar y recrear" MUST pedir confirmación en un diálogo modal que explica qué pasará (se guarda una copia del archivo dañado y se crea una base nueva con la configuración inicial), con "Cancelar" como opción enfocada al abrir; solo la confirmación explícita SHALL invocar la recuperación.

#### Scenario: Cancelar
- **WHEN** el usuario abre el diálogo y elige "Cancelar" o presiona Escape
- **THEN** el diálogo se cierra, no se invoca la recuperación y el foco vuelve al botón "Respaldar y recrear"

#### Scenario: Confirmar
- **WHEN** el usuario confirma en el diálogo
- **THEN** se invoca la recuperación una sola vez y la acción queda deshabilitada mientras está en curso

### Requirement: Resultado de la recuperación
Tras una recuperación exitosa la interfaz SHALL anunciar, en una región en vivo, que la base se recreó y el nombre del archivo de respaldo, y recargar la configuración y el estado sin reiniciar; ante un error SHALL mostrar el mensaje del catálogo para su tipo y mantener la acción disponible si sigue siendo recuperable.

#### Scenario: Éxito
- **WHEN** la recuperación responde con el respaldo `data.db.corrupt-1700000000000.bak`
- **THEN** se anuncia el mensaje de éxito del catálogo con ese nombre, el estado pasa a "Lista" y los controles de configuración se habilitan

#### Scenario: Error al respaldar
- **WHEN** la recuperación falla con `kind = "store_unavailable"`
- **THEN** se muestra el mensaje del catálogo que sugiere cerrar otros programas y reintentar, y el botón "Respaldar y recrear" sigue disponible

### Requirement: Controles de configuración sin persistencia
Con la persistencia no disponible, la interfaz SHALL usar los valores por defecto (modo simple) y mostrar deshabilitados los controles que escriben configuración, con una explicación del catálogo.

#### Scenario: Selector de modo con base dañada
- **WHEN** el usuario abre Configuración con la base dañada
- **THEN** el selector de modo muestra "Simple", está deshabilitado y explica que primero hay que recuperar la base

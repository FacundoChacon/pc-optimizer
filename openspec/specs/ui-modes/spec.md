# ui-modes Specification

## Purpose
Define los dos modos de presentación de pc-optimizer (simple por defecto y avanzado opt-in): cómo se elige y persiste el modo, cómo cambia el layout del shell y la densidad de los componentes, y la garantía de que los modos solo cambian la presentación y nunca las protecciones ni el tema.

## Requirements

### Requirement: Modo simple por defecto
La app SHALL mostrarse en modo simple en el primer arranque y siempre que el modo guardado no pueda leerse; MUST NOT mostrar el layout de ningún modo antes de conocer el modo guardado.

#### Scenario: Primer arranque
- **WHEN** la app arranca con una base recién creada
- **THEN** la interfaz se muestra en modo simple

#### Scenario: Carga del modo guardado
- **WHEN** la app arranca con `ui_mode = "advanced"` guardado
- **THEN** mientras la configuración se carga se muestra solo el indicador de carga del catálogo, y luego la interfaz aparece directamente en modo avanzado

#### Scenario: Configuración ilegible
- **WHEN** la lectura de la configuración falla al arrancar
- **THEN** la interfaz se muestra en modo simple

### Requirement: Cambio de modo persistido sin reinicio
El usuario SHALL poder cambiar entre modo simple y avanzado desde Configuración; el cambio se guarda en `settings.ui_mode`, se aplica sin reiniciar, conserva la pantalla actual y se anuncia en una región en vivo.

#### Scenario: Activar modo avanzado
- **WHEN** el usuario elige "Avanzado" en Configuración y la escritura se confirma
- **THEN** el shell pasa al layout avanzado, la pantalla Configuración sigue visible y se anuncia el mensaje del catálogo de modo avanzado activado

#### Scenario: Modo persistido entre arranques
- **WHEN** el usuario activó el modo avanzado y vuelve a abrir la app
- **THEN** la app aparece en modo avanzado

#### Scenario: Volver a modo simple
- **WHEN** en modo avanzado el usuario elige "Simple"
- **THEN** el shell pasa al layout simple sin reiniciar

### Requirement: Cambio de modo fallido
Si la escritura del modo falla, el modo visible MUST NOT cambiar y la interfaz SHALL mostrar el mensaje de error del catálogo correspondiente a su tipo.

#### Scenario: Error al guardar
- **WHEN** el usuario elige "Avanzado" y la escritura responde con error `store_unavailable`
- **THEN** la interfaz sigue en modo simple, el selector vuelve a "Simple" y se muestra el mensaje del catálogo para base no disponible

### Requirement: Layout del modo simple
En modo simple el shell SHALL mostrar una cabecera con el nombre de la app y la navegación horizontal, y el contenido en tarjetas amplias con una sola acción primaria por pantalla como máximo.

#### Scenario: Estructura simple
- **WHEN** la interfaz está en modo simple
- **THEN** hay una navegación en la cabecera, no hay barra lateral y cada pantalla tiene como máximo una acción primaria

### Requirement: Layout del modo avanzado
En modo avanzado el shell SHALL mostrar una barra lateral de navegación con las mismas pantallas que el modo simple y el contenido con densidad compacta.

#### Scenario: Estructura avanzada
- **WHEN** la interfaz está en modo avanzado
- **THEN** hay una barra lateral de navegación con las mismas entradas que el modo simple y los componentes usan la densidad compacta

### Requirement: Mismo tema en ambos modos
Ambos modos SHALL usar el mismo tema oscuro (mismos tokens de color y `color-scheme: dark`); el modo solo cambia la disposición y la densidad.

#### Scenario: Tema invariable al cambiar de modo
- **WHEN** se cambia de modo simple a avanzado
- **THEN** el contenedor raíz conserva las mismas clases de color de fondo y de texto del tema

### Requirement: Componentes compartidos con densidad por modo
Los componentes de interfaz SHALL ser los mismos en ambos modos y adaptar su densidad al modo activo, sin pantallas duplicadas por modo.

#### Scenario: Mismo componente, otra densidad
- **WHEN** una misma tarjeta se renderiza en modo simple y en modo avanzado
- **THEN** es el mismo componente con el mismo contenido y solo cambia su densidad

### Requirement: Los modos no cambian las protecciones
El modo de la interfaz MUST NOT alterar ninguna regla, validación ni protección del backend; el backend SHALL comportarse igual cualquiera sea el modo guardado.

#### Scenario: Escritura inválida en modo avanzado
- **WHEN** con `ui_mode = "advanced"` se intenta escribir un ajuste de solo lectura
- **THEN** se rechaza con el mismo error que en modo simple

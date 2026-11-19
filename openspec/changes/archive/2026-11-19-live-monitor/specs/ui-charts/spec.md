## Purpose

Ofrecer componentes de visualización compartidos (medidor y serie temporal en vivo) con el tema oscuro de la app, compatibles con la CSP estricta y accesibles, para el monitor y para las pantallas que los reutilicen.

## ADDED Requirements

### Requirement: Medidor de proporción con severidad
El medidor SHALL dibujar una pista y un relleno proporcional al valor, acotado entre 0 % y 100 %, con el color de severidad del nivel recibido; el nivel MUST mostrarse también con icono y texto, y el valor MUST mostrarse en texto junto al medidor.

#### Scenario: Valor dentro de rango
- **WHEN** el medidor recibe 42 % con nivel normal
- **THEN** el relleno ocupa el 42 % de la pista con el color del acento y el texto muestra "42 %"

#### Scenario: Valor fuera de rango
- **WHEN** el medidor recibe 130 % o un valor no numérico
- **THEN** el relleno queda acotado a 100 % o a 0 % respectivamente

#### Scenario: Severidad sin depender del color
- **WHEN** el medidor recibe el nivel muy alto
- **THEN** el relleno usa el color de peligro y junto al medidor aparecen un icono y el texto del nivel

### Requirement: Serie temporal en vivo
La serie temporal SHALL dibujar una o dos series sobre un eje de tiempo fijo que termina en el instante de la última muestra y cubre la ventana indicada, con un solo eje vertical; los puntos ausentes y los huecos MUST cortar la línea, sin interpolar ni dibujar ceros.

#### Scenario: Ventana deslizante
- **WHEN** la serie recibe muestras de los últimos 5 minutos y llega una nueva
- **THEN** el eje de tiempo se desplaza para terminar en la nueva muestra y la más antigua fuera de la ventana deja de dibujarse

#### Scenario: Punto ausente
- **WHEN** una muestra intermedia tiene el valor ausente
- **THEN** la línea se interrumpe en esa muestra y continúa en la siguiente disponible

#### Scenario: Eje de porcentaje
- **WHEN** la serie es un porcentaje
- **THEN** el eje vertical va de 0 a 100 cualquiera sea el máximo de los datos

#### Scenario: Eje de tasas
- **WHEN** la serie es una tasa en bytes por segundo con pico de 37 MB/s
- **THEN** el eje vertical termina en un valor redondo mayor o igual al pico

### Requirement: Gráficos compatibles con la CSP estricta
Los componentes de gráficos SHALL dibujarse con atributos y clases del tema, y MUST NOT usar atributos `style`, elementos `<style>` en línea, evaluación de código ni recursos externos.

#### Scenario: Sin estilos en línea
- **WHEN** se renderizan un medidor y una serie temporal con cursor y lectura visibles
- **THEN** ningún elemento generado tiene atributo `style` y no hay elementos `<style>`

### Requirement: Gráficos accesibles
Cada serie temporal SHALL tener un nombre accesible, permitir recorrer sus valores con el teclado, ofrecer una vista de tabla con las últimas muestras y, con dos series, una leyenda además de etiquetas directas; los gráficos MUST NOT animar transiciones.

#### Scenario: Lectura por teclado
- **WHEN** el gráfico tiene el foco y el usuario pulsa la flecha izquierda
- **THEN** el cursor se mueve a la muestra anterior y la lectura muestra su hora y su valor en texto

#### Scenario: Vista de tabla
- **WHEN** el usuario activa "Ver tabla"
- **THEN** se muestra una tabla con la hora y el valor de cada serie de las últimas muestras, con "No disponible" en las ausentes

#### Scenario: Dos series
- **WHEN** el gráfico muestra lectura y escritura
- **THEN** hay una leyenda con ambos nombres y cada línea tiene su etiqueta en el extremo

### Requirement: Colores del tema oscuro validados
Los colores de las series y de la severidad SHALL salir de los tokens del tema oscuro; el contraste de cada color de serie contra la superficie MUST cumplir el mínimo para elementos gráficos (3:1) y los textos MUST usar los tokens de texto, nunca el color de la serie.

#### Scenario: Contraste de las series
- **WHEN** la suite de tests calcula el contraste de los colores de serie contra la superficie del tema
- **THEN** todos alcanzan al menos 3:1

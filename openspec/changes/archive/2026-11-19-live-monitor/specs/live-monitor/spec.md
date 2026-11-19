## Purpose

Mostrar en vivo cuánto usan el procesador, la memoria y el disco del sistema, muestreando en segundo plano con un costo acotado, sin persistir nada y sin inventar valores cuando una métrica no se puede medir.

## ADDED Requirements

### Requirement: Muestreo periódico en segundo plano
El sistema SHALL muestrear las métricas cada 1 segundo en un hilo propio que arranca con la aplicación y corre mientras la aplicación vive, haya o no una pantalla mirando; una lectura lenta MUST NOT bloquear la interfaz ni los comandos IPC.

#### Scenario: Una muestra por intervalo
- **WHEN** el muestreo corre durante 10 intervalos con sensores simulados
- **THEN** se producen 10 muestras con número de secuencia estrictamente creciente y separadas por el intervalo configurado

#### Scenario: Muestreo sin pantalla abierta
- **WHEN** la aplicación arranca y nadie se suscribió al monitor durante 30 segundos
- **THEN** la ventana en memoria ya contiene las muestras de esos 30 segundos

#### Scenario: Sin ráfagas tras un atraso
- **WHEN** el hilo de muestreo se atrasa más de un intervalo
- **THEN** el siguiente plazo se calcula desde el instante actual y no se toman muestras seguidas para recuperar las perdidas

### Requirement: Métricas del sistema muestreadas
Cada muestra SHALL incluir, cuando estén disponibles: uso total de CPU (0–100 %), memoria usada y total en bytes, espacio total y libre del volumen del sistema en bytes, tiempo activo del disco del volumen del sistema (0–100 %) y bytes leídos y escritos por segundo en ese volumen; ningún otro volumen ni dato por usuario MUST incluirse.

#### Scenario: Muestra completa
- **WHEN** todas las fuentes responden
- **THEN** la muestra contiene las siete métricas dentro de sus rangos (porcentajes entre 0 y 100, memoria usada no mayor que la total, espacio libre no mayor que el total, tasas no negativas)

#### Scenario: Solo el volumen del sistema
- **WHEN** la máquina tiene otros volúmenes además del del sistema
- **THEN** las métricas de espacio y actividad de disco corresponden solo al volumen del sistema

### Requirement: Métricas no disponibles informadas como ausencia
Una métrica que no se puede medir SHALL informarse como ausente con un motivo estable (`warming_up`, `after_gap`, `source_error`, `not_supported`) y MUST NOT informarse como cero ni estimarse; el resto de la muestra SHALL conservar sus valores.

#### Scenario: Primera lectura
- **WHEN** se toma la primera muestra tras arrancar
- **THEN** CPU, tiempo activo y tasas de disco están ausentes con motivo `warming_up` y memoria y espacio tienen su valor

#### Scenario: Fuente que falla
- **WHEN** la fuente del tiempo activo del disco falla en una lectura
- **THEN** esa métrica queda ausente con motivo `source_error` y las demás métricas de la muestra tienen su valor

#### Scenario: Contador que retrocede
- **WHEN** el contador acumulado de bytes leídos es menor que en la lectura anterior
- **THEN** la tasa de lectura de esa muestra queda ausente y no se informa un valor negativo ni cero

### Requirement: Ventana en memoria de cinco minutos
El sistema SHALL conservar en memoria como máximo las últimas 300 muestras y ninguna con más de 300 segundos de antigüedad respecto de la más reciente; las muestras MUST NOT persistirse en disco ni en la base local.

#### Scenario: Desalojo por cantidad
- **WHEN** llega la muestra número 301
- **THEN** la ventana conserva 300 muestras y la más antigua fue descartada

#### Scenario: Desalojo por antigüedad
- **WHEN** llega una muestra 10 minutos después de la anterior
- **THEN** la ventana conserva solo las muestras de los últimos 300 segundos

#### Scenario: Sin persistencia
- **WHEN** el monitor corre con la base local no disponible
- **THEN** el muestreo y la ventana funcionan igual y no se escribe nada en la base

### Requirement: Huecos tras suspensión registrados como ausencia
Cuando el tiempo entre dos muestras excede claramente el intervalo (suspensión, hibernación o bloqueo del hilo) o el reloj de pared salta, el sistema SHALL marcar la muestra siguiente como posterior a un hueco y descartar en ella las métricas que promedian el período; el hueco MUST NOT rellenarse con ceros ni interpolarse.

#### Scenario: Suspensión
- **WHEN** entre dos muestras pasan 10 minutos según el reloj de pared
- **THEN** la nueva muestra está marcada como posterior a un hueco, su CPU, tiempo activo y tasas están ausentes con motivo `after_gap` y su memoria y espacio tienen valor

#### Scenario: Reloj de pared hacia atrás
- **WHEN** el reloj de pared retrocede entre dos muestras
- **THEN** la ventana se vacía y la nueva muestra queda marcada como posterior a un hueco

#### Scenario: Intervalo normal
- **WHEN** entre dos muestras pasa un intervalo con una variación menor a la tolerancia
- **THEN** la nueva muestra no está marcada como posterior a un hueco

### Requirement: Resumen y niveles por métrica
El sistema SHALL calcular para CPU, memoria y tiempo activo del disco el valor actual, el promedio de los últimos 60 segundos, el pico de la ventana y un nivel (`normal`, `high`, `very_high`, `unknown`) a partir del promedio, y para el espacio del volumen el porcentaje usado y un nivel (`normal`, `almost_full`, `unknown`); las métricas ausentes MUST NOT contar en los cálculos.

#### Scenario: Niveles en los bordes
- **WHEN** el promedio de 60 s de CPU es 69,9 %, 70 % y 90 %
- **THEN** el nivel es `normal`, `high` y `very_high` respectivamente

#### Scenario: Memoria y disco
- **WHEN** el promedio de 60 s de memoria usada es 85 % y el de tiempo activo del disco es 95 %
- **THEN** el nivel de memoria es `high` y el de disco es `very_high`

#### Scenario: Espacio casi lleno
- **WHEN** el volumen del sistema tiene usado el 92 % de su capacidad
- **THEN** el nivel de espacio es `almost_full`

#### Scenario: Sin datos disponibles
- **WHEN** todas las muestras de los últimos 60 s tienen la CPU ausente
- **THEN** el resumen de CPU tiene valores nulos y nivel `unknown`

#### Scenario: Pico momentáneo
- **WHEN** la CPU marca 100 % durante un segundo y 10 % el resto del último minuto
- **THEN** el pico es 100 % y el nivel sigue `normal`

### Requirement: Independencia de los trabajos de escaneo y limpieza
El muestreo y los comandos del monitor SHALL funcionar con cualquier escaneo, limpieza o vaciado de Papelera en curso, y MUST NOT impedir que esos trabajos empiecen.

#### Scenario: Suscripción durante un escaneo
- **WHEN** hay un análisis de carpetas en curso y la interfaz se suscribe al monitor
- **THEN** la suscripción se acepta y las muestras siguen llegando durante el análisis

#### Scenario: Escaneo con el monitor activo
- **WHEN** hay una suscripción al monitor activa y se inicia un escaneo de basura
- **THEN** el escaneo empieza normalmente y no responde ningún error de trabajo en curso

### Requirement: Costo del muestreo medido y acotado
El sistema SHALL medir el tiempo que tarda cada lectura de sensores e informar su promedio en el resumen; el muestreo con la pantalla cerrada MUST mantener el uso de CPU del proceso por debajo del 1 % de la CPU total, verificado en el smoke guiado.

#### Scenario: Costo informado
- **WHEN** las lecturas simuladas tardan 2 ms cada una
- **THEN** el resumen informa un costo medio de lectura de 2 000 microsegundos

#### Scenario: Presupuesto verificado
- **WHEN** en el smoke se mide durante 5 minutos el uso de CPU del proceso con el monitor muestreando y la pantalla cerrada
- **THEN** el promedio queda por debajo del 1 % de la CPU total

### Requirement: Fallo del muestreo sin afectar a la aplicación
Si el muestreo falla de forma irrecuperable o no puede arrancar, el sistema SHALL dejar de muestrear, avisar al suscriptor vigente y responder `monitor_unavailable` a nuevas suscripciones, y el resto de la aplicación MUST seguir funcionando; el muestreo MUST NOT reiniciarse automáticamente.

#### Scenario: Falla interna durante el muestreo
- **WHEN** una lectura de sensores falla de forma inesperada dentro del hilo
- **THEN** el suscriptor recibe un aviso de detención con motivo `sampler_failed` y los demás comandos siguen respondiendo

#### Scenario: Suscripción con el muestreo caído
- **WHEN** la interfaz se suscribe después de una falla del muestreo
- **THEN** recibe el error `monitor_unavailable`

### Requirement: Flujo de muestras por suscripción
La interfaz SHALL recibir las muestras suscribiéndose con un canal de la propia invocación: la respuesta trae la ventana actual y el último resumen, y cada muestra posterior llega por el canal con su resumen, sin duplicados ni pérdidas entre la ventana y el flujo; la interfaz MUST NOT consultar periódicamente.

#### Scenario: Ventana inicial y continuación
- **WHEN** la interfaz se suscribe con 120 muestras en la ventana
- **THEN** la respuesta trae esas 120 muestras ordenadas y la primera muestra del canal tiene el número de secuencia siguiente a la última de la ventana

#### Scenario: Un solo suscriptor
- **WHEN** la interfaz se suscribe por segunda vez sin cancelar la primera suscripción
- **THEN** las muestras siguientes llegan solo al canal de la segunda suscripción

#### Scenario: Cancelación idempotente
- **WHEN** la interfaz cancela una suscripción que ya fue reemplazada
- **THEN** la respuesta es exitosa y la suscripción vigente sigue recibiendo muestras

#### Scenario: Canal roto
- **WHEN** el envío de una muestra por el canal del suscriptor falla
- **THEN** el suscriptor se descarta y el muestreo continúa

### Requirement: Comandos IPC del monitor
El backend SHALL exponer `start_monitor_stream` (con el canal de muestras) y `stop_monitor_stream` (con el identificador de suscripción), con respuestas en camelCase sin rutas ni datos de usuario; un identificador que no sea un entero seguro de JavaScript MUST rechazarse con `invalid_argument`.

#### Scenario: Forma de la respuesta
- **WHEN** la interfaz invoca `start_monitor_stream`
- **THEN** recibe `subscriptionId`, `intervalMs` = 1000, `windowMs` = 300000, `samples` y `summary` (nulo si todavía no hay muestras)

#### Scenario: Identificador inválido
- **WHEN** la interfaz invoca `stop_monitor_stream` con un identificador negativo o no entero
- **THEN** recibe el error `invalid_argument`

### Requirement: Monitor sin bloqueo por contexto de privilegios
El monitor SHALL funcionar igual con el contexto de privilegios coincidente o no coincidente, porque solo informa métricas de la máquina.

#### Scenario: Elevación por otra cuenta
- **WHEN** el contexto de privilegios es `mismatch` y la interfaz se suscribe al monitor
- **THEN** la suscripción se acepta y las muestras tienen las mismas métricas que en `match`

### Requirement: Pantalla Monitor en modo simple
En modo simple la pantalla Monitor SHALL mostrar tres tarjetas (Procesador, Memoria y Disco del sistema) con el valor actual y el nivel en palabras con icono, sin acción primaria, y un estado explícito para métricas en medición, no disponibles con su explicación, sin datos recientes o monitor no disponible.

#### Scenario: Tarjetas con nivel
- **WHEN** la CPU tiene nivel `high` y la memoria `normal`
- **THEN** la tarjeta Procesador muestra el porcentaje y "Alto" con icono, la de Memoria muestra porcentaje, usado de total y "Normal", y no hay botón primario

#### Scenario: Métrica no disponible
- **WHEN** el tiempo activo del disco está ausente con motivo `not_supported`
- **THEN** la tarjeta del disco muestra "No disponible" con una explicación breve y no muestra 0 %

#### Scenario: Sin datos recientes
- **WHEN** no llega ninguna muestra durante 3 intervalos
- **THEN** la pantalla marca los valores como "sin datos recientes"

#### Scenario: Cambio de nivel anunciado
- **WHEN** el nivel de memoria pasa de `normal` a `high`
- **THEN** se anuncia de forma cortés el cambio y los valores que cambian cada segundo no se anuncian

### Requirement: Pantalla Monitor en modo avanzado
En modo avanzado la pantalla Monitor SHALL mostrar los medidores de CPU, memoria, tiempo activo y espacio con valor actual, promedio de 60 s y pico, un gráfico en vivo de los últimos 5 minutos por métrica (CPU, memoria, tiempo activo y lectura/escritura) y un panel de detalles con intervalo, muestras en la ventana, huecos, costo medio de lectura y disponibilidad de cada métrica con su código.

#### Scenario: Gráficos en vivo
- **WHEN** llega una muestra nueva
- **THEN** cada gráfico incorpora el punto nuevo y su eje de tiempo sigue cubriendo los últimos 5 minutos

#### Scenario: Hueco en el gráfico
- **WHEN** la ventana contiene una muestra marcada como posterior a un hueco
- **THEN** las líneas de los gráficos se cortan en ese punto

#### Scenario: Detalles
- **WHEN** el usuario mira el panel de detalles
- **THEN** ve el intervalo de 1 s, la cantidad de muestras, los huecos de la ventana, el costo medio de lectura y el código de disponibilidad de cada métrica

### Requirement: Suscripción solo mientras se usa
La interfaz SHALL compartir una única suscripción entre todas las pantallas que muestran el monitor, abrirla cuando la primera se muestra y cancelarla cuando la última se oculta; con la ventana oculta MUST NOT redibujar los gráficos, aunque siga guardando las muestras.

#### Scenario: Salir de la pantalla
- **WHEN** el usuario navega de Monitor a Limpieza
- **THEN** la interfaz cancela su suscripción al monitor

#### Scenario: Volver a la pantalla
- **WHEN** el usuario vuelve a Monitor
- **THEN** se abre una suscripción nueva y los gráficos muestran la ventana sin puntos duplicados

## Purpose

Entregar la app a usuarios de Windows como un instalador verificable y honesto sobre sus limitaciones (sin firma, siempre elevada, sistemas probados), con una versión única y un proceso de release que nunca publica sin una acción explícita del dueño.

## ADDED Requirements

### Requirement: Instalador NSIS sin firma
La distribución de v0.x SHALL consistir en un único instalador NSIS para Windows x64, sin firma de código, generado por el empaquetador de Tauri a partir del commit verificado; MUST NOT publicarse un paquete MSI ni otros formatos en v0.1.

#### Scenario: Artefactos del empaquetado
- **WHEN** se genera el instalador de la versión 0.1.0
- **THEN** se produce exactamente un instalador `.exe` NSIS para x64 cuyo nombre incluye la versión, y ningún `.msi`

#### Scenario: Instalador descargado de internet
- **WHEN** el usuario abre el instalador descargado con un navegador en Windows 11
- **THEN** Windows puede mostrar el aviso de SmartScreen de editor desconocido, tal como lo describen las notas del release

### Requirement: Instalación por máquina en una ubicación protegida
El instalador SHALL instalar la app para todos los usuarios en la carpeta de programas del sistema, solicitando elevación al iniciar, de modo que el ejecutable que siempre corre elevado no pueda ser reemplazado por un proceso sin privilegios de administrador.

#### Scenario: Ubicación de instalación
- **WHEN** el usuario completa la instalación con las opciones por defecto
- **THEN** el ejecutable queda en `%ProgramFiles%\PC Optimizer\` y la entrada de desinstalación se registra para la máquina

#### Scenario: Escritura sin privilegios
- **WHEN** un proceso no elevado intenta modificar el ejecutable instalado
- **THEN** Windows lo deniega

#### Scenario: UAC del instalador cancelado
- **WHEN** el usuario cancela el aviso de UAC del instalador
- **THEN** no se instala nada

### Requirement: Instalador en español
Las pantallas del instalador y del desinstalador SHALL mostrarse en español.

#### Scenario: Idioma de las pantallas
- **WHEN** el usuario ejecuta el instalador en un Windows con cualquier idioma de interfaz
- **THEN** los textos de las pantallas del instalador están en español

### Requirement: Arranque al terminar la instalación sin fallos silenciosos
Si el instalador ofrece abrir la app al terminar, la app SHALL iniciarse elevada (pidiendo UAC si corresponde) o el instalador MUST NOT ofrecer esa opción; MUST NOT quedar una opción que falle en silencio o con el error 740.

#### Scenario: Abrir la app desde la página final
- **WHEN** el usuario deja marcada la opción de abrir la app y termina la instalación
- **THEN** Windows pide UAC y, si el usuario lo aprueba, la app inicia elevada; si lo cancela, la app no inicia y no aparece ningún error

#### Scenario: Opción que no puede elevar
- **WHEN** el mecanismo del instalador para abrir la app no puede pedir elevación (falla con el error 740 o no hace nada)
- **THEN** la página final no ofrece abrir la app y las notas del release indican abrirla desde el menú Inicio

### Requirement: Datos del usuario conservados al desinstalar
La desinstalación SHALL eliminar los archivos del programa, los accesos directos y la entrada de desinstalación, y MUST conservar el directorio de datos de la app (`%LOCALAPPDATA%\pc-optimizer` del perfil que corrió la app), con independencia de la casilla de eliminar datos del desinstalador; las notas del release SHALL indicar cómo borrarlo a mano.

#### Scenario: Desinstalación con la casilla de datos marcada
- **WHEN** el usuario desinstala marcando la casilla de eliminar los datos de la aplicación
- **THEN** se eliminan los datos del componente web asociados al identificador de la app y se conserva `%LOCALAPPDATA%\pc-optimizer` con su base y su log de limpiezas

#### Scenario: Desinstalación sin marcar la casilla
- **WHEN** el usuario desinstala sin marcar la casilla
- **THEN** se eliminan los archivos del programa y los accesos directos, y los datos quedan intactos

#### Scenario: Reinstalación
- **WHEN** el usuario reinstala la misma versión tras desinstalar
- **THEN** la app abre con la configuración y el historial previos

### Requirement: Versión única de la aplicación
La versión de la app SHALL ser la misma en la configuración de Tauri, el manifiesto del crate Rust y el `package.json`, y la suite de tests MUST fallar si difieren o si alguna no es una versión semántica `MAJOR.MINOR.PATCH`.

#### Scenario: Versiones sincronizadas
- **WHEN** las tres fuentes declaran `0.1.0`
- **THEN** el test de versión pasa y la pantalla inicial muestra `0.1.0`

#### Scenario: Versión desincronizada
- **WHEN** una de las tres fuentes declara una versión distinta
- **THEN** el test falla nombrando la fuente y ambas versiones

### Requirement: Tag coincidente con la versión
El release SHALL usar el tag `v<versión>` con la versión única de la app y MUST fallar antes de construir si el tag ya existe en el repositorio.

#### Scenario: Tag nuevo
- **WHEN** se dispara el release con la versión `0.1.0` y no existe `v0.1.0`
- **THEN** el release continúa con el tag `v0.1.0`

#### Scenario: Tag existente
- **WHEN** se dispara el release y `v0.1.0` ya existe
- **THEN** el workflow falla antes de construir, sin modificar el tag ni ningún release

### Requirement: Release en borrador disparado por el dueño
El release SHALL dispararse solo manualmente desde `main`, ejecutar todos los gates del pipeline de CI, y solo si todos pasan crear el tag en el commit construido y un release de GitHub en estado **borrador**; MUST NOT publicar el release ni dispararse por push, PR o calendario.

#### Scenario: Gates en verde
- **WHEN** el dueño dispara el release desde `main` y todos los gates pasan
- **THEN** existe el tag `v0.1.0` apuntando al commit construido y un release borrador con el instalador, `SHA256SUMS.txt` y las notas, sin publicar

#### Scenario: Gate en rojo
- **WHEN** algún gate falla durante el release
- **THEN** no se crea el tag ni el release

#### Scenario: Disparo desde otra rama
- **WHEN** se dispara el release desde una rama distinta de `main`
- **THEN** el workflow falla sin construir ni crear nada

#### Scenario: Publicación
- **WHEN** el borrador existe
- **THEN** queda invisible para el público hasta que el dueño lo publica a mano

### Requirement: Integridad de los artefactos del release
El release SHALL adjuntar un archivo `SHA256SUMS.txt` con la suma SHA-256 de cada instalador adjunto, calculada en la misma ejecución que lo construyó.

#### Scenario: Verificación por el usuario
- **WHEN** un usuario calcula la suma SHA-256 del instalador descargado
- **THEN** coincide con la línea correspondiente de `SHA256SUMS.txt`

### Requirement: Notas del release honestas
Las notas del release SHALL indicar en español: que el instalador no está firmado y cómo se ve el aviso de SmartScreen; que la app pide UAC en cada arranque y por qué; los sistemas verificados (Windows 11) y no verificados (Windows 10 22H2); que la app no usa red ni telemetría; dónde guarda sus datos y cómo eliminarlos; y cómo verificar la suma SHA-256.

#### Scenario: Contenido mínimo
- **WHEN** se revisa el borrador del release v0.1.0
- **THEN** las notas contienen cada uno de esos puntos

#### Scenario: Windows 10 no verificado
- **WHEN** un usuario de Windows 10 lee las notas
- **THEN** encuentra que Windows 10 22H2 no fue verificado manualmente en v0.1

### Requirement: Verificación manual guiada del instalador
Antes de crear el release, el instalador generado por el pipeline SHALL verificarse manualmente con el dueño en Windows 11: instalación, arranque elevado, recorrido de las funciones de v0.1, ausencia de conexiones de red del proceso, cancelación de UAC, rendimiento del análisis de carpetas y desinstalación; Windows 10 22H2 MUST quedar registrado como no verificado.

#### Scenario: Smoke completo en Windows 11
- **WHEN** termina la verificación guiada
- **THEN** cada paso tiene su resultado anotado en el change y cualquier diferencia detuvo el proceso hasta que el dueño decidió

#### Scenario: Proceso sin conexiones de red
- **WHEN** la app instalada está abierta y se listan las conexiones TCP del sistema
- **THEN** ninguna pertenece al proceso de la app

#### Scenario: Análisis completo del volumen del sistema en release
- **WHEN** el dueño analiza `C:` completo con la app instalada
- **THEN** se anotan duración, cantidad de archivos y memoria pico frente al objetivo de 5 minutos; si se supera, el release queda en espera hasta que el dueño decida

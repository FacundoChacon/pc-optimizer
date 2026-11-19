# domain-ports Specification

## Purpose
Define los contratos (puertos) entre la lógica de dominio pura de pc-optimizer y el sistema operativo o la persistencia, de modo que toda regla de negocio pueda probarse con dobles en memoria sin tocar el disco, la Papelera, los sensores ni la base del usuario.

## Requirements

### Requirement: Puertos definidos en el dominio sin I/O
El dominio SHALL declarar como contratos abstractos el acceso al sistema de archivos, a la Papelera, a los sensores de rendimiento, al reloj y a la persistencia; las definiciones MUST NOT depender de APIs de archivos, procesos, red, Windows, SQLite ni Tauri.

#### Scenario: Contratos declarados en la capa de dominio
- **WHEN** la suite de arquitectura recorre los archivos de la capa de dominio, incluidos los contratos y sus dobles de prueba
- **THEN** no encuentra imports de `std::fs`, `std::process`, `std::net`, `windows`, `rusqlite` ni `tauri`

#### Scenario: Implementaciones reales fuera del dominio
- **WHEN** se busca la implementación real de un contrato de puerto
- **THEN** está en la capa de adaptadores y no en la de dominio

### Requirement: Operaciones de puerto síncronas y con errores tipados
Cada operación falible de un puerto SHALL ser síncrona y devolver un resultado con un error tipado que distinga al menos: no encontrado, acceso denegado, recurso en uso, operación no soportada y error de E/S genérico; MUST NOT abortar el proceso ante un fallo del sistema.

#### Scenario: Recurso inexistente
- **WHEN** se consulta la metadata de una ruta que no existe
- **THEN** el puerto devuelve el error "no encontrado" con la ruta en el detalle

#### Scenario: Archivo bloqueado al moverlo a la Papelera
- **WHEN** se intenta mover a la Papelera un archivo abierto en exclusiva por otro proceso
- **THEN** el puerto devuelve el error "recurso en uso" y el archivo no se modifica

### Requirement: Metadata sin seguir puntos de reanálisis
El puerto de sistema de archivos SHALL informar, para una ruta o para cada entrada de un directorio, su tipo, tamaño en bytes, fecha de modificación en UTC epoch ms, si es punto de reanálisis (junction/symlink) y si es contenido de nube, sin seguir el destino de los puntos de reanálisis. La metadata de una ruta MUST incluir además su identidad de archivo (volumen + id) y cantidad de hard links cuando el sistema los provea; las entradas de un listado pueden omitirlos.

#### Scenario: Junction informada como punto de reanálisis
- **WHEN** se lista un directorio que contiene una junction hacia otra carpeta
- **THEN** la entrada de la junction se informa como punto de reanálisis y su contenido de destino no aparece en el listado

#### Scenario: Marcador de posición en la nube
- **WHEN** se consulta la metadata de un archivo de OneDrive que solo existe en la nube
- **THEN** la metadata lo marca como marcador de posición en la nube

#### Scenario: Archivo de nube hidratado
- **WHEN** se consulta la metadata de un archivo de OneDrive descargado localmente que conserva su tag de reanálisis de proveedor de nube
- **THEN** la metadata lo marca como contenido de nube

#### Scenario: Identidad estable de un archivo
- **WHEN** se consulta dos veces la metadata del mismo archivo sin modificarlo
- **THEN** ambas informan la misma identidad de archivo

#### Scenario: Identidad distinta tras recrear
- **WHEN** un archivo se borra y se crea otro con el mismo nombre
- **THEN** la metadata del nuevo informa una identidad distinta

#### Scenario: Hard links informados
- **WHEN** se consulta la metadata de un archivo que tiene dos nombres
- **THEN** la metadata informa 2 hard links

#### Scenario: Metadata de un symlink sin seguirlo
- **WHEN** se consulta la metadata de un symlink hacia un archivo de 1 MB
- **THEN** la metadata describe el symlink como punto de reanálisis, con su propia identidad y no la del destino

### Requirement: Canonicalización explícita
El puerto de sistema de archivos SHALL ofrecer la canonicalización de una ruta como operación propia que puede fallar, devolviendo la ruta absoluta real con nombres largos y sin seguir el último componente si es punto de reanálisis; una ruta que no puede canonicalizarse o cuya ruta real no tiene letra de unidad MUST devolver error y nunca una ruta parcial.

#### Scenario: Ruta con segmentos relativos
- **WHEN** se canonicaliza `C:\Users\ana\Temp\..\Temp\a.tmp` y la ruta existe
- **THEN** el resultado es la ruta absoluta sin segmentos `..`

#### Scenario: Ruta imposible de canonicalizar
- **WHEN** se canonicaliza una ruta inexistente
- **THEN** el puerto devuelve un error

#### Scenario: Nombre corto expandido
- **WHEN** se canonicaliza una ruta que usa el nombre corto 8.3 de una carpeta existente
- **THEN** el resultado usa el nombre largo de esa carpeta

#### Scenario: Symlink como último componente
- **WHEN** se canonicaliza la ruta de un symlink hacia otra carpeta
- **THEN** el resultado es la ruta del symlink y no la de su destino

#### Scenario: Junction intermedia
- **WHEN** se canonicaliza `<tmp>\j\a.txt` donde `<tmp>\j` es una junction hacia `<tmp>\real`
- **THEN** el resultado es `<tmp>\real\a.txt`

### Requirement: Papelera sin borrado permanente
El puerto de Papelera SHALL ofrecer únicamente mover a la Papelera un elemento ya re-verificado (el objetivo que entrega la re-verificación anti-TOCTOU, nunca una ruta suelta); MUST NOT exponer ninguna operación de borrado permanente ni de vaciado (el vaciado se incorpora solo con su propio change aprobado), y su error MUST distinguir: en uso, Papelera no disponible, elemento demasiado grande, ruta demasiado larga, acceso denegado, no encontrado, mecanismo inutilizable y otro fallo.

#### Scenario: Elemento movido a la Papelera
- **WHEN** se mueve a la Papelera un archivo existente y no bloqueado
- **THEN** la operación tiene éxito y el archivo deja de estar en su ruta original

#### Scenario: Volumen sin Papelera
- **WHEN** se intenta mover a la Papelera un archivo de un volumen cuya Papelera no está disponible o está deshabilitada
- **THEN** el puerto devuelve el error "Papelera no disponible" y el archivo no se borra

#### Scenario: Superficie del contrato auditada
- **WHEN** se revisan las operaciones del contrato de Papelera
- **THEN** la única operación que modifica archivos es mover a la Papelera

#### Scenario: Solo objetivos re-verificados
- **WHEN** un componente intenta mover a la Papelera una ruta que no pasó por la re-verificación
- **THEN** el contrato no lo permite: la única entrada aceptada es el objetivo re-verificado

#### Scenario: Último componente no seguido
- **WHEN** el objetivo resulta ser una junction o un symlink en el instante de moverlo
- **THEN** se mueve el enlace y su destino queda intacto

#### Scenario: Fallo interno del mecanismo
- **WHEN** el mecanismo de Papelera del sistema no puede inicializarse o falla de forma interna
- **THEN** el puerto devuelve el error "mecanismo inutilizable" y el proceso sigue funcionando

### Requirement: Muestra de sensores
El puerto de sensores SHALL devolver una muestra instantánea con uso de CPU (porcentaje 0–100), memoria usada y total en bytes y actividad de disco (porcentaje 0–100) cuando esté disponible; una métrica no disponible MUST informarse como ausente y no como cero.

#### Scenario: Actividad de disco no disponible
- **WHEN** la fuente de datos no puede medir la actividad de disco
- **THEN** la muestra informa la actividad de disco como ausente y el resto de métricas con su valor

#### Scenario: Muestra completa
- **WHEN** todas las fuentes responden
- **THEN** la muestra contiene CPU, memoria usada, memoria total y actividad de disco dentro de sus rangos

### Requirement: Reloj en UTC epoch milisegundos
El tiempo que usan el dominio y la persistencia SHALL obtenerse de un puerto de reloj que devuelve el instante actual en milisegundos UTC desde la época Unix, de modo que los tests puedan fijarlo.

#### Scenario: Reloj fijo en tests
- **WHEN** un test usa un reloj fijado en `1700000000000`
- **THEN** todo timestamp generado a partir de ese reloj vale `1700000000000`

#### Scenario: Reloj del sistema
- **WHEN** se consulta el reloj real
- **THEN** devuelve un valor positivo en milisegundos UTC, independiente de la zona horaria configurada en Windows

### Requirement: Persistencia segregada por agregado
El acceso a la persistencia SHALL dividirse en contratos independientes para configuración, escaneos, log de limpieza y hallazgos de basura, de modo que cada consumidor dependa solo del que usa y los dobles de prueba no deban implementar operaciones ajenas.

#### Scenario: Consumidor de solo configuración
- **WHEN** un componente necesita leer y escribir configuración
- **THEN** puede recibir solo el contrato de configuración, sin depender de escaneos ni del log

#### Scenario: Consumidor de hallazgos
- **WHEN** la detección de basura guarda los resultados de un escaneo
- **THEN** depende solo de los contratos de escaneos y de hallazgos, sin depender de configuración ni del log de limpieza

#### Scenario: Hallazgos sin operaciones de borrado de archivos
- **WHEN** se revisan las operaciones del contrato de hallazgos
- **THEN** solo guardan y consultan registros de la base; ninguna actúa sobre archivos del disco

### Requirement: Dobles de prueba con el mismo contrato
Cada puerto SHALL tener un doble en memoria para tests que cumpla el mismo contrato observable que su adaptador real; para la persistencia y el sistema de archivos, una misma batería de tests de contrato MUST pasar contra el doble y contra el adaptador real sobre directorios temporales.

#### Scenario: Batería de contrato compartida
- **WHEN** se ejecuta la batería de contrato de persistencia contra el doble en memoria y contra una base SQLite temporal
- **THEN** ambas pasan los mismos casos (seed, settings, escaneos, retención, log, hallazgos, resumen por categoría, retención de hallazgos y escaneos interrumpidos)

#### Scenario: Batería de contrato del sistema de archivos
- **WHEN** se ejecuta la batería de contrato de sistema de archivos contra el doble y contra el adaptador real sobre un directorio temporal con archivos, carpetas, una junction y un symlink
- **THEN** ambos pasan los mismos casos (metadata, listado, canonicalización, identidad, hard links, errores)

#### Scenario: Doble de sistema de archivos configurable
- **WHEN** un test configura en el doble una junction, un archivo bloqueado y un marcador en la nube
- **THEN** el doble responde a cada operación como lo haría el sistema real en esos casos, sin tocar el disco

#### Scenario: Doble de la Papelera configurable
- **WHEN** un test configura el doble de consulta de la Papelera con un tamaño, una cantidad o un error
- **THEN** la consulta devuelve exactamente eso sin tocar la Papelera real

#### Scenario: Dobles excluidos del binario
- **WHEN** se compila la app para producción
- **THEN** los dobles de prueba no forman parte del binario

### Requirement: Fijación de ancestros de una ruta
El puerto de sistema de archivos SHALL ofrecer fijar todos los ancestros de una ruta (desde la raíz del volumen hasta el padre) de modo que, mientras la fijación viva, ningún proceso pueda renombrarlos ni borrarlos; la fijación MUST fallar si algún ancestro es punto de reanálisis o no puede abrirse, y MUST NOT impedir mover el elemento objetivo.

#### Scenario: Ancestro fijado no se puede renombrar
- **WHEN** se fijan los ancestros de `<tmp>\a\b\f.txt` y otro código intenta renombrar `<tmp>\a`
- **THEN** el renombre falla mientras la fijación siga vigente

#### Scenario: El objetivo sigue siendo movible
- **WHEN** los ancestros de `<tmp>\a\b\f.txt` están fijados
- **THEN** `f.txt` puede renombrarse o moverse

#### Scenario: Liberación
- **WHEN** la fijación se libera
- **THEN** los ancestros vuelven a poder renombrarse

#### Scenario: Ancestro que es junction
- **WHEN** se intenta fijar los ancestros de `<tmp>\j\f.txt` y `<tmp>\j` es una junction
- **THEN** la fijación falla y no queda ningún ancestro fijado

### Requirement: Identidad de la sesión
El dominio SHALL definir un puerto que informe, de forma independiente y cada una con su propio error tipado, el SID del usuario del token del proceso, si ese token está elevado, el SID del dueño del shell de la sesión del proceso y el SID del usuario de logon de esa sesión; una señal inexistente MUST informarse como ausente y no como error.

#### Scenario: Shell no disponible
- **WHEN** la sesión del proceso no tiene proceso de shell
- **THEN** el puerto informa esa señal como ausente y las demás con su valor

#### Scenario: Fallo de una consulta
- **WHEN** falla la lectura del usuario de logon de la sesión
- **THEN** el puerto devuelve un error tipado solo para esa consulta, sin afectar las demás

### Requirement: Doble de prueba de la identidad de la sesión
El puerto de identidad de la sesión SHALL tener un doble en memoria configurable por respuesta (valor, ausente o error) para cada consulta, excluido del binario de producción.

#### Scenario: Caso over-the-shoulder simulado
- **WHEN** un test configura el doble con token de `admin` elevado y ambas señales de sesión de `ana`
- **THEN** el doble devuelve exactamente esos valores sin consultar el sistema operativo

### Requirement: Consulta de la Papelera de solo lectura
El dominio SHALL declarar un contrato que, para la raíz de un volumen, devuelva la cantidad de elementos y el tamaño total de la Papelera del usuario del proceso; el contrato MUST NOT ofrecer operaciones que listen, restauren, borren o vacíen elementos.

#### Scenario: Consulta del volumen del sistema
- **WHEN** se consulta la Papelera de `C:\`
- **THEN** se obtienen la cantidad de elementos y los bytes totales, ambos mayores o iguales a cero

#### Scenario: Consulta fallida
- **WHEN** la consulta al sistema falla
- **THEN** el contrato devuelve un error tipado y no modifica la Papelera

#### Scenario: Superficie del contrato auditada
- **WHEN** se revisan las operaciones del contrato de consulta de la Papelera
- **THEN** su única operación es la consulta de cantidad y tamaño

### Requirement: Doble de la Papelera para mover
El doble de la Papelera SHALL permitir configurar por ruta cada error del puerto y ejecutar una acción justo antes de mover (para simular un reemplazo en la ventana residual), registrar lo movido y quitarlo del doble de sistema de archivos compartido, sin tocar el disco ni la Papelera reales.

#### Scenario: Error configurado
- **WHEN** un test configura "en uso" para `C:\t\a.tmp` y se intenta moverlo
- **THEN** el doble devuelve "en uso", no registra el movimiento y el archivo sigue en el doble de sistema de archivos

#### Scenario: Reemplazo justo antes de mover
- **WHEN** un test configura que antes de mover `C:\t\a.tmp` se reemplace por un enlace
- **THEN** el doble mueve el enlace y el destino del enlace sigue existiendo

### Requirement: Contratos de persistencia de la limpieza
La persistencia SHALL ofrecer a la limpieza, en contratos separados, la lectura de los hallazgos elegibles de un escaneo por categorías y la fecha de limpieza de un escaneo; la limpieza MUST depender solo de los contratos de escaneos, hallazgos y log, y ninguna de estas operaciones actúa sobre archivos del disco.

#### Scenario: Consumidor de la limpieza
- **WHEN** la limpieza arma un preview y registra resultados
- **THEN** depende solo de los contratos de escaneos, hallazgos y log de limpieza, sin depender de configuración

### Requirement: Política de la Papelera antes de mover
El sistema SHALL ofrecer un puerto de política de la Papelera que, para el volumen de una ruta, informe si la Papelera está disponible (no deshabilitada por `NukeOnDelete` del volumen o global ni por la política `NoRecycleFiles` de máquina o de usuario) y su capacidad máxima; la decisión MUST ser una función pura de los valores crudos. Una lectura que falle por cualquier motivo distinto de "no existe la clave o el valor" MUST tratarse como Papelera no disponible (fail-closed); "no existe" equivale al valor por defecto de Windows (habilitada, capacidad desconocida: no se omite por tamaño). El adaptador de Papelera MUST consultar esta política inmediatamente antes de mover cada elemento y negarse sin invocar al mecanismo del sistema cuando esté deshabilitada (o ilegible) o el tamaño verificado del elemento supere la capacidad, porque el mecanismo del sistema destruye el archivo sin informar error en esos casos. El puerto de Papelera MUST ofrecer además una comprobación previa única por ejecución.

#### Scenario: Papelera deshabilitada por NukeOnDelete
- **WHEN** el valor `NukeOnDelete` del volumen vale 1 y se intenta mover un archivo
- **THEN** el puerto devuelve "Papelera no disponible", no invoca al mecanismo del sistema y el archivo sigue en su lugar

#### Scenario: Papelera deshabilitada por política
- **WHEN** `NoRecycleFiles` vale 1 en la política de máquina o de usuario
- **THEN** la decisión es "no disponible" aunque no haya `NukeOnDelete`

#### Scenario: Política ilegible
- **WHEN** la lectura de la política falla por un error distinto de "no existe" (por ejemplo acceso denegado o tipo inesperado)
- **THEN** el puerto devuelve "Papelera no disponible" y no invoca al mecanismo del sistema

#### Scenario: Clave o valor inexistente
- **WHEN** ninguna clave ni valor de política existe para el volumen
- **THEN** la Papelera se considera habilitada y sin límite de tamaño conocido

#### Scenario: Elemento mayor que la capacidad
- **WHEN** el tamaño verificado del elemento supera la capacidad máxima de la Papelera del volumen
- **THEN** el puerto devuelve "elemento demasiado grande", no invoca al mecanismo del sistema y el archivo sigue en su lugar

#### Scenario: Comprobación previa
- **WHEN** la limpieza comprueba la Papelera antes de mover el primer elemento y la política la declara no disponible
- **THEN** devuelve "Papelera no disponible" sin mover nada

### Requirement: Vaciado de la Papelera en un contrato propio
El dominio SHALL declarar un contrato de vaciado separado de los de mover a la Papelera y de medirla, con una única operación: vaciar la Papelera del usuario del proceso en la raíz del volumen del sistema. La raíz MUST ser un valor que solo representa la raíz de una unidad (nunca vacía ni nula, nunca una carpeta), y el error MUST distinguir acceso denegado, mecanismo inutilizable y otro fallo con código estable.

#### Scenario: Raíz obligatoria
- **WHEN** se revisa la firma del contrato de vaciado
- **THEN** solo acepta la raíz de una unidad; no existe forma de pedir "todas las Papeleras" ni una carpeta

#### Scenario: Contratos existentes sin vaciado
- **WHEN** se revisan los contratos de mover a la Papelera y de medirla
- **THEN** ninguno ofrece vaciar, listar, restaurar ni borrar elementos

#### Scenario: Fallo interno del mecanismo de vaciado
- **WHEN** el mecanismo del sistema no puede inicializarse o falla de forma interna
- **THEN** el contrato devuelve el error "mecanismo inutilizable" y el proceso sigue funcionando

### Requirement: Doble del vaciado de la Papelera
El doble del vaciado SHALL registrar cada raíz pedida, devolver un resultado o error configurable y dejar en el doble de medición compartido el estado posterior configurado (vacía, parcial o sin cambios), sin tocar la Papelera real; MUST excluirse del binario de producción como los demás dobles.

#### Scenario: Vaciado parcial simulado
- **WHEN** un test configura el doble para dejar 2 elementos de 40 MB tras vaciar una Papelera de 12 elementos
- **THEN** después de vaciar, la medición del doble devuelve 2 elementos y 40 MB, y el doble registra una sola llamada con la raíz pedida

#### Scenario: Sin llamadas
- **WHEN** un flujo rechaza el vaciado antes de ejecutarlo
- **THEN** el doble no registra ninguna llamada

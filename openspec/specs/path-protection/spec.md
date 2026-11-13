# path-protection Specification

## Purpose
Definir la decisión pura de si una ruta puede tocarse (protegida o elegible para borrado): normalización léxica de rutas Windows, reglas de protección con lista cerrada de excepciones, reparse points y contenido en la nube no elegibles, y re-verificación anti-TOCTOU antes de cada borrado. Es la única puerta de protección que usan los flujos de limpieza, Papelera y reglas personalizadas.

## Requirements

### Requirement: Decisión con motivo para cada ruta
El sistema SHALL responder cada consulta de protección con exactamente una decisión: `allowed` (con la ruta real resuelta, su tipo, tamaño e identidad de archivo), `skipped_cloud`, o `protected` con un motivo de una lista cerrada y estable. Cada motivo MUST tener un código textual estable apto para el log y el catálogo de la UI.

#### Scenario: Archivo temporal elegible
- **WHEN** se consulta `C:\Users\ana\AppData\Local\Temp\a.tmp`, un archivo común existente
- **THEN** la decisión es `allowed` e incluye su ruta real, tamaño e identidad de archivo

#### Scenario: Ruta protegida con motivo
- **WHEN** se consulta `C:\Windows\System32\kernel32.dll`
- **THEN** la decisión es `protected` con el motivo `windows_dir`

#### Scenario: Correspondencia con los resultados del log
- **WHEN** un consumidor traduce una decisión a un resultado de limpieza
- **THEN** `protected` corresponde a `skipped_protected`, `skipped_cloud` a `skipped_cloud` y `allowed` no produce resultado de omisión

### Requirement: Fail-closed ante cualquier duda
Toda falla al interpretar la ruta, resolverla, leer sus metadatos, obtener su identidad o construir el contexto SHALL producir una decisión `protected`; MUST NOT existir camino de error que termine en `allowed`.

#### Scenario: Metadatos ilegibles
- **WHEN** leer los metadatos de un ancestro o del objetivo falla con acceso denegado
- **THEN** la decisión es `protected` con un motivo de resolución que identifica el tipo de falla

#### Scenario: Ruta inexistente
- **WHEN** se consulta una ruta que no existe
- **THEN** la decisión es `protected` con el motivo de resolución "no encontrada"

#### Scenario: Identidad de archivo no disponible
- **WHEN** el sistema de archivos no informa la identidad del objetivo
- **THEN** la decisión es `protected`

### Requirement: Normalización de la forma de la ruta
Antes de evaluar reglas, el sistema SHALL normalizar la ruta: quitar el prefijo `\\?\` de rutas de unidad, aceptar `/` como separador fuera de la forma `\\?\`, colapsar separadores repetidos, descartar segmentos `.` y resolver segmentos `..`. Un `..` que sube por encima de la raíz del volumen MUST rechazarse.

#### Scenario: Segmentos relativos que escapan hacia Windows
- **WHEN** se consulta `C:\Users\ana\AppData\Local\Temp\..\..\..\..\..\Windows\System32\x.dll`
- **THEN** la decisión es `protected` con el motivo `windows_dir`

#### Scenario: Prefijo verbatim
- **WHEN** se consulta `\\?\C:\Windows\Temp\x.tmp` y `C:\Windows\Temp\x.tmp` es un archivo común
- **THEN** la decisión es la misma que para `C:\Windows\Temp\x.tmp`

#### Scenario: Subida por encima de la raíz
- **WHEN** se consulta `C:\..\Windows\x.dll`
- **THEN** la decisión es `protected` con un motivo de ruta no interpretable

#### Scenario: Forma verbatim con segmentos relativos
- **WHEN** se consulta `\\?\C:\Users\ana\..\bob\x.txt`
- **THEN** la decisión es `protected` con un motivo de ruta no interpretable

### Requirement: Rechazo de formas de ruta no locales o ambiguas
El sistema SHALL tratar como `protected` (motivo de ruta no interpretable) toda ruta: relativa o relativa a unidad, UNC o `\\?\UNC\`, de dispositivo (`\\.\`, `\??\`, `\\?\Volume{…}`), con flujo de datos alternativo (`:` fuera de la unidad), con comodines o caracteres inválidos, con un componente que es nombre de dispositivo reservado, con un componente terminado en punto o espacio, o que no es Unicode válido.

#### Scenario: Flujo de datos alternativo
- **WHEN** se consulta `C:\Users\ana\AppData\Local\Temp\a.txt:oculto`
- **THEN** la decisión es `protected` con el motivo `alternate_data_stream`

#### Scenario: Ruta de red
- **WHEN** se consulta `\\servidor\compartido\a.tmp` o `\\?\UNC\servidor\compartido\a.tmp`
- **THEN** la decisión es `protected` con el motivo `network_path`

#### Scenario: Ruta de dispositivo
- **WHEN** se consulta `\\.\C:\Users\ana\a.tmp`
- **THEN** la decisión es `protected` con el motivo `device_path`

#### Scenario: Punto final que Windows recortaría
- **WHEN** se consulta `C:\Windows.\System32\x.dll` o `C:\Users\ana\AppData\Local\Temp\a.tmp `
- **THEN** la decisión es `protected` con el motivo `trailing_dot_or_space`

#### Scenario: Nombre de dispositivo reservado
- **WHEN** se consulta `C:\Users\ana\AppData\Local\Temp\nul.txt`
- **THEN** la decisión es `protected` con el motivo `reserved_device_name`

#### Scenario: Ruta relativa
- **WHEN** se consulta `Temp\a.tmp` o `C:Temp\a.tmp`
- **THEN** la decisión es `protected` con el motivo `not_absolute`

### Requirement: Comparación de nombres sin distinguir mayúsculas como NTFS
La pertenencia de una ruta a una raíz, excepción o ancla SHALL compararse sin distinguir mayúsculas con un plegado simple uno a uno por carácter del plano básico, equivalente al de NTFS; MUST NOT usar plegado completo (que expande caracteres) ni normalización Unicode.

#### Scenario: Mayúsculas mezcladas
- **WHEN** se consulta `c:\wInDoWs\sYsTeM32\x.dll`
- **THEN** la decisión es `protected` con el motivo `windows_dir`

#### Scenario: Mayúsculas no ASCII en el nombre de usuario
- **WHEN** el usuario actual es `Jürgen` y se consulta `C:\Users\JÜRGEN\AppData\Local\Temp\a.tmp`
- **THEN** la ruta se reconoce dentro del perfil del usuario actual

#### Scenario: Sin equivalencias por expansión
- **WHEN** se compara un componente `STRASSE` con un ancla `Straße`
- **THEN** no se consideran el mismo nombre

### Requirement: Nombres cortos 8.3 resueltos por la ruta real
Las reglas SHALL evaluarse también sobre la ruta real con nombres largos obtenida del sistema de archivos; un componente con forma de nombre corto que no pueda resolverse MUST dar `protected`.

#### Scenario: Nombre corto de Program Files
- **WHEN** se consulta `C:\PROGRA~1\App\a.log` y el sistema de archivos la resuelve a `C:\Program Files\App\a.log`
- **THEN** la decisión es `protected` con el motivo `program_files`

#### Scenario: Nombre corto del perfil
- **WHEN** se consulta `C:\Users\ANAMAR~1\AppData\Local\Temp\a.tmp` y su ruta real es `C:\Users\ana maria\AppData\Local\Temp\a.tmp`, perfil del usuario actual
- **THEN** la decisión es `allowed` con la ruta real de nombres largos

### Requirement: Contexto de protección inyectado
Las ubicaciones del sistema (unidad y carpeta de Windows, Program Files, ProgramData, raíz de perfiles, carpetas del usuario actual, directorios de la app) SHALL provenir de un contexto resuelto por el sistema operativo e inyectado a la evaluación; las reglas MUST NOT contener letras de unidad ni rutas absolutas fijas.

#### Scenario: Windows instalado en otra unidad
- **WHEN** el contexto indica Windows en `D:\Windows` y perfiles en `D:\Users`
- **THEN** `D:\Windows\System32\x.dll` es `protected` con motivo `windows_dir` y `D:\Windows\Temp\a.tmp` es elegible

#### Scenario: Usuario actual del contexto
- **WHEN** el contexto designa a `ana` como usuario actual
- **THEN** solo el perfil de `ana` puede contener rutas elegibles dentro de la raíz de perfiles

### Requirement: Contexto inválido o incompleto protege todo
Si una ubicación obligatoria del contexto (carpeta de Windows, Program Files, ProgramData, raíz de perfiles) no puede resolverse o normalizarse, o alguna ubicación es una raíz de volumen, el sistema SHALL decidir `protected` con el motivo `context_unavailable` para toda ruta.

#### Scenario: ProgramData no resoluble
- **WHEN** el contexto no logra resolver ProgramData
- **THEN** cualquier ruta consultada, incluida una de `%TEMP%`, es `protected` con motivo `context_unavailable`

#### Scenario: Ubicación que es raíz de volumen
- **WHEN** el contexto informa ProgramData como `C:\`
- **THEN** cualquier ruta consultada es `protected` con motivo `context_unavailable`

#### Scenario: Usuario actual desconocido
- **WHEN** el contexto no puede determinar las carpetas del usuario actual
- **THEN** toda ruta dentro de la raíz de perfiles es `protected` con motivo `other_user_profile`

### Requirement: Raíces de volumen y entradas de sistema de la raíz protegidas
La raíz de cualquier volumen y, en la raíz de cualquier volumen, las entradas cuyo nombre empieza con `$` y las de una lista cerrada (`System Volume Information`, `pagefile.sys`, `hiberfil.sys`, `swapfile.sys`, `DumpStack.log`, `DumpStack.log.tmp`, `Recovery`, `Boot`, `bootmgr`, `BOOTNXT`, `Config.Msi`, `Documents and Settings`) SHALL ser `protected` junto con todo su contenido, sin excepciones.

#### Scenario: Raíz del volumen
- **WHEN** se consulta `C:\`
- **THEN** la decisión es `protected` con el motivo `volume_root`

#### Scenario: Papelera de reciclaje
- **WHEN** se consulta `C:\$Recycle.Bin\S-1-5-21-1\$R1.tmp`
- **THEN** la decisión es `protected` con el motivo `volume_system_entry`

#### Scenario: Archivo de paginación
- **WHEN** se consulta `C:\pagefile.sys` o `C:\System Volume Information\x`
- **THEN** la decisión es `protected` con el motivo `volume_system_entry`

### Requirement: Anclas del sistema y del usuario no eliminables
Las ubicaciones del contexto en sí (raíz de perfiles, perfil actual, AppData local y roaming, carpetas conocidas del usuario, carpeta temporal, carpeta de Windows, Program Files, ProgramData, directorios de la app) y las raíces de cada excepción SHALL ser `protected` con motivo `anchor`, aunque su contenido pueda ser elegible.

#### Scenario: Carpeta temporal en sí
- **WHEN** se consulta `C:\Users\ana\AppData\Local\Temp`
- **THEN** la decisión es `protected` con el motivo `anchor`

#### Scenario: Raíz de una excepción en sí
- **WHEN** se consulta `C:\Windows\Temp`
- **THEN** la decisión es `protected` con el motivo `anchor`, aunque `C:\Windows\Temp\a.tmp` sea elegible

#### Scenario: Carpeta Descargas en sí
- **WHEN** se consulta `C:\Users\ana\Downloads`
- **THEN** la decisión es `protected` con el motivo `anchor`

### Requirement: Perfiles de otros usuarios protegidos
Toda ruta dentro de la raíz de perfiles que no esté en el perfil del usuario actual (otros usuarios, `Public`, `Default`, `All Users`) SHALL ser `protected` con motivo `other_user_profile`, sin excepciones.

#### Scenario: Temporales de otro usuario
- **WHEN** se consulta `C:\Users\bob\AppData\Local\Temp\a.tmp`
- **THEN** la decisión es `protected` con el motivo `other_user_profile`

#### Scenario: Perfil público
- **WHEN** se consulta `C:\Users\Public\Documents\a.txt`
- **THEN** la decisión es `protected` con el motivo `other_user_profile`

#### Scenario: Nombre de usuario con espacios y Unicode
- **WHEN** el usuario actual es `José María` y se consulta `C:\Users\José María\AppData\Local\Temp\a.tmp`
- **THEN** la decisión es `allowed`, y la misma ruta bajo `C:\Users\José\` es `protected` con motivo `other_user_profile`

### Requirement: Solo volúmenes soportados
Mientras PA-12 siga abierta, solo el volumen del sistema SHALL ser soportado; toda ruta de otro volumen MUST ser `protected` con motivo `unsupported_volume`. La lista de volúmenes soportados MUST ser parte del contexto inyectado.

#### Scenario: Archivo en otro disco
- **WHEN** el volumen del sistema es `C:` y se consulta `D:\Temp\a.tmp`
- **THEN** la decisión es `protected` con el motivo `unsupported_volume`

### Requirement: Raíces protegidas
SHALL ser `protected` todo el contenido de: carpeta de Windows (`windows_dir`), cada carpeta Program Files (`program_files`), ProgramData (`program_data`), perfiles de Chrome, Edge, Brave y Firefox del usuario actual (`browser_profile`) y directorios de datos e instalación de la app (`app_dir`), salvo lo cubierto por una excepción de la lista cerrada.

#### Scenario: Program Files (x86)
- **WHEN** se consulta `C:\Program Files (x86)\App\cache\a.tmp`
- **THEN** la decisión es `protected` con el motivo `program_files`

#### Scenario: Cookies del navegador
- **WHEN** se consulta `C:\Users\ana\AppData\Local\Google\Chrome\User Data\Default\Network\Cookies`
- **THEN** la decisión es `protected` con el motivo `browser_profile`

#### Scenario: Base de datos de la app
- **WHEN** se consulta `C:\Users\ana\AppData\Local\pc-optimizer\pc-optimizer.db`
- **THEN** la decisión es `protected` con el motivo `app_dir`

#### Scenario: Ruta fuera de toda raíz protegida
- **WHEN** se consulta `C:\Users\ana\Downloads\setup-viejo.exe`
- **THEN** la decisión es `allowed`

### Requirement: Lista cerrada de excepciones
Las únicas excepciones SHALL ser, como subárboles estrictos: `<Windows>\Temp`, `<Windows>\SoftwareDistribution\Download`, `<Windows>\Minidump`, `<ProgramData>\Microsoft\Windows\WER\ReportArchive`, `<ProgramData>\Microsoft\Windows\WER\ReportQueue`, `<perfil Chromium>\<un perfil>\{Cache, Code Cache, GPUCache}` (Chrome, Edge, Brave) y `<LocalAppData>\Mozilla\Firefox\Profiles\<un perfil>\cache2`. Agregar o ampliar una MUST requerir un change aprobado sobre esta capacidad.

#### Scenario: Temporales de Windows
- **WHEN** se consulta `C:\Windows\Temp\x.tmp`
- **THEN** la decisión es `allowed`

#### Scenario: Caché de Windows Update
- **WHEN** se consulta `C:\Windows\SoftwareDistribution\Download\abc\x.cab`
- **THEN** la decisión es `allowed`, mientras `C:\Windows\SoftwareDistribution\DataStore\x.edb` es `protected` con motivo `windows_dir`

#### Scenario: Caché de Chrome en cualquier perfil
- **WHEN** se consulta `C:\Users\ana\AppData\Local\Google\Chrome\User Data\Profile 2\Cache\Cache_Data\f_000001`
- **THEN** la decisión es `allowed`

#### Scenario: Comodín de un solo componente
- **WHEN** se consulta `C:\Users\ana\AppData\Local\Google\Chrome\User Data\Cache\x`
- **THEN** la decisión es `protected` con motivo `browser_profile`, porque el comodín exige exactamente un componente de perfil

#### Scenario: Caché de Firefox
- **WHEN** se consulta `C:\Users\ana\AppData\Local\Mozilla\Firefox\Profiles\ab12.default\cache2\entries\X`
- **THEN** la decisión es `allowed`, mientras `C:\Users\ana\AppData\Roaming\Mozilla\Firefox\Profiles\ab12.default\logins.json` es `protected` con motivo `browser_profile`

### Requirement: Precedencia de reglas
La evaluación SHALL seguir este orden: (1) ruta no interpretable o contexto inválido; (2) denegaciones duras —volumen no soportado, raíz de volumen, entrada de sistema de la raíz, ancla, perfil de otro usuario—; (3) excepción de la lista cerrada; (4) raíz protegida; (5) elegible. Una excepción MUST NOT anular una denegación dura y solo aplica a descendientes estrictos de su raíz.

#### Scenario: Excepción no anula perfil ajeno
- **WHEN** se consulta `C:\Users\bob\AppData\Local\Google\Chrome\User Data\Default\Cache\x`
- **THEN** la decisión es `protected` con motivo `other_user_profile`

#### Scenario: Primera regla protegida que coincide
- **WHEN** un directorio de la app está dentro de Program Files y se consulta un archivo dentro de él
- **THEN** la decisión es `protected` y el motivo es el de la primera raíz en el orden documentado

### Requirement: Reparse points nunca atravesados
Si cualquier ancestro de la ruta (desde la raíz del volumen, excluida, hasta el padre) es un punto de reanálisis (junction, symlink, punto de montaje u otro), la decisión SHALL ser `protected` con motivo `reparse_point_in_path`, o `skipped_cloud` si ese ancestro es de nube.

#### Scenario: Junction en TEMP hacia Windows
- **WHEN** `C:\Users\ana\AppData\Local\Temp\j` es una junction hacia `C:\Windows` y se consulta `C:\Users\ana\AppData\Local\Temp\j\System32\x.dll`
- **THEN** la decisión es `protected` con motivo `reparse_point_in_path` y nada de `C:\Windows` es elegible por esa vía

#### Scenario: Junction hacia una ruta que sería elegible
- **WHEN** `C:\Users\ana\AppData\Local\Temp\j` es una junction hacia `C:\Users\ana\Downloads` y se consulta `C:\Users\ana\AppData\Local\Temp\j\a.exe`
- **THEN** la decisión es `protected` con motivo `reparse_point_in_path`

### Requirement: Un reparse point como objetivo no es elegible
Cuando la ruta consultada es en sí un punto de reanálisis, la decisión SHALL ser `skipped_cloud` si es de nube y `protected` con motivo `reparse_point` en cualquier otro caso; ni el enlace ni su destino son elegibles en v0.x.

#### Scenario: Symlink dentro de TEMP
- **WHEN** se consulta `C:\Users\ana\AppData\Local\Temp\link.dll`, un symlink a `C:\Windows\System32\x.dll`
- **THEN** la decisión es `protected` con motivo `reparse_point`

#### Scenario: Junction dentro de TEMP
- **WHEN** se consulta `C:\Users\ana\AppData\Local\Temp\j`, una junction a una carpeta vacía de TEMP
- **THEN** la decisión es `protected` con motivo `reparse_point`

### Requirement: Ruta léxica y ruta real deben coincidir
La ruta real devuelta por el sistema de archivos SHALL coincidir con la ruta normalizada consultada en unidad, cantidad de componentes y cada componente (sin distinguir mayúsculas), salvo componentes con forma de nombre corto 8.3; cualquier otra diferencia MUST dar `protected` con motivo `resolved_path_mismatch`. Las reglas MUST evaluarse sobre ambas y basta que una proteja.

#### Scenario: Redirección no detectada por metadatos
- **WHEN** los metadatos de los ancestros no informan reparse points pero la ruta real de `C:\Users\ana\AppData\Local\Temp\x\a.tmp` es `C:\Windows\a.tmp`
- **THEN** la decisión es `protected`

#### Scenario: Unidad sustituida
- **WHEN** `X:` es una unidad `subst` de `C:\Windows` incluida en el contexto como soportada y se consulta `X:\System32\x.dll`
- **THEN** la decisión es `protected`

### Requirement: Contenido en la nube omitido
Un objetivo con atributos de marcador de nube (`RECALL_ON_DATA_ACCESS`, `RECALL_ON_OPEN`, `OFFLINE`) o con un tag de reanálisis de proveedor de nube, incluso hidratado, SHALL decidirse `skipped_cloud` cuando de otro modo sería elegible; si además está protegido, prevalece `protected`.

#### Scenario: Archivo de OneDrive solo en la nube
- **WHEN** se consulta `C:\Users\ana\OneDrive\video.mp4`, marcador sin datos locales
- **THEN** la decisión es `skipped_cloud`

#### Scenario: Marcador en ruta protegida
- **WHEN** se consulta un marcador de nube dentro de `C:\Users\bob\OneDrive`
- **THEN** la decisión es `protected` con motivo `other_user_profile`

### Requirement: Archivos con varios hard links no elegibles
Un archivo con más de un nombre (hard links) SHALL ser `protected` con motivo `multiple_hard_links`, porque moverlo a la Papelera no libera espacio y su otro nombre puede estar en una ruta protegida.

#### Scenario: Hard link en TEMP a un archivo de sistema
- **WHEN** se consulta `C:\Users\ana\AppData\Local\Temp\h.dll`, con 2 hard links
- **THEN** la decisión es `protected` con motivo `multiple_hard_links`

### Requirement: La decisión sobre un directorio no cubre su contenido
Una decisión `allowed` sobre un directorio SHALL certificar solo esa entrada; MUST NOT interpretarse como elegibilidad de sus descendientes, y toda operación que afecte descendientes MUST obtener una decisión por cada uno.

#### Scenario: Directorio elegible con contenido protegido
- **WHEN** `C:\Users\ana\AppData\Local\Temp\d` es `allowed` y contiene una junction `j`
- **THEN** la consulta de `C:\Users\ana\AppData\Local\Temp\d\j` sigue siendo `protected` con motivo `reparse_point`

### Requirement: Re-verificación anti-TOCTOU antes de borrar
Antes de mover un elemento a la Papelera, el sistema SHALL fijar los ancestros de su ruta real contra renombres y borrados, repetir la verificación completa y comparar la identidad de archivo (volumen e id) y el tipo con los del escaneo; cualquier diferencia o falla MUST dar `protected` y la fijación MUST mantenerse hasta terminar el movimiento.

#### Scenario: Archivo reemplazado por un symlink
- **WHEN** entre el escaneo y la ejecución `a.tmp` se reemplaza por un symlink hacia `C:\Windows\System32\x.dll`
- **THEN** la re-verificación da `protected` y el elemento no se mueve

#### Scenario: Archivo reemplazado por otro con el mismo nombre
- **WHEN** entre el escaneo y la ejecución `a.tmp` se borra y se crea otro `a.tmp`
- **THEN** la re-verificación da `protected` con motivo `identity_changed`

#### Scenario: Carpeta ancestro reemplazada por una junction
- **WHEN** entre el escaneo y la ejecución la carpeta padre se reemplaza por una junction hacia `C:\Windows`
- **THEN** la re-verificación da `protected`

#### Scenario: Ancestro fijado durante el movimiento
- **WHEN** la re-verificación dio `allowed` y la fijación sigue vigente
- **THEN** ningún otro proceso puede renombrar ni borrar los ancestros fijados hasta que se libera

#### Scenario: Ruta a mover
- **WHEN** la re-verificación da `allowed`
- **THEN** la ruta que se entrega para mover a la Papelera es la ruta real verificada y no la ruta original recibida

### Requirement: Validación de raíces de reglas personalizadas
El sistema SHALL rechazar como raíz de una regla de limpieza toda ruta que sea `protected` por un motivo distinto de ancla de contenido (carpetas conocidas del usuario, temporal, raíz de excepción) o que sea ancestro de una raíz protegida, de una raíz de excepción, del perfil actual o de un ancla estructural.

#### Scenario: Reglas sobre ubicaciones críticas
- **WHEN** se valida una regla con raíz `C:\`, `C:\Users`, `C:\Windows`, `C:\Users\ana` o `C:\Users\ana\AppData\Local`
- **THEN** la regla se rechaza con el motivo correspondiente

#### Scenario: Regla sobre Descargas
- **WHEN** se valida una regla con raíz `C:\Users\ana\Downloads`
- **THEN** la raíz se acepta y cada elemento que la regla encuentre se decide por separado

#### Scenario: Regla sobre otro usuario
- **WHEN** se valida una regla con raíz `C:\Users\bob\Downloads`
- **THEN** la regla se rechaza con motivo `other_user_profile`

### Requirement: Protección no configurable por el usuario
Las raíces protegidas, las denegaciones duras y la lista de excepciones SHALL definirse en código de dominio con tests de tabla; ningún ajuste, regla personalizada ni categoría de limpieza MUST poder ampliarlas, reducirlas ni desactivarlas en tiempo de ejecución.

#### Scenario: Categoría que intenta limpiar fuera de la lista
- **WHEN** una categoría de limpieza produce un candidato en `C:\Windows\Logs\CBS\CBS.log`
- **THEN** la decisión es `protected` con motivo `windows_dir`, porque esa ubicación no está en la lista cerrada de excepciones

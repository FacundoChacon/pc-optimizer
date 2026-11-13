# Spec Delta

## Purpose

Detectar y medir, sin modificar nada, la basura de las categorías de la whitelist de limpieza (KB 11), verificando cada candidato contra la protección de rutas y respetando el bloqueo por-usuario del contexto de privilegios, para que la limpieza posterior opere solo sobre hallazgos verificados.

## ADDED Requirements

### Requirement: Whitelist cerrada de categorías
La detección SHALL producir hallazgos solo de las categorías `temp_user`, `temp_windows`, `browser_cache_chrome`, `browser_cache_edge`, `browser_cache_firefox`, `browser_cache_brave`, `old_logs`, `wu_cache`, `thumbnail_cache`, `crash_dumps` y `recycle_bin`; la persistencia MUST rechazar cualquier otra categoría. Ningún ajuste, privilegio ni dato de entrada MUST poder agregar categorías en tiempo de ejecución.

#### Scenario: Categoría fuera de la whitelist
- **WHEN** se intenta guardar un hallazgo con categoría `downloads` o `program_files`
- **THEN** la escritura se rechaza y el escaneo no registra ese hallazgo

#### Scenario: Resumen con todas las categorías
- **WHEN** termina un escaneo de basura
- **THEN** su resumen contiene exactamente una entrada por cada categoría de la whitelist, incluidas las que no tienen hallazgos

#### Scenario: Elevación sin efecto sobre la whitelist
- **WHEN** la app corre elevada como administrador
- **THEN** el conjunto de categorías y de ubicaciones recorridas es el mismo que sin elevación

### Requirement: Ubicaciones cerradas por categoría
Cada categoría SHALL recorrer solo sus ubicaciones de la lista cerrada, derivadas de las carpetas del sistema y del usuario que informa el sistema operativo, nunca de rutas fijas ni de datos del usuario: `temp_user` → temporal del usuario; `temp_windows` → `<Windows>\Temp`; `wu_cache` → `<Windows>\SoftwareDistribution\Download`; las demás según los escenarios. Una ubicación que no puede resolverse MUST omitirse e informarse.

#### Scenario: Cachés de navegadores Chromium
- **WHEN** existen `<LocalAppData>\Google\Chrome\User Data\Default\Cache` y `...\User Data\Profile 2\Code Cache`
- **THEN** `browser_cache_chrome` recorre `Cache`, `Code Cache` y `GPUCache` de cada perfil (un solo nivel bajo `User Data`), y lo mismo hacen `browser_cache_edge` con `Microsoft\Edge\User Data` y `browser_cache_brave` con `BraveSoftware\Brave-Browser\User Data`

#### Scenario: Caché de Firefox
- **WHEN** existe `<LocalAppData>\Mozilla\Firefox\Profiles\ab12.default\cache2`
- **THEN** `browser_cache_firefox` recorre solo ese `cache2` y nada de `<RoamingAppData>\Mozilla\Firefox\Profiles`

#### Scenario: Datos del perfil del navegador nunca recorridos
- **WHEN** el perfil de Chrome contiene `Network\Cookies`, `Login Data`, `History` y `Extensions`
- **THEN** ninguno de esos elementos aparece como hallazgo ni como omitido de `browser_cache_chrome`

#### Scenario: Miniaturas del Explorador
- **WHEN** `<LocalAppData>\Microsoft\Windows\Explorer` contiene `thumbcache_256.db`, `thumbcache_idx.db` e `iconcache_32.db`
- **THEN** `thumbnail_cache` considera solo los `thumbcache_*.db` de esa carpeta, sin descender a subcarpetas, y no considera `iconcache_32.db`

#### Scenario: Volcados de fallos
- **WHEN** existen archivos en `<LocalAppData>\CrashDumps`, `<Windows>\Minidump`, `<ProgramData>\Microsoft\Windows\WER\ReportArchive` y `...\WER\ReportQueue`
- **THEN** `crash_dumps` recorre esas cuatro ubicaciones y ninguna otra (no `<Windows>\MEMORY.DMP` ni `<Windows>\LiveKernelReports`)

#### Scenario: Navegador no instalado
- **WHEN** no existe `<LocalAppData>\BraveSoftware\Brave-Browser\User Data`
- **THEN** `browser_cache_brave` termina sin hallazgos y sin error

#### Scenario: Temporal del usuario no resoluble
- **WHEN** el sistema no informa la carpeta temporal del usuario
- **THEN** `temp_user` queda con estado `unavailable` y el resto de las categorías se escanea normalmente

### Requirement: Logs viejos sin ubicaciones en esta versión
La categoría `old_logs` SHALL existir en la whitelist con antigüedad mínima de 30 días por fecha de última escritura, pero en esta versión su lista de ubicaciones MUST estar vacía hasta que el dueño apruebe ubicaciones concretas; nunca MUST recorrer `<Windows>\Logs` ni otra ruta protegida.

#### Scenario: Escaneo con old_logs
- **WHEN** termina un escaneo de basura
- **THEN** `old_logs` informa estado `unavailable`, cero elementos y cero bytes

#### Scenario: Log de Windows protegido
- **WHEN** existe `C:\Windows\Logs\CBS\CBS.log` con 90 días de antigüedad
- **THEN** no aparece como hallazgo de ninguna categoría

### Requirement: Solo archivos como hallazgos
Un hallazgo SHALL ser siempre un archivo individual dentro de una ubicación de su categoría; las carpetas se recorren pero MUST NOT registrarse como hallazgo, y la raíz de una ubicación MUST NOT ser hallazgo ni omitido.

#### Scenario: Carpeta con archivos en TEMP
- **WHEN** el temporal del usuario contiene la carpeta `instalador\` con `a.msi` (10 MB) y `b.log` (1 KB), ambos elegibles y con más de 24 h
- **THEN** los hallazgos son `instalador\a.msi` y `instalador\b.log` y la carpeta `instalador` no figura como hallazgo

#### Scenario: Raíz de la ubicación
- **WHEN** se escanea `temp_windows`
- **THEN** `C:\Windows\Temp` en sí no figura entre los hallazgos ni entre los omitidos

### Requirement: Recorrido sin seguir puntos de reanálisis
El recorrido SHALL listar el contenido de las carpetas sin seguir junctions, symlinks, puntos de montaje ni otros puntos de reanálisis; un punto de reanálisis encontrado MUST registrarse como omitido con motivo `reparse_point` y su destino MUST NOT recorrerse. Si la raíz de una ubicación es un punto de reanálisis, la ubicación no se recorre y se informa como inaccesible.

#### Scenario: Junction en TEMP hacia Windows
- **WHEN** el temporal del usuario contiene `j`, una junction hacia `C:\Windows`
- **THEN** `j` figura como omitido con motivo `reparse_point` y ningún archivo de `C:\Windows` aparece en el escaneo

#### Scenario: Symlink a un archivo de sistema
- **WHEN** el temporal del usuario contiene `link.dll`, un symlink a `C:\Windows\System32\x.dll`
- **THEN** `link.dll` figura como omitido con motivo `reparse_point` y su tamaño no suma

#### Scenario: Ubicación que es una junction
- **WHEN** `<LocalAppData>\CrashDumps` es una junction hacia otra carpeta
- **THEN** esa ubicación no se recorre y cuenta como inaccesible en el resumen de `crash_dumps`

### Requirement: Verificación de protección de cada candidato
Cada archivo candidato SHALL decidirse con la verificación de protección de rutas antes de contarse: `allowed` lo hace elegible con su ruta real, tamaño e identidad; `protected` y `skipped_cloud` lo registran como omitido con su código de motivo estable. Un omitido MUST NOT sumar al tamaño ni a la cantidad elegibles.

#### Scenario: Archivo elegible
- **WHEN** `C:\Windows\Temp\x.tmp` (2 MB, 3 días) se verifica como `allowed`
- **THEN** se registra como hallazgo elegible de `temp_windows` con 2 MB, su ruta real y su identidad de archivo

#### Scenario: Archivo con varios hard links
- **WHEN** `C:\Windows\Temp\h.dll` tiene 2 hard links
- **THEN** se registra como omitido con motivo `multiple_hard_links` y no suma al tamaño de `temp_windows`

#### Scenario: Marcador de nube
- **WHEN** un archivo candidato es un marcador de OneDrive
- **THEN** se registra como omitido con motivo `skipped_cloud`

#### Scenario: Error al verificar
- **WHEN** leer los metadatos de un candidato falla con acceso denegado
- **THEN** se registra como omitido con el motivo de resolución correspondiente y el escaneo continúa

### Requirement: Antigüedad mínima de los temporales
Los archivos de `temp_user` y `temp_windows` SHALL considerarse candidatos solo si su fecha de última escritura tiene al menos 24 horas; los más recientes o sin fecha MUST ignorarse sin registrarse como hallazgo ni como omitido.

#### Scenario: Temporal reciente
- **WHEN** el temporal del usuario contiene `nuevo.tmp` modificado hace 2 horas y `viejo.tmp` modificado hace 3 días
- **THEN** solo `viejo.tmp` es candidato

#### Scenario: Temporal sin fecha
- **WHEN** el sistema no informa la fecha de última escritura de `x.tmp` en `C:\Windows\Temp`
- **THEN** `x.tmp` no es candidato

### Requirement: Detección de solo lectura
La detección SHALL limitarse a listar carpetas y leer metadatos y tamaños; MUST NOT borrar, mover, renombrar, truncar, abrir para escritura ni cambiar atributos de ningún archivo o carpeta, ni vaciar la Papelera.

#### Scenario: Disco intacto tras escanear
- **WHEN** se escanea con un doble de sistema de archivos que registra cada operación
- **THEN** las únicas operaciones registradas son de lectura (listado, metadatos, canonicalización) y ninguna de fijación ni de Papelera

### Requirement: Alcance por-usuario de cada ubicación
Cada ubicación SHALL declarar alcance por-usuario si está en el perfil del usuario actual (temporal del usuario, cachés de navegador, miniaturas, `<LocalAppData>\CrashDumps`, Papelera) y de máquina en otro caso (`<Windows>\Temp`, `<Windows>\SoftwareDistribution\Download`, `<Windows>\Minidump`, WER de `<ProgramData>`).

#### Scenario: Categoría con alcance mixto
- **WHEN** se consultan las ubicaciones de `crash_dumps`
- **THEN** `<LocalAppData>\CrashDumps` es por-usuario y las otras tres son de máquina

### Requirement: Bloqueo por-usuario en mismatch
Con contexto de privilegios `mismatch`, las ubicaciones por-usuario MUST NOT recorrerse ni consultarse; una categoría con todas sus ubicaciones bloqueadas SHALL informar `blocked` con el motivo (`different_user` o `undetermined`) y una con parte bloqueada SHALL informar `partial` con ese motivo. Las de máquina se escanean igual que en `match`.

#### Scenario: Over-the-shoulder
- **WHEN** el contexto es `mismatch/different_user` y se escanea
- **THEN** `temp_user`, las cuatro `browser_cache_*`, `thumbnail_cache` y `recycle_bin` informan `blocked` con motivo `different_user`, sin hallazgos y sin ninguna operación sobre el perfil, y `temp_windows` y `wu_cache` se escanean

#### Scenario: Volcados en mismatch
- **WHEN** el contexto es `mismatch/undetermined` y se escanea
- **THEN** `crash_dumps` informa `partial` con motivo `undetermined` e incluye solo hallazgos de `<Windows>\Minidump` y del WER de `<ProgramData>`

#### Scenario: Coincidencia
- **WHEN** el contexto es `match`
- **THEN** ninguna categoría informa `blocked` ni `partial` por privilegios

### Requirement: Contexto de protección restringido y fail-closed
La verificación de protección de un escaneo SHALL usar un contexto restringido por el contexto de privilegios (en `mismatch`, sin usuario actual); si el contexto de protección no puede construirse, todas las categorías salvo `recycle_bin` MUST informar `unavailable` sin hallazgos.

#### Scenario: Ruta del perfil en mismatch
- **WHEN** el contexto es `mismatch` y una ubicación de máquina contiene una junction que la verificación resolvería dentro del perfil del administrador
- **THEN** cualquier ruta del perfil queda `protected` con motivo `other_user_profile` o `reparse_point` y nunca es elegible

#### Scenario: Contexto no resoluble
- **WHEN** el sistema no informa la carpeta de Windows
- **THEN** el escaneo termina con todas las categorías de archivos en `unavailable`, cero hallazgos elegibles y ninguna excepción

### Requirement: Papelera solo medida
La categoría `recycle_bin` SHALL informar únicamente la cantidad de elementos y el tamaño total de la Papelera del usuario actual en el volumen del sistema; MUST NOT registrar hallazgos individuales, listar su contenido, modificarla ni vaciarla. Un fallo de la consulta deja la categoría `unavailable`.

#### Scenario: Papelera con contenido
- **WHEN** la Papelera del volumen del sistema tiene 12 elementos y 340 MB
- **THEN** `recycle_bin` informa `done` con 12 elementos y 340 MB y no hay hallazgos de esa categoría

#### Scenario: Consulta fallida
- **WHEN** la consulta de la Papelera falla
- **THEN** `recycle_bin` informa `unavailable` y el resto del escaneo termina normalmente

### Requirement: Carpetas inaccesibles informadas sin abortar
Una carpeta que no puede listarse (acceso denegado, en uso, error de E/S) SHALL contarse como inaccesible en el resumen de su categoría y el recorrido MUST continuar con el resto; ningún fallo de una carpeta o archivo MUST abortar el escaneo.

#### Scenario: Subcarpeta sin permisos
- **WHEN** `C:\Windows\Temp\privada` no puede listarse y `C:\Windows\Temp` contiene además `a.tmp` elegible
- **THEN** `temp_windows` termina `done` con `a.tmp` como hallazgo y una carpeta inaccesible

#### Scenario: Ubicación entera sin permisos
- **WHEN** la app corre sin elevar y `C:\Windows\Temp` no puede listarse
- **THEN** `temp_windows` termina `done` con cero hallazgos y una carpeta inaccesible, y las demás categorías se escanean

### Requirement: Resumen por categoría
El resultado de un escaneo SHALL incluir, por categoría: alcance, estado (`done`, `partial`, `blocked`, `unavailable`), motivo de bloqueo si corresponde, cantidad y bytes elegibles, cantidad de omitidos y cantidad de carpetas inaccesibles; y totales elegibles del escaneo.

#### Scenario: Totales
- **WHEN** `temp_user` tiene 3 elegibles (6 MB) y 1 omitido, y `wu_cache` tiene 2 elegibles (4 MB)
- **THEN** los totales del escaneo son 5 elementos y 10 MB, y el omitido no suma

#### Scenario: Papelera en los totales
- **WHEN** la Papelera informa 340 MB
- **THEN** su tamaño figura en su categoría pero no se suma a los totales elegibles del escaneo

### Requirement: Un escaneo de basura a la vez con progreso
El sistema SHALL ejecutar como máximo un escaneo a la vez sin bloquear la interfaz e informar progreso durante el recorrido (categoría actual, archivos vistos y bytes elegibles acumulados); pedir otro escaneo mientras uno está en curso MUST fallar con el error `scan_in_progress` sin afectar al que corre.

#### Scenario: Progreso
- **WHEN** se inicia un escaneo
- **THEN** la interfaz recibe al menos un aviso de inicio con el identificador del escaneo, avisos de progreso por categoría y el resultado final al terminar

#### Scenario: Segundo escaneo
- **WHEN** se pide un escaneo mientras otro está en curso
- **THEN** la petición falla con `scan_in_progress` y el primero termina normalmente

### Requirement: Cancelación de un escaneo
El usuario SHALL poder cancelar el escaneo en curso; el recorrido MUST detenerse en el próximo archivo o carpeta, el escaneo MUST quedar `cancelled` y sus hallazgos parciales MUST descartarse. Cancelar un escaneo que no está en curso MUST fallar con `scan_not_found`.

#### Scenario: Cancelación a mitad
- **WHEN** el usuario cancela mientras se recorre `browser_cache_chrome`
- **THEN** el escaneo queda `cancelled`, no se recorren más categorías y no quedan hallazgos guardados de ese escaneo

#### Scenario: Cancelación tardía
- **WHEN** se pide cancelar un escaneo ya terminado
- **THEN** la petición falla con `scan_not_found` y el escaneo conserva su estado y sus hallazgos

### Requirement: Hallazgos persistidos para la limpieza
Al terminar un escaneo con éxito, el sistema SHALL guardar cada hallazgo (elegible u omitido) con su escaneo, categoría, ruta real, tamaño, fecha de última escritura, marca de omitido con su motivo y, si es elegible, su identidad de archivo; un escaneo fallido MUST quedar `error` sin hallazgos.

#### Scenario: Identidad guardada
- **WHEN** un hallazgo elegible se guarda
- **THEN** al releerlo se obtienen el mismo volumen e identificador de archivo que informó la verificación

#### Scenario: Fallo de la base al guardar
- **WHEN** la base falla al guardar los hallazgos
- **THEN** el escaneo informa error, no quedan hallazgos parciales y el escaneo no queda `running`

### Requirement: Consulta del último escaneo por IPC
El backend SHALL exponer el resumen del último escaneo de basura terminado `done` (o ninguno) y una consulta paginada de sus hallazgos por categoría (máximo 500 por página, con total); las respuestas MUST NOT incluir SIDs ni nombres de cuenta, y las rutas solo aparecen en la consulta de hallazgos.

#### Scenario: Sin escaneos
- **WHEN** nunca se escaneó
- **THEN** la consulta del último escaneo devuelve vacío sin error

#### Scenario: Página de hallazgos
- **WHEN** `temp_user` tiene 1200 hallazgos y se pide la página desde 1000 con límite 500
- **THEN** se devuelven 200 hallazgos ordenados por tamaño descendente y el total 1200

#### Scenario: Límite fuera de rango
- **WHEN** se piden hallazgos con límite 0 o 501
- **THEN** la petición falla con un error de argumento inválido

#### Scenario: Escaneo inexistente
- **WHEN** se piden hallazgos de un escaneo que no existe o no es el último escaneo de basura terminado
- **THEN** la petición falla con `scan_not_found`

### Requirement: Pantalla de limpieza en modo simple
La pantalla Limpieza SHALL ofrecer una única acción primaria para buscar basura y, tras el escaneo, una tarjeta por categoría con su nombre llano, tamaño legible y estado con icono y texto; en esta versión MUST NOT ofrecer ninguna acción que limpie, mueva o borre.

#### Scenario: Primer uso
- **WHEN** el usuario abre Limpieza sin escaneos previos
- **THEN** ve un estado vacío explicativo y el botón "Buscar basura" como única acción primaria

#### Scenario: Resultado
- **WHEN** termina un escaneo con `temp_user` en 1,2 GB
- **THEN** la tarjeta de archivos temporales muestra "1,2 GB" y el total liberable del escaneo, y no hay botón de limpiar

#### Scenario: Último escaneo al volver
- **WHEN** el usuario vuelve a la pantalla tras un escaneo terminado
- **THEN** ve el resumen del último escaneo sin volver a escanear

### Requirement: Progreso y cancelación accesibles
Durante un escaneo la pantalla SHALL mostrar la categoría actual y el avance en una región anunciada a lectores de pantalla, ofrecer un botón "Cancelar" alcanzable por teclado y deshabilitar el inicio de otro escaneo.

#### Scenario: Escaneo en curso
- **WHEN** hay un escaneo en curso
- **THEN** el progreso está en una región `aria-live="polite"`, "Buscar basura" está deshabilitado y "Cancelar" tiene foco alcanzable con Tab

#### Scenario: Cancelado
- **WHEN** el usuario cancela
- **THEN** la pantalla anuncia que el escaneo se canceló y conserva el resumen del último escaneo terminado, si existe

### Requirement: Categorías bloqueadas explicadas
Una categoría `blocked` o `partial` SHALL mostrar en su tarjeta, desde el catálogo, por qué no se escaneó (otra cuenta de Windows o cuenta no verificada) y que el resto sigue disponible; una `unavailable` SHALL mostrar el estado "no disponible" explícito, nunca un tamaño de cero.

#### Scenario: Over-the-shoulder
- **WHEN** el escaneo informa `temp_user` `blocked/different_user`
- **THEN** la tarjeta muestra el texto de "la app se abrió con otra cuenta de Windows", sin tamaño, con icono de advertencia y texto

#### Scenario: Logs viejos
- **WHEN** `old_logs` informa `unavailable`
- **THEN** su tarjeta muestra "No disponible en esta versión" y no "0 B"

### Requirement: Rutas y motivos en modo avanzado
En modo avanzado cada tarjeta SHALL permitir ver los hallazgos de su categoría paginados, con ruta completa, tamaño, fecha y, para los omitidos, el motivo traducido desde el catálogo (con un texto genérico para códigos desconocidos); en modo simple MUST NOT mostrarse rutas.

#### Scenario: Ver rutas
- **WHEN** en modo avanzado el usuario expande `temp_user`
- **THEN** ve las primeras 100 rutas ordenadas por tamaño y puede cargar más

#### Scenario: Motivo de omisión
- **WHEN** un hallazgo está omitido con motivo `reparse_point`
- **THEN** se muestra el texto del catálogo para enlaces no seguidos junto al código

#### Scenario: Modo simple
- **WHEN** el modo es simple
- **THEN** ninguna ruta de archivo aparece en la pantalla

### Requirement: Tests sin el disco real del usuario
La lógica de detección SHALL probarse con dobles de sistema de archivos, de Papelera, de persistencia y de contexto de privilegios; los tests que recorran el disco o consulten la Papelera reales MUST limitarse a directorios temporales o quedar ignorados por defecto con el motivo indicado.

#### Scenario: Suite por defecto
- **WHEN** se ejecuta la suite de Rust sin `--ignored`
- **THEN** ningún test lee el temporal, el perfil, la Papelera ni `C:\Windows` reales de la máquina

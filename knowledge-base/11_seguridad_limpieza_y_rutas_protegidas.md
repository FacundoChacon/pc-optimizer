# Seguridad de Limpieza y Rutas Protegidas

Dominio de gobernanza **HIGH**. Este documento es la referencia operativa de RN-LIM y RN-PROT (ver 05). Cualquier cambio requiere propuesta, revisión y tests antes de escribir código.

## Principios

1. **Fail closed**: whitelist de lo que se puede limpiar; ante duda, no tocar.
2. **Recuperable**: todo va a la Papelera.
3. **Transparente**: preview con cantidad y tamaño + confirmación explícita + log.
4. **Defensa en profundidad**: protección verificada al escanear y de nuevo al ejecutar.

## Categorías limpiables (whitelist)

| Categoría | Ruta / origen | Notas |
|-----------|---------------|-------|
| `temp_user` | `%TEMP%` del usuario de la sesión interactiva | Bloqueada si over-the-shoulder |
| `temp_windows` | `C:\Windows\Temp` | Subruta explícita permitida |
| `browser_cache_*` | Cachés de Chrome, Edge, Firefox, Brave (solo directorios de caché, nunca perfiles/cookies/contraseñas/historial) | Por-usuario |
| `old_logs` | Logs viejos en ubicaciones conocidas | **Suposición:** antigüedad mínima 30 días |
| `wu_cache` | `C:\Windows\SoftwareDistribution\Download` | Subruta explícita permitida |
| `thumbnail_cache` | Caché de miniaturas del Explorador | Por-usuario |
| `crash_dumps` | Volcados de fallos conocidos (ej. `%LOCALAPPDATA%\CrashDumps`, `Minidump`) | |
| `recycle_bin` | Papelera | Única acción permanente (C-10): protocolo backend de 3 pasos (preparar → reconocer → vaciar), solo volumen del sistema, vence a 2 min, re-medición estricta, bloqueada en mismatch; se registra con `scan_id` nulo (no marca el escaneo como limpiado) |
| `custom_rule` | Reglas del usuario (v0.3) | Siempre sujetas a rutas protegidas |

### Ubicaciones concretas de la detección (C-08)

Derivadas de las carpetas que informa el SO (nunca rutas fijas). Cada hallazgo es un archivo y pasa por la verificación de protección.

| Categoría | Ubicaciones | Alcance |
|-----------|-------------|---------|
| `temp_user` | Temporal del usuario; solo archivos con última escritura >= 24 h | Por-usuario |
| `temp_windows` | `<Windows>\Temp`; solo archivos con última escritura >= 24 h | Máquina |
| `browser_cache_chrome` / `_edge` / `_brave` | `<Local>\Google\Chrome` / `Microsoft\Edge` / `BraveSoftware\Brave-Browser`, luego `\User Data\<perfil>\{Cache, Code Cache, GPUCache}` | Por-usuario |
| `browser_cache_firefox` | `<Local>\Mozilla\Firefox\Profiles\<perfil>\cache2` (nunca `<Roaming>`) | Por-usuario |
| `old_logs` | Sin ubicaciones en esta versión (PA-15); antigüedad prevista >= 30 días | - |
| `wu_cache` | `<Windows>\SoftwareDistribution\Download` | Máquina |
| `thumbnail_cache` | `<Local>\Microsoft\Windows\Explorer`, solo primer nivel y solo `thumbcache_*.db` | Por-usuario |
| `crash_dumps` | `<Local>\CrashDumps` (por-usuario); `<Windows>\Minidump`, `<ProgramData>\Microsoft\Windows\WER\ReportArchive` y `\ReportQueue` (máquina) | Mixto |
| `recycle_bin` | Solo medida (cantidad y tamaño) de la Papelera del usuario en el volumen del sistema; sin listar ni modificar | Por-usuario |

La detección no abre archivos para probar si están en uso: se omiten al limpiar.
## Rutas protegidas (nunca se tocan)

| Ruta | Excepción permitida |
|------|---------------------|
| `C:\Windows` (incl. `System32`, `WinSxS`, `Installer`) | Solo `Windows\Temp` y `SoftwareDistribution\Download` |
| `C:\Program Files`, `C:\Program Files (x86)` | Ninguna |
| `C:\ProgramData` | Solo cachés whitelisted |
| Perfiles de otros usuarios | Ninguna |
| OneDrive: placeholders / cloud-only | Ninguna (se omiten y reportan) |
| Archivos en uso / bloqueados | Se omiten y reportan; nunca se fuerzan |
| Perfiles de navegador (cookies, logins, historial, extensiones) | Ninguna |

**Suposición:** se protegen también `$Recycle.Bin` (salvo la acción de vaciar), `System Volume Information`, `pagefile.sys`, `hiberfil.sys` y raíces de volumen.

## Algoritmo de verificación (dominio puro)

1. Canonicalizar la ruta (quitar `\\?\`, resolver `..`, normalizar mayúsculas y separadores, resolver 8.3 y reparse points).
2. Si falla la canonicalización → **protegida**.
3. Si coincide con una excepción explícita de la whitelist → candidata.
4. Si cae bajo una ruta protegida → **protegida**.
5. Si es de otro perfil de usuario → **protegida**.
6. Si tiene atributos de placeholder/offline → **omitida (cloud)**.
7. Se vuelve a correr al ejecutar (anti-TOCTOU) justo antes de mover a la Papelera.

## Flujo de seguridad de una limpieza

```
escaneo → clasificación whitelist → is_protected → preview (n, bytes)
   → confirmación explícita → re-verificación → Papelera → cleanup_log
```

Detalle de la re-verificación al confirmar (C-09), por ítem y en este orden:

- **Re-verificación por handle**: se vuelve a evaluar el alcance por-usuario con el contexto de privilegios de ese momento, se fijan los ancestros y se repite la resolución completa sobre la ruta real persistida (`prepare_delete`), comparando identidad (volumen + id de archivo) y tipo; el tamaño y la fecha de última escritura se leen del archivo ABIERTO (no del listado de la carpeta, que NTFS actualiza tarde en archivos abiertos para escritura). Si tamaño o fecha difieren de lo persistido → `skipped_locked` / `changed_since_scan`.
- **Guarda de 24 h**: ningún archivo con última escritura en las últimas 24 horas (o sin fecha) se mueve, en todas las categorías; se evalúa al previsualizar con la fecha persistida y al confirmar con la fecha del handle.
- **Aborto por Papelera no disponible**: si la Papelera del volumen no está disponible (deshabilitada, inexistente) o su mecanismo no puede usarse, la ejecución se aborta en el primer caso (el ítem queda como `error`) y no se intenta ningún otro; nunca se borra de forma permanente. Un fallo por ítem (en uso, demasiado grande, acceso denegado) no detiene el resto.

## Duplicados: re-verificación de contenido (C-11)

Las copias de un grupo se mueven por `run_cleanup` con un gancho adicional que, con la fijación viva y antes de mover, re-hashea el archivo abierto: hash distinto del grupo → `skipped_locked` / `content_changed`; no se puede leer → `skipped_locked` / `content_unreadable` (o `in_use`); identidad distinta → `skipped_protected` / `identity_changed`. Antes de mover la primera copia de un grupo se verifica que al menos una copia conservada sigue en su ruta con la misma identidad, tamaño, fecha y hash; si ninguna, todas las del grupo quedan `skipped_protected` / `kept_copy_unavailable`. El escaneo nunca abre lo que la protección no declaró `allowed` ni contenido en la nube.

## Casos borde a cubrir con tests

- Rutas con `..`, mayúsculas/minúsculas mezcladas, `\\?\`, nombres cortos 8.3.
- Junctions/symlinks apuntando fuera de la whitelist (no se siguen).
- Archivo reemplazado por un symlink entre escaneo y ejecución.
- Volumen sin Papelera (red/USB): se omite y reporta.
- Nombres de usuario con espacios/Unicode.
- Regla personalizada que apunta a `C:\`, a `C:\Users` entero o a `C:\Windows`: rechazada.
- Over-the-shoulder: categorías por-usuario bloqueadas.

## Over-the-shoulder y UIPI

- **Over-the-shoulder**: un usuario estándar ingresa credenciales de OTRA cuenta admin en UAC → el proceso corre como esa cuenta y `%TEMP%`, cachés, Papelera y HKCU son del perfil equivocado. La app compara el usuario del token elevado con el de la sesión interactiva; si difieren, advierte y bloquea la limpieza por-usuario (RN-PRIV-03).
- **UIPI**: el drag & drop desde el Explorador (no elevado) hacia una ventana elevada es bloqueado por Windows. La UI debe ofrecer un selector de carpeta como alternativa.

## Registro (auditoría)

`cleanup_log` es append-only: `ts`, `category`, `path`, `size_bytes`, `outcome`, `scan_id`, `error_detail`. Visible en la UI (C-12: pantalla Historial; modo simple sin rutas ni códigos, modo avanzado con rutas completas; con contexto `mismatch` el backend devuelve las rutas ocultas) y en reportes (rutas ofuscadas por defecto, regla distinta que define C-27).

`error_detail` es un **código estable** (ASCII `snake_case`, sin rutas ni texto localizado) para todo outcome distinto de `moved_to_trash`. Tabla de C-09 (design D-06; sin outcomes nuevos):

| Situación | outcome | `error_detail` |
|---|---|---|
| Barrera por-usuario con sesión de otro usuario / sin determinar | `skipped_protected` | `blocked_different_user` / `blocked_undetermined` |
| Re-verificación de protección rechazada | `skipped_protected` | código de `ProtectedReason` (`identity_changed`, `reparse_point`, `multiple_hard_links`, `other_user_profile`, `unresolvable_not_found`, ...) |
| El archivo está abierto por otro proceso al verificar | `skipped_locked` | `unresolvable_in_use` |
| Pasó a ser placeholder de nube | `skipped_cloud` | `skipped_cloud` |
| Tamaño o fecha distintos de los del escaneo | `skipped_locked` | `changed_since_scan` |
| Escrito hace menos de 24 h o sin fecha | `skipped_locked` | `recently_modified` |
| Papelera: en uso | `skipped_locked` | `in_use` |
| Papelera: no disponible en el volumen (aborta) | `error` | `recycle_bin_unavailable` |
| Papelera: ítem demasiado grande | `error` | `recycle_bin_too_small` |
| Papelera: ruta demasiado larga | `error` | `path_too_long` |
| Papelera: acceso denegado / requiere elevación | `error` | `permission_denied` |
| Papelera: el ítem desapareció | `error` | `not_found` |
| Papelera respondió éxito pero el archivo sigue | `error` | `not_moved` |
| Mecanismo de Papelera inutilizable (aborta) | `error` | `trash_unavailable` |
| Otro fallo | `error` | `io` o `io_0x<HRESULT>` |

## Nota C-07: selector de carpeta y Explorador en el proceso elevado

`pick_folder_root` (`IFileOpenDialog` con `FOS_PICKFOLDERS`) y `reveal_in_explorer` (`SHOpenFolderAndSelectItems`, nunca `ShellExecute`) corren en hilos COM STA propios del backend, sin plugins `dialog`/`shell` ni capabilities nuevas. Riesgo conocido: el diálogo del shell carga extensiones de terceros dentro del proceso elevado (mitigado con `FOS_PICKFOLDERS`). En over-the-shoulder el Explorador está bloqueado.

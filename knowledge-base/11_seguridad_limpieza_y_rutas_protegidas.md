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
| `recycle_bin` | Papelera | Única acción permanente; doble confirmación; pregunta abierta PA-02 |
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

`cleanup_log` es append-only: `ts`, `category`, `path`, `size_bytes`, `outcome`, `scan_id`. Visible en la UI (modo avanzado: rutas completas) y en reportes (rutas ofuscadas por defecto).

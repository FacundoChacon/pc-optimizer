# Proposal

## Why

Todo borrado de pc-optimizer (C-08 junk, C-09 Papelera, C-11 duplicados, C-21 reglas personalizadas) depende de una única pregunta: "¿se puede tocar esta ruta?". Hoy no existe respuesta: C-02 dejó el puerto `FileSystem` y sus fakes, pero ni el algoritmo de protección (KB 11 §Algoritmo, RN-PROT-01..07) ni un adaptador real del sistema de archivos. Como la app corre **siempre elevada**, un error aquí no es un bug menor: una junction plantada en `%TEMP%` por un proceso sin privilegios podría convertir a la app en una primitiva de "borrado arbitrario como administrador" (clase de vulnerabilidad conocida en limpiadores y antivirus). Es el siguiente paso del camino crítico (`C-02 → C-03 → C-08 → C-09`) y gobernanza ALTO.

## What Changes

- **Dominio puro de protección** (`src-tauri/src/domain/protection/`):
  - Normalización léxica pura de rutas Windows (`\\?\`, `..`/`.`, separadores, mayúsculas con plegado simple tipo NTFS, rechazo de UNC/dispositivo/ADS/nombres reservados/puntos y espacios finales).
  - `ProtectionContext` inyectado (unidad del sistema, `Windows`, Program Files, ProgramData, raíz de perfiles, carpetas del usuario actual, directorios de la app); sin `C:` en el código.
  - Evaluación pura de reglas (`evaluate_rules` / `is_protected`): denegaciones duras, raíces protegidas y una **lista cerrada** de excepciones propiedad de C-03 (`Windows\Temp`, `SoftwareDistribution\Download`, `Windows\Minidump`, WER, cachés de Chromium/Firefox).
  - Servicio `PathProtection` que resuelve la ruta real vía el puerto `FileSystem` (sin atravesar reparse points, expandiendo 8.3 por canonicalización) y produce una decisión con motivo (`Allowed`, `SkippedCloud`, `Protected(reason)`), **fail-closed** de punta a punta.
  - Anti-TOCTOU: `prepare_delete` fija (pin) los ancestros, re-verifica y compara identidad de archivo (volumen + file id) contra lo escaneado; C-09 mueve a la Papelera mientras el guard vive.
  - Validación de raíz de reglas (`check_rule_root`) para C-21: rechaza `C:\`, `C:\Users`, `C:\Windows`, el perfil, AppData y cualquier ancestro de una ruta protegida.
- **Puerto `FileSystem` ampliado** (cambio mínimo): `FileMeta` suma identidad de archivo y cantidad de hard links; `canonicalize` no sigue el último componente; nueva operación `pin_ancestors`. Se precisa la semántica de `cloud_placeholder` (incluye tags de reparse de nube).
- **Adaptadores Windows** (`src-tauri/src/adapters/windows/`): `WindowsFileSystem` (implementación real del puerto, primera de C-02) y resolución del `ProtectionContext` con Known Folder APIs, dejando la costura del "usuario interactivo" para C-05.
- Tests de tabla exhaustivos, invariantes enumeradas (sin dependencias nuevas) y tests reales del adaptador con junctions/symlinks en `tempdir`.

## Capabilities

### New Capabilities
- `path-protection`: decide si una ruta puede ser objeto de limpieza: normalización, contexto inyectado, reglas protegidas y excepciones cerradas, semántica de reparse points/nube/hard links, decisiones con motivo, re-verificación anti-TOCTOU y validación de raíces de reglas.

### Modified Capabilities
- `domain-ports`: el puerto de sistema de archivos informa identidad de archivo y hard links, canonicaliza sin seguir el último componente, amplía la detección de nube y agrega la fijación de ancestros; su contrato pasa a verificarse también contra el adaptador real.

## Impact

- **Código nuevo**: `src-tauri/src/domain/protection/{mod,path,context,rules,decision,service}.rs` + tests; `src-tauri/src/adapters/windows/{mod,fs,known_folders}.rs`.
- **Código modificado**: `domain/ports/fs.rs` (`FileMeta`, `FileIdentity`, `AncestorPin`, `pin_ancestors`), `domain/ports/fakes.rs` (builders de identidad, hard links, canonicalización identidad, pins), `domain/mod.rs`, `adapters/mod.rs`.
- **Dependencias**: ninguna crate nueva ni cambio de versión. Se habilitan **features** del crate `windows` ya fijado (`=0.62.2`): `Win32_Foundation`, `Win32_Security`, `Win32_Storage_FileSystem`, `Win32_System_Com`, `Win32_UI_Shell` (y `Win32_System_SystemServices` si no se definen las constantes de reparse localmente). `Cargo.lock` no debe cambiar (verificado con `--locked`). `windows` deja de figurar como dependencia sin uso.
- **Sin IPC ni UI**: C-03 no expone comandos; los primeros consumidores son C-07/C-08 (decisión por candidato), C-09 (`prepare_delete`), C-11 y C-21 (`check_rule_root`).
- **Fuera de alcance**: categorías de basura (C-08), el movimiento a la Papelera y su adaptador (C-09), detección del usuario de la sesión interactiva (C-05), soporte de otros volúmenes (PA-12, por defecto solo el volumen del sistema).

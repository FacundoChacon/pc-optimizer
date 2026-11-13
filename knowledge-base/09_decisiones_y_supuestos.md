# Decisiones y Supuestos

## Decisiones documentadas

### DD-01 — Tauri 2 + Rust + React como stack
**Decisión**: Tauri 2 (backend Rust) con frontend React + Vite + TypeScript + Tailwind.
**Contexto**: app de escritorio Windows que necesita acceso de bajo nivel (FS, Win32, WMI) y UI rica.
**Alternativas consideradas**: Electron (Node), PySide/PyQt (Python).
**Justificación**: binarios chicos y bajo consumo de RAM (importante para una app de optimización y modo bandeja); Rust da seguridad de memoria y buen acceso a Win32; separación dominio/adaptadores favorece TDD; UI web moderna para gráficos y modos.
**Trade-offs aceptados**: curva de Rust; dependencia de WebView2; menos ecosistema de gráficos de escritorio que Qt.

### DD-02 — Ejecutar siempre como administrador
**Decisión**: manifest `requireAdministrator`; UAC en cada arranque.
**Contexto**: leer SMART/WMI, limpiar `Windows\Temp` y `SoftwareDistribution`, y gestionar tareas programadas requieren elevación.
**Alternativas consideradas**: (a) elevación bajo demanda por acción; (b) servicio de Windows con UI no elevada.
**Justificación**: la decisión del usuario prioriza simplicidad y que todas las funciones funcionen sin flujos parciales. Un servicio amplía superficie de ataque y complejidad de instalación; la elevación bajo demanda complica la arquitectura (procesos helper, IPC elevado).
**Trade-offs aceptados**: UAC en cada inicio; mayor impacto si hay un bug (mitigado por whitelist, protección de rutas y Papelera); riesgo over-the-shoulder (RN-PRIV-03); UIPI bloquea drag & drop del Explorador.

### DD-03 — Autostart con Task Scheduler "highest privileges"
**Decisión**: el autostart de la bandeja es opt-in y usa una tarea programada con privilegios más altos.
**Contexto**: con always-admin, la clave `Run` del registro no puede arrancar un proceso elevado sin UAC.
**Alternativas consideradas**: clave `Run`/carpeta Inicio; servicio.
**Justificación**: es el mecanismo estándar para autostart elevado sin prompt.
**Trade-offs aceptados**: crear la tarea requiere admin; debe quitarse limpiamente al desinstalar o desactivar la opción.

### DD-04 — Sin drivers de kernel
**Decisión**: jamás se usan drivers de kernel; temperaturas best-effort (NVMe/SMART, NVML/ADLX, ACPI vía WMI).
**Contexto**: leer temperatura de CPU con precisión suele requerir WinRing0 u otro driver.
**Alternativas consideradas**: WinRing0 / librerías tipo LibreHardwareMonitor con driver.
**Justificación**: Microsoft Defender marca WinRing0 como vulnerable; un driver firmado de terceros es un riesgo de seguridad inaceptable para una app pública.
**Trade-offs aceptados**: la temperatura de CPU frecuentemente figurará como "no disponible"; se documenta honestamente.

### DD-05 — Solo Papelera, nunca borrado permanente
**Decisión**: todo borrado va a la Papelera; vaciar la Papelera es la única acción permanente y con doble confirmación (pendiente de ratificar).
**Alternativas consideradas**: borrado permanente directo; mover a carpeta de cuarentena propia.
**Justificación**: recuperabilidad nativa y entendible para el usuario; sin infraestructura extra de cuarentena.
**Trade-offs aceptados**: el espacio no se libera hasta vaciar la Papelera; archivos en volúmenes sin Papelera pueden fallar (se omiten y reportan).

### DD-06 — Whitelist de limpieza + rutas protegidas
**Decisión**: limpiar solo categorías conocidas; protección de rutas en dominio puro.
**Alternativas consideradas**: blacklist (borrar todo salvo lo protegido); heurísticas de "archivos sin uso".
**Justificación**: una blacklist falla abierta; una whitelist falla cerrada. Seguridad > cantidad de espacio liberado.
**Trade-offs aceptados**: se libera menos espacio que en cleaners agresivos.

### DD-07 — Mejoras solo reversibles y de lista cerrada
**Decisión**: catálogo cerrado (inicio, plan de energía, efectos visuales, limpieza whitelisted) con snapshot en SQLite.
**Alternativas consideradas**: tweaks de registro/servicios genéricos; un "optimizador total" con un click.
**Justificación**: cada cambio debe poder deshacerse con exactitud; los tweaks genéricos son la fuente típica de roturas.
**Trade-offs aceptados**: menos "poder" que otras herramientas.

### DD-08 — SQLite local
**Decisión**: `rusqlite` con archivo local.
**Alternativas consideradas**: archivos JSON; sled/otra KV; DB embebida con ORM.
**Justificación**: consultas de series de tiempo, agregación y retención; transaccionalidad; una sola dependencia madura.
**Trade-offs aceptados**: migraciones a mano; consumo de disco acotado por políticas de retención.

### DD-09 — Dos modos de UI (simple / avanzado)
**Decisión**: arranca en simple; avanzado se activa en Configuración. Mismo estilo oscuro.
**Alternativas consideradas**: una sola UI densa; dos apps; modo simple con "ver más" por pantalla.
**Justificación**: el público abarca comunes y avanzados; un toggle global evita duplicar lógica (solo cambia presentación).
**Trade-offs aceptados**: más trabajo de UI/QA; riesgo de inconsistencia (ver IN-01 en 10).

### DD-10 — Sin telemetría, sin red, sin cuentas
**Decisión**: app 100 % local.
**Alternativas consideradas**: telemetría anónima opt-in; reporte de crashes remoto; auto-update.
**Justificación**: confianza (repo público, app con privilegios altos); menor superficie de ataque; consistente con "explica y no oculta".
**Trade-offs aceptados**: sin datos de uso ni crash reports automáticos; sin auto-update en v0.x.

### DD-11 — Distribución por GitHub Releases, sin firma en v0.x
**Decisión**: instalador NSIS/MSI sin firma de código; SmartScreen mostrará advertencia (documentada).
**Alternativas consideradas**: firmar con certificado desde v0.1; Microsoft Store.
**Justificación**: costo y trámite del certificado no justificados en pre-1.0; se revisa más adelante.
**Trade-offs aceptados**: fricción de instalación y desconfianza inicial.

### DD-12 — Strict TDD con dominio puro separado
**Decisión**: lógica en `domain` sin I/O; adaptadores tras traits con fakes.
**Justificación**: las reglas de seguridad deben probarse sin tocar el disco real.
**Trade-offs aceptados**: más código de interfaces; disciplina de capas.

### DD-13 — Duplicados con hash por etapas y decisión humana
**Decisión**: tamaño → hash parcial (64 KB inicio/fin) → blake3 completo; usuario elige qué conservar.
**Alternativas consideradas**: hash completo de todo; auto-elegir la copia "más vieja".
**Justificación**: eficiencia de I/O; el significado de "cuál conservar" depende del contexto del usuario.
**Trade-offs aceptados**: más pasos en la UI.

### DD-14 — Apps sin uso: solo sugerir
**Decisión**: sugerencia con señales best-effort; desinstalar abre el desinstalador del programa.
**Justificación**: no hay señal fiable de "uso"; la desinstalación silenciosa es riesgosa.
**Trade-offs aceptados**: más clicks para el usuario.

## Supuestos inferidos

### SU-01 — Pesos del health score
**Supuesto**: disco libre 30 %, RAM 20 %, basura 15 %, inicio 15 %, SMART 10 %, temperaturas 10 %.
**Origen**: inferido del brief (el usuario pidió score ponderado, sin pesos).
**Riesgo si es falso**: score poco representativo.
**Cómo validar**: probar con equipos reales y ajustar; hacer los pesos configurables en código.

### SU-02 — Umbrales de alertas por defecto
**Supuesto**: disco > 90 %, temperatura > 85 °C, RAM > 90 % sostenida 60 s, cooldown 6 h.
**Origen**: brief (disco > 90 %); resto inferido.
**Riesgo si es falso**: spam o alertas tardías.
**Cómo validar**: uso real en v0.3; exponer en configuración.

### SU-03 — Los escaneos programados no limpian solos
**Supuesto**: solo detectan y notifican.
**Origen**: coherente con HIGH governance y confirmación obligatoria.
**Riesgo si es falso**: el usuario esperaba limpieza automática.
**Cómo validar**: feedback en v0.3; si se pide, diseñar "limpieza automática" como feature aparte con revisión.

### SU-04 — Directorio de datos en el perfil del usuario interactivo
**Supuesto**: `%APPDATA%` del usuario real, no del token elevado.
**Origen**: inferido para evitar guardar datos en el perfil de otra cuenta (over-the-shoulder).
**Riesgo si es falso**: datos en perfil equivocado.
**Cómo validar**: test manual con over-the-shoulder.

### SU-05 — Windows 10 22H2 como mínimo
**Supuesto**: soportado; profundidad pendiente (ver 10).
**Origen**: inferido (Win10 llegó a fin de soporte en oct-2025).
**Riesgo si es falso**: APIs/WMI distintas.
**Cómo validar**: pruebas en VM Win10.

### SU-06 — Duplicados excluyen archivos < 1 MB por defecto
**Supuesto**: reduce ruido y costo.
**Origen**: inferido.
**Riesgo si es falso**: se omiten duplicados relevantes.
**Cómo validar**: opción configurable en modo avanzado.

### SU-07 — Reglas personalizadas solo en modo avanzado
**Supuesto**: el usuario común no las ve.
**Origen**: inferido de la separación de modos.
**Riesgo si es falso**: usuarios comunes sin acceso a una función útil.
**Cómo validar**: revisión de usabilidad.

### SU-08 — Sin cifrado de la DB local
**Supuesto**: los datos (rutas, métricas) no son sensibles críticos.
**Origen**: inferido.
**Riesgo si es falso**: fuga de rutas con nombres personales.
**Cómo validar**: revisar contenido de la DB; ofuscación en reportes (RN-REP-04).

### SU-09 — Link simbólicos/junctions no se siguen
**Supuesto**: más seguro; puede subestimar tamaños.
**Origen**: inferido de RN-PROT.
**Riesgo si es falso**: carpetas montadas por junction aparecen como vacías.
**Cómo validar**: pruebas con junctions comunes (OneDrive, perfiles).

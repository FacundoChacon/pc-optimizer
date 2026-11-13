# Visión y Objetivos

## Propósito del sistema

**pc-optimizer** es una aplicación de escritorio para Windows 10/11 que analiza el almacenamiento y el rendimiento del hardware, detecta archivos basura y pesados, los elimina de forma segura, sugiere y aplica mejoras reversibles, y puede correr como asistente continuo en la bandeja del sistema.

**Problema**: los usuarios (comunes y avanzados) no saben qué llena su disco ni por qué su PC anda lenta. Las herramientas existentes son opacas y agresivas (limpiadores que borran de más) o demasiado técnicas. pc-optimizer **explica, previsualiza y permite deshacer**.

**Tipo y escala**: app local, un solo usuario, sin backend, sin cuentas, sin telemetría, sin acceso a red. Repo público: https://github.com/FacundoChacon/pc-optimizer

## Prioridades de calidad

1. Seguridad (no romper ni borrar de más)
2. Correctitud
3. Mantenibilidad
4. Velocidad de entrega

## Objetivos por actor

| Actor | Objetivo principal | Objetivos secundarios |
|-------|--------------------|-----------------------|
| Usuario común (modo simple) | Entender el estado de su PC con un puntaje y liberar espacio sin riesgo | Una acción primaria por pantalla; explicaciones en lenguaje llano; deshacer cualquier mejora |
| Usuario avanzado (modo avanzado) | Ver métricas crudas, rutas, hashes, procesos y reglas con control fino | Reglas de limpieza propias; reportes exportables; historial y tendencias |
| Sistema/Scheduler | Ejecutar escaneos programados y alertas sin intervención | Respetar cooldown anti-spam; no consumir recursos notables |
| Sistema operativo Windows | (externo) Proveer UAC, Task Scheduler, Papelera, registro, WMI | — |

## Alcance por versión

### v0.1 — Núcleo de limpieza
- Shell de la app + dashboard (simple/avanzado).
- Monitor en vivo de CPU/RAM/disco.
- Análisis de disco por carpeta (treemap/lista) y top de archivos pesados.
- Detección de basura con whitelist de categorías.
- Detección de duplicados (tamaño → hash parcial → blake3 completo).
- Limpieza a la Papelera con preview, confirmación y log.

### v0.2 — Sistema y mejoras
- Programas de inicio, apps sin uso (sugerir), SMART + temperaturas best-effort.
- Mejoras reversibles de 1 click con undo exacto.
- Health score 0-100 e historial de 24 h.

### v0.3 — Asistente continuo
- Modo bandeja, escaneos programados (diario/semanal), alertas.
- Tendencias semanales ("qué carpeta crece más"), reglas de limpieza personalizadas.
- Benchmark antes/después, exportación de reporte HTML y PDF.

## Fuera de alcance

- Borrado permanente de archivos (salvo vaciar la Papelera, con doble confirmación; ver pregunta abierta en 10).
- Drivers de kernel de cualquier tipo (WinRing0 rechazado explícitamente).
- Tweaks genéricos de registro o desactivación de servicios de Windows.
- Desinstalación silenciosa de programas.
- Backend, cuentas, sincronización en la nube, telemetría, acceso a red, auto-update (en v0.x).
- Antivirus / detección de malware.
- Soporte macOS/Linux.

## Métricas de éxito

**Suposición:** al ser app local sin telemetría, las métricas se validan manualmente / por feedback en GitHub.

- Cero incidentes de borrado fuera de la whitelist (verificado por tests de protección de rutas).
- 100 % de las mejoras aplicadas son reversibles al estado previo exacto (test de snapshot/undo).
- Un usuario común completa "escanear → revisar → limpiar" sin salir del modo simple.
- Cobertura alta (**Suposición:** objetivo >= 90 %) en la lógica de dominio pura (reglas, scoring, protección de rutas, clasificación de basura).
- Overhead del modo bandeja en reposo bajo (**Suposición:** < 1 % CPU y < 150 MB RAM).

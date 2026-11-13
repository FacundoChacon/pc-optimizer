# pc-optimizer — Base de Conocimiento

Base de conocimiento de **pc-optimizer**: app de escritorio para Windows 10/11 (Tauri 2 + Rust + React) que analiza almacenamiento y rendimiento, detecta y elimina basura de forma segura (siempre a la Papelera, con preview y log), aplica mejoras reversibles y puede correr como asistente en la bandeja. Local, sin backend, sin cuentas, sin telemetría.

## Índice de Archivos

| Archivo | Contenido |
|---------|-----------|
| [01_vision_y_objetivos.md](01_vision_y_objetivos.md) | Propósito, problema, objetivos por actor, alcance por versión (v0.1-v0.3), fuera de alcance, métricas |
| [02_descripcion_general.md](02_descripcion_general.md) | Stack, arquitectura general, integraciones con Windows, contrato IPC, distribución |
| [03_actores_y_roles.md](03_actores_y_roles.md) | Actores, modos de UI como "roles", contexto de privilegios, matriz de capacidades |
| [04_modelo_de_datos.md](04_modelo_de_datos.md) | Entidades SQLite, ERD mermaid, campos, relaciones, retención, seed |
| [05_reglas_de_negocio.md](05_reglas_de_negocio.md) | Reglas RN-XX por dominio (limpieza, protección, duplicados, mejoras, salud, monitoreo, alertas, privilegios, reportes) |
| [06_funcionalidades.md](06_funcionalidades.md) | Épicas e historias de usuario con hito y gobernanza |
| [07_flujos_principales.md](07_flujos_principales.md) | Flujos extremo a extremo con diagramas de secuencia mermaid |
| [08_arquitectura_propuesta.md](08_arquitectura_propuesta.md) | Patrones, gobernanza HIGH/MEDIUM/LOW, estructura de directorios, seguridad Tauri, testing |
| [09_decisiones_y_supuestos.md](09_decisiones_y_supuestos.md) | Decisiones DD-xx con alternativas y supuestos SU-xx |
| [10_preguntas_abiertas.md](10_preguntas_abiertas.md) | Inconsistencias y preguntas abiertas priorizadas |
| [11_seguridad_limpieza_y_rutas_protegidas.md](11_seguridad_limpieza_y_rutas_protegidas.md) | Whitelist de limpieza, rutas protegidas, algoritmo de verificación, over-the-shoulder, UIPI |
| [12_ui_modos_simple_avanzado.md](12_ui_modos_simple_avanzado.md) | Modos simple/avanzado, pantallas, accesibilidad, i18n |
| [13_monitoreo_hardware_y_privilegios.md](13_monitoreo_hardware_y_privilegios.md) | Always-admin, autostart por Task Scheduler, sensores sin driver, muestreo y retención |

## Orden de lectura recomendado

1. Dominio y alcance → [01](01_vision_y_objetivos.md), [03](03_actores_y_roles.md)
2. Seguridad (lo más crítico) → [11](11_seguridad_limpieza_y_rutas_protegidas.md), [05](05_reglas_de_negocio.md)
3. Datos → [04](04_modelo_de_datos.md)
4. Arquitectura → [02](02_descripcion_general.md), [08](08_arquitectura_propuesta.md), [13](13_monitoreo_hardware_y_privilegios.md)
5. Producto y UI → [06](06_funcionalidades.md), [07](07_flujos_principales.md), [12](12_ui_modos_simple_avanzado.md)
6. Antes de codificar → [09](09_decisiones_y_supuestos.md), [10](10_preguntas_abiertas.md)

## Convenciones

- Idioma: español rioplatense neutro, técnico.
- Ítems inferidos están marcados como **Suposición:**.
- Gobernanza: borrado y tweaks del sistema = HIGH; scanners/monitor = MEDIUM; UI/catálogos/tipos = LOW.
- Códigos: `RN-XX-NN` (reglas), `US-NNN` (historias), `DD-NN` (decisiones), `SU-NN` (supuestos), `IN-NN` (inconsistencias), `PA-NN` (preguntas).
- El roadmap de changes irá en `CHANGES.md` (se genera después).

## Resumen Ejecutivo

pc-optimizer prioriza seguridad > correctitud > mantenibilidad > velocidad: limpia solo una whitelist de categorías, nunca toca rutas protegidas, siempre usa la Papelera con preview y log, y solo aplica mejoras de una lista cerrada con undo exacto. Corre siempre como administrador (sin drivers de kernel), con modo simple por defecto y modo avanzado opcional. Se entrega en tres hitos: v0.1 núcleo de limpieza, v0.2 sistema y mejoras, v0.3 asistente continuo.

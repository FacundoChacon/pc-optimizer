# Actores y Roles

pc-optimizer no tiene RBAC clásico: es una app local, de un solo usuario, sin cuentas ni login. Los "roles" son **modos de UI** más un **contexto de privilegios** (siempre elevado).

## Actores del sistema

| Actor | Descripción | Cómo interactúa |
|-------|-------------|-----------------|
| Usuario común | Persona sin conocimientos técnicos. Usa **modo simple** (default). | Mira health score, tarjetas grandes, ejecuta una acción primaria por pantalla, confirma previews. |
| Usuario avanzado | Persona técnica. Activa **modo avanzado** en Configuración. | Dashboard denso: gauges, gráficos en vivo, métricas crudas, rutas, hashes, procesos, detalle de reglas; define reglas propias. |
| Sistema / Scheduler | Tareas en segundo plano dentro del proceso de bandeja. | Ejecuta escaneos programados, muestrea métricas, evalúa alertas, aplica cooldown. No borra sin confirmación del usuario (**Suposición:** los escaneos programados solo detectan/alertan; no limpian solos). |
| Sistema operativo Windows | Actor externo. | UAC (elevación), Task Scheduler (autostart), Papelera, registro, WMI. Puede denegar acceso o bloquear archivos en uso. |

## Contexto de privilegios (en lugar de RBAC)

| Contexto | Descripción | Efecto |
|----------|-------------|--------|
| Elevado, mismo usuario | Usuario es administrador y aprueba UAC (Consent). Token elevado = usuario de la sesión interactiva. | Funcionamiento completo. |
| Elevado, **otro usuario** (over-the-shoulder) | Usuario estándar ingresa credenciales de OTRA cuenta admin en UAC. El proceso corre como esa otra cuenta. | `%TEMP%`, cachés de navegador, Papelera y claves HKCU pertenecen al perfil equivocado. La app lo detecta y **advierte/bloquea la limpieza por-usuario** (RN-PRIV-03). |
| No elevado | No debería ocurrir (manifest `requireAdministrator`). | La app no arranca si el usuario cancela UAC. |

## Matriz de capacidades por modo (UI)

Ambos modos usan el mismo backend y las mismas reglas de seguridad; cambia solo la **visibilidad** de información y controles.

| Capacidad | Simple | Avanzado |
|-----------|:------:|:--------:|
| Health score y tarjetas explicativas | Sí | Sí |
| Limpieza con preview y confirmación | Sí (flujo guiado) | Sí (con rutas completas) |
| Ver rutas, hashes, procesos, métricas crudas | No | Sí |
| Gauges y gráficos en vivo | Resumen | Completo |
| Reglas de limpieza personalizadas | No (**Suposición**) | Sí |
| Aplicar/deshacer mejoras | Sí (con riesgo e impacto en lenguaje llano) | Sí (con snapshot visible) |
| Umbrales de alertas | Defaults | Editables |
| Saltar protecciones de rutas | Nunca | Nunca |

## Rutas públicas

No aplica: no hay servidor ni autenticación. Todas las pantallas son locales; el acceso está controlado únicamente por UAC al iniciar la app.

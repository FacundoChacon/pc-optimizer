# Preguntas Abiertas

## Inconsistencias detectadas

### IN-01 — Diseño denso de power-user vs. usuarios comunes
**Documento A dice** (visión/diseño): el dashboard avanzado es denso, oscuro, con gauges, gráficos en vivo, rutas y hashes.
**Documento B dice** (problema/objetivos): la app debe servir a usuarios comunes que no entienden términos técnicos.
**Impacto**: si ambos públicos usan la misma UI, uno de los dos queda mal servido.
**Resolución propuesta**: resuelto por modos (simple por defecto, avanzado opt-in; ver 12). Verificar en una revisión de usabilidad (skill `ux-ui-revisor`) tras v0.1.

### IN-02 — "Nunca borrado permanente" vs. vaciar la Papelera
**Documento A dice**: todo borrado va a la Papelera, nunca permanente.
**Documento B dice**: vaciar la Papelera es una acción permanente permitida con doble confirmación.
**Impacto**: contradicción con la regla de seguridad principal si no se define explícitamente.
**Resolución**: ✅ RESUELTO (2026-11-12, dueño del proyecto) — vaciar la Papelera es la única excepción declarada (RN-LIM-06), con doble confirmación mostrando tamaño y cantidad; nunca la disparan escaneos programados ni reglas automáticas.

### IN-03 — Siempre admin vs. principio de mínimo privilegio
**Documento A dice**: la app corre siempre como administrador.
**Documento B dice**: la seguridad es prioridad máxima.
**Impacto**: un bug tiene mayor radio de impacto.
**Resolución propuesta**: aceptado por decisión del usuario (DD-02); se mitiga con whitelist, protección canónica, Papelera, no-drivers y tests.

## Preguntas abiertas (priorizadas)

| ID | Prioridad | Pregunta | Bloquea | Decisor |
|----|-----------|----------|---------|---------|
| PA-01 | Alta | ¿Qué librería de gráficos usar (Recharts, uPlot, visx, ECharts, canvas propio) considerando gráficos en vivo cada 1-2 s y bajo overhead? | v0.1 (monitor en vivo) | Dev / dueño del proyecto |
| PA-02 | ✅ Resuelta | ¿Se permite vaciar la Papelera como única acción permanente (con doble confirmación) o la app solo la abre? → **Sí, con doble confirmación; nunca automática.** | v0.1 (limpieza) | Dueño del proyecto |
| PA-03 | Alta | ¿Cómo detectar de forma fiable apps "sin uso"? (Prefetch/UserAssist/last access son señales débiles; Prefetch puede estar deshabilitado en SSD) | v0.2 | Dev |
| PA-04 | Media | Pesos finales del health score (SU-01) y curva de cada subpuntaje | v0.2 | Dueño del proyecto |
| PA-05 | Media | Umbrales y cooldown por defecto de las alertas (SU-02) | v0.3 | Dueño del proyecto |
| PA-06 | Media | ¿Los escaneos programados corren con la app cerrada (Task Scheduler lanzando un modo headless) o solo mientras la bandeja está viva? | v0.3 | Dev / dueño |
| PA-07 | Media | Generación de PDF: imprimir desde el webview (WebView2 PrintToPdf) vs. crate Rust (ej. `printpdf`/`genpdf`/headless) | v0.3 | Dev |
| PA-08 | Media | Profundidad de soporte de Windows 10 (¿pruebas en CI/VM? ¿APIs que difieren?) | v0.1 (alcance de pruebas) | Dueño del proyecto |
| PA-09 | Baja | Firma de código: ¿cuándo y con qué certificado (Azure Trusted Signing, EV, OV)? | Release 1.0 | Dueño del proyecto |
| PA-10 | Baja | i18n: ¿se agregará inglés u otros idiomas? ¿librería (i18next) o catálogo propio? | Post v0.3 | Dueño del proyecto |
| PA-11 | Baja | Auto-update: ¿cuándo y con qué mecanismo sin romper el principio de "sin red"? | Post v0.3 | Dueño del proyecto |
| PA-12 | ✅ Resuelta (v0.1) | ¿Se soporta más de un disco/volumen en el análisis y la limpieza desde v0.1, o solo `C:`? → **v0.1: solo volumen del sistema para limpieza; otros volúmenes protegidos. Análisis de solo lectura de otros volúmenes y limpieza multi-volumen se reevalúan en una versión posterior — decidido por el dueño 2026-11-13.** | v0.1 | Dueño del proyecto |
| PA-13 | Baja | Política de retención final de `scan`, `cleanup_log` y `alert` (hoy **Suposición**) | v0.2 | Dev |
| PA-15 | Media | Ubicaciones de `old_logs`: las típicas (`C:\Windows\Logs\CBS`, `DISM`, `Panther`, `System32\LogFiles`) están protegidas por `path-protection` (C-03). ¿Qué ubicaciones concretas se habilitan y con qué excepciones nuevas? Hasta entonces la categoría queda sin ubicaciones ("No disponible en esta versión") | v0.1 (post C-08) | Dueño del proyecto |
| PA-14 | ✅ Resuelta | Detección over-the-shoulder: ¿bloqueo total o solo de las categorías por-usuario? (hoy: solo por-usuario) → **Bloqueo solo de categorías por-usuario, con aviso explicativo.** | v0.1 | Dueño del proyecto |

# UI: Modos Simple y Avanzado

Gobernanza: **LOW** (UI, catálogos, tipos).

## Principios

- Un solo backend y un solo set de reglas; los modos cambian **presentación y nivel de detalle**, nunca las protecciones.
- Estética oscura única en ambos modos.
- Textos en español, centralizados en un catálogo (`src/shared/i18n`) para i18n futura (RN-GLB-03).
- Explicar antes de actuar: cada acción muestra qué hará, cuánto afecta y cómo deshacerla.

## Modo simple (por defecto)

- **Health score** 0-100 grande, con color/etiqueta ("Excelente", "Mejorable", "Atención").
- **Tarjetas grandes** por tema: Espacio, Basura, Inicio, Memoria, Salud del disco.
- **Una acción primaria por pantalla** (ej. "Limpiar ahora").
- Explicaciones en lenguaje llano ("Estos archivos son temporales y se pueden borrar sin problema").
- Previews resumidos por categoría (cantidad y tamaño), con opción "ver detalle".
- Sin rutas, hashes ni métricas crudas salvo bajo "ver detalle".
- Mejoras con riesgo e impacto en palabras, y botón de deshacer visible.

## Modo avanzado (Configuración → Modo avanzado)

- **Tema oscuro denso** con **sidebar** de navegación.
- **Gauges** de CPU/RAM/disco/temperaturas, **gráficos en vivo** (muestreo 1-2 s) y historial 24 h.
- **Métricas crudas**, lista de procesos, rutas completas, hashes (blake3) de duplicados, detalle de reglas y snapshots.
- Editor de reglas de limpieza personalizadas y umbrales de alertas.
- Tablas ordenables/filtrables; copiar rutas; revelar en Explorador.

## Mapa de pantallas

| Pantalla | Simple | Avanzado | Hito |
|----------|--------|----------|------|
| Inicio / Dashboard | Score + tarjetas | Gauges + gráficos + sidebar | v0.1 / v0.2 |
| Espacio en disco | Resumen + top 5 | Treemap/lista completa, top archivos | v0.1 |
| Limpieza | Tarjetas por categoría + preview | Rutas, categorías, log | v0.1 |
| Duplicados | Grupos con elección guiada | Hashes, rutas, filtros | v0.1 |
| Inicio de Windows | Lista con impacto | Detalle de entradas | v0.2 |
| Mejoras | Cartas con riesgo/impacto + undo | Snapshots, historial | v0.2 |
| Monitor | Resumen | Gráficos en vivo, procesos | v0.1 |
| Alertas y asistente | Activar/desactivar | Umbrales, cooldown, schedules, reglas | v0.3 |
| Reportes / Benchmark | Botón exportar | Opciones y detalle | v0.3 |
| Configuración | Modo, autostart | Todo lo anterior | v0.1 |

## Accesibilidad y consistencia

- **Suposición:** apuntar a WCAG 2.2 AA (contraste en tema oscuro, foco visible, navegación por teclado, `prefers-reduced-motion`).
- No usar solo color para indicar estado (icono + texto).
- Mensajes de error accionables ("No se pudo borrar porque el archivo está en uso; se omitió").
- Estados "no disponible" explícitos (temperatura de CPU).
- Revisión de usabilidad pendiente para IN-01 (ver 10).

## Notas técnicas

- El modo se guarda en `settings.ui_mode`; el cambio no requiere reinicio.
- Componentes compartidos con variantes `simple`/`advanced` en vez de pantallas duplicadas (**Suposición**).
- Librería de gráficos: pendiente (PA-01). Requisito: render eficiente de series en vivo y tema oscuro.
- Drag & drop desde el Explorador no funciona hacia ventanas elevadas (UIPI): ofrecer selector de carpeta.

# Funcionalidades

Organizadas por épica y luego por historia (US-NNN). Cada historia lleva **hito** (v0.1/v0.2/v0.3) y **gobernanza** (HIGH/MEDIUM/LOW; ver 08).

## Épica 1: Shell y dashboard

### US-001 — Dashboard en modo simple
**Hito**: v0.1 (health score completo en v0.2) · **Gobernanza**: LOW
**Como** usuario común **quiero** ver el estado de mi PC en tarjetas grandes con una acción primaria **para** entender qué hacer sin conocimientos técnicos.

**Criterios de aceptación**:
- [ ] La app arranca en modo simple.
- [ ] Una acción primaria por pantalla (C-12: Inicio tiene exactamente una, "Limpiar ahora"; ninguna otra pantalla tiene más de una).
- [ ] Textos en lenguaje llano, desde el catálogo de strings.
- [ ] Inicio muestra tres tarjetas (Espacio, Basura, Memoria) con cifra, frase llana y "Ver detalle", y estados explícitos "No disponible" / "Midiendo…" / "Todavía no buscaste basura" en lugar de ceros.
- [ ] "Limpiar ahora" lleva a Limpieza y abre el preview (busca antes si el último escaneo tiene 60 min o más); nunca borra por sí sola. Se deshabilita con motivo visible si hay otro trabajo en curso.
- [ ] Pantalla Historial: limpiezas en frase llana, sin rutas ni códigos.

**Reglas relacionadas**: RN-GLB-03

### US-002 — Dashboard en modo avanzado
**Hito**: v0.1 · **Gobernanza**: LOW
**Como** usuario avanzado **quiero** activar un dashboard denso (sidebar, gauges, gráficos, métricas crudas, rutas, hashes) **para** tener control y detalle.

- [ ] Toggle en Configuración, persistido en `settings`.
- [ ] Mismo tema oscuro en ambos modos.
- [ ] Inicio avanzado (C-12): medidores y un gráfico de CPU/memoria de 5 min, tabla de basura por categoría con códigos, resumen del último análisis de disco con su ruta y las 3 últimas limpiezas.
- [ ] Historial avanzado: conteos por resultado con código y tabla paginada de ítems con ruta completa (rutas ocultas si la app corre con otra cuenta).

**Reglas relacionadas**: RN-GLB-03

## Épica 2: Monitoreo

### US-003 — Monitor en vivo CPU/RAM/disco
**Hito**: v0.1 · **Gobernanza**: MEDIUM
**Como** usuario **quiero** ver uso de CPU, RAM y disco en vivo **para** detectar qué satura mi equipo.

- [ ] Muestreo cada 1 s en un hilo propio, siempre activo desde el arranque; ventana de 5 min en memoria, sin persistir (C-06).
- [ ] Valores en vivo de CPU, memoria y disco del sistema (tiempo activo, lectura/escritura y espacio); una métrica que no se puede medir se muestra como "No disponible", nunca como 0.
- [ ] Modo simple: tres tarjetas con nivel en palabras; modo avanzado: medidores, cuatro gráficos de 5 min y detalles de disponibilidad.
- [ ] Tras una suspensión el hueco se ve como corte, sin ceros.
- [ ] Sin impacto perceptible: el proceso con la pantalla cerrada promedia < 1 % de la CPU total y cada lectura cuesta <= 5 ms de media (medido en el smoke, RN-MON-06).

**Reglas relacionadas**: RN-MON-01, RN-MON-06

### US-004 — Temperaturas y SMART
**Hito**: v0.2 · **Gobernanza**: MEDIUM
**Como** usuario **quiero** ver temperaturas y estado SMART **para** anticipar problemas de hardware.

- [ ] Muestra "no disponible" cuando no hay sensor accesible.
- [ ] Sin drivers de kernel.
- [ ] Estado SMART crítico genera alerta.

**Reglas relacionadas**: RN-MON-04, RN-MON-05, RN-SAL-04

### US-005 — Historial de 24 h
**Hito**: v0.2 · **Gobernanza**: MEDIUM
**Como** usuario avanzado **quiero** ver gráficos de las últimas 24 h **para** correlacionar picos con eventos.

- [ ] Agregados por minuto persistidos y purgados a las 24 h.

**Reglas relacionadas**: RN-MON-02

## Épica 3: Análisis de disco

### US-006 — Uso de disco por carpeta
**Hito**: v0.1 · **Gobernanza**: MEDIUM
**Como** usuario **quiero** ver qué carpetas ocupan más (treemap/lista) **para** saber qué llena mi disco.

- [ ] Recorrido sin seguir junctions/symlinks.
- [ ] Progreso y cancelación.
- [ ] Carpetas inaccesibles se reportan, no rompen el escaneo.

**Reglas relacionadas**: RN-PROT-04

### US-007 — Top de archivos pesados
**Hito**: v0.1 · **Gobernanza**: MEDIUM
**Como** usuario **quiero** una lista de los archivos más grandes **para** decidir qué revisar.

- [ ] Ordenable por tamaño y fecha.
- [ ] Acción solo de revelar en Explorador / enviar a revisión de limpieza (no borra directo salvo flujo con preview).

**Reglas relacionadas**: RN-LIM-03, RN-LIM-04

## Épica 4: Limpieza

### US-008 — Detección de basura
**Hito**: v0.1 · **Gobernanza**: HIGH
**Como** usuario **quiero** que la app detecte basura conocida **para** liberar espacio sin riesgo.

- [ ] Solo categorías whitelisted.
- [ ] Muestra tamaño por categoría.
- [ ] Nada en rutas protegidas.

**Reglas relacionadas**: RN-LIM-02, RN-PROT-01, RN-PROT-02

### US-009 — Limpieza con preview y log
**Hito**: v0.1 · **Gobernanza**: HIGH
**Como** usuario **quiero** previsualizar y confirmar antes de limpiar **para** no perder nada importante.

- [ ] Preview con cantidad y tamaño totales.
- [ ] Confirmación explícita.
- [ ] Todo va a la Papelera.
- [ ] Archivos en uso se omiten y reportan.
- [ ] Cada ítem queda en `cleanup_log`.

**Reglas relacionadas**: RN-LIM-01, RN-LIM-03, RN-LIM-04, RN-LIM-05, RN-LIM-07

### US-010 — Vaciar Papelera
**Hito**: v0.1 · **Gobernanza**: HIGH
**Como** usuario **quiero** vaciar la Papelera con doble confirmación **para** recuperar el espacio realmente.

- [ ] Doble confirmación propia: dos diálogos distintos; el segundo exige marcar una casilla "Entiendo que no se puede deshacer" (C-10).
- [ ] Cantidad y tamaño medidos visibles en ambos pasos; si la Papelera cambia entre pasos, no se vacía y se vuelve a revisar.
- [ ] Bloqueada en `mismatch` (over-the-shoulder), con aviso.
- [ ] Registro en el log (`recycle_bin_emptied`, o `error` si falló; `partial`/`unverified` en el detalle).

**Reglas relacionadas**: RN-LIM-06

### US-011 — Duplicados
**Hito**: v0.1 · **Gobernanza**: HIGH
**Como** usuario **quiero** ver grupos de duplicados y elegir cuál conservar **para** liberar espacio sin perder datos.

- [ ] Etapas tamaño → hash parcial → blake3, solo en las 6 carpetas de contenido del usuario, sin seguir enlaces ni leer contenido en la nube.
- [ ] Nunca auto-borrado ni copia preseleccionada o recomendada; al menos una copia queda (la conservada se re-verifica por contenido antes de mover las demás).
- [ ] Borrado por flujo de limpieza estándar (`run_cleanup`, solo Papelera) con re-verificación del contenido de cada copia.

**Reglas relacionadas**: RN-DUP-01..06

### US-012 — Reglas de limpieza personalizadas
**Hito**: v0.3 · **Gobernanza**: HIGH
**Como** usuario avanzado **quiero** definir patrones de ruta con filtros de antigüedad/tamaño **para** limpiar mis propias carpetas temporales.

- [ ] Rechaza reglas que toquen rutas protegidas.
- [ ] Preview obligatorio.

**Reglas relacionadas**: RN-LIM-08, RN-PROT-06

## Épica 5: Sistema y mejoras

### US-013 — Programas de inicio
**Hito**: v0.2 · **Gobernanza**: HIGH
**Como** usuario **quiero** ver y deshabilitar/habilitar programas de inicio **para** acelerar el arranque.

- [ ] Muestra impacto y riesgo.
- [ ] Reversible con snapshot.

**Reglas relacionadas**: RN-MEJ-01..04

### US-014 — Apps sin uso
**Hito**: v0.2 · **Gobernanza**: MEDIUM
**Como** usuario **quiero** que se me sugieran programas que no uso **para** decidir si desinstalarlos.

- [ ] Detección por claves Uninstall + señal de último uso best-effort.
- [ ] "Desinstalar" abre el desinstalador del programa.

**Reglas relacionadas**: RN-MEJ-06

### US-015 — Mejoras de 1 click con undo
**Hito**: v0.2 · **Gobernanza**: HIGH
**Como** usuario **quiero** aplicar mejoras con riesgo e impacto visibles y poder deshacerlas **para** probar sin miedo.

- [ ] Lista cerrada (inicio, plan de energía, efectos visuales, limpieza whitelisted).
- [ ] Snapshot previo, undo exacto.

**Reglas relacionadas**: RN-MEJ-01..05

### US-016 — Health score
**Hito**: v0.2 · **Gobernanza**: MEDIUM
**Como** usuario común **quiero** un puntaje 0-100 con explicación **para** saber cómo está mi PC de un vistazo.

- [ ] Muestra qué factores lo bajan.
- [ ] Indica métricas no disponibles.

**Reglas relacionadas**: RN-SAL-01..06

## Épica 6: Asistente continuo

### US-017 — Modo bandeja y autostart
**Hito**: v0.3 · **Gobernanza**: HIGH
**Como** usuario **quiero** que la app quede en la bandeja y opcionalmente arranque con Windows **para** monitoreo continuo.

- [ ] Autostart opt-in vía Task Scheduler con privilegios altos.
- [ ] Cerrar ventana minimiza a bandeja.

**Reglas relacionadas**: RN-PRIV-01, RN-PRIV-02

### US-018 — Escaneos programados
**Hito**: v0.3 · **Gobernanza**: MEDIUM
**Como** usuario **quiero** escaneos diarios/semanales **para** enterarme de problemas sin acordarme.

- [ ] Solo detectan y notifican.

**Reglas relacionadas**: RN-ALE-04

### US-019 — Alertas
**Hito**: v0.3 · **Gobernanza**: MEDIUM
**Como** usuario **quiero** alertas de disco > 90 %, temperatura alta y RAM saturada **para** actuar a tiempo.

- [ ] Umbrales configurables.
- [ ] Cooldown anti-spam.

**Reglas relacionadas**: RN-ALE-01..05

### US-020 — Tendencias semanales
**Hito**: v0.3 · **Gobernanza**: MEDIUM
**Como** usuario avanzado **quiero** ver qué carpeta crece más **para** anticipar falta de espacio.

- [ ] Usa snapshots periódicos y agregados horarios (~90 días).

**Reglas relacionadas**: RN-MON-02, RN-MON-03

## Épica 7: Rendimiento y reportes

### US-021 — Benchmark antes/después
**Hito**: v0.3 · **Gobernanza**: MEDIUM
**Como** usuario **quiero** medir mi PC antes y después de optimizar **para** ver si sirvió.

- [ ] Disco lectura/escritura, CPU single loop, apps de inicio.

**Reglas relacionadas**: RN-REP-02

### US-022 — Exportar reporte HTML/PDF
**Hito**: v0.3 · **Gobernanza**: LOW
**Como** usuario **quiero** exportar un reporte **para** guardarlo o compartirlo.

- [ ] HTML y PDF generados localmente.

**Reglas relacionadas**: RN-REP-01, RN-REP-03, RN-REP-04

## Épica 8: Privilegios y configuración

### US-023 — Arranque elevado y verificación de perfil
**Hito**: v0.1 · **Gobernanza**: HIGH
**Como** usuario **quiero** que la app detecte si corre bajo otra cuenta **para** no limpiar el perfil equivocado.

- [ ] Compara usuario del token vs sesión interactiva.
- [ ] Si difieren: advierte y bloquea limpieza por-usuario.

**Reglas relacionadas**: RN-PRIV-01, RN-PRIV-03

### US-024 — Configuración
**Hito**: v0.1 (umbrales en v0.3) · **Gobernanza**: LOW
**Como** usuario **quiero** configurar modo, autostart y umbrales **para** adaptar la app.

- [ ] Persistido en `settings`.

**Reglas relacionadas**: RN-GLB-03

## Nota C-07: criterios reales de US-006 / US-007

US-006: lista navegable con barras proporcionales (sin treemap; diferido), hasta 20 000 carpetas guardadas, carpetas sin acceso informadas sin romper el análisis. US-007: los 500 archivos más grandes, ordenables por tamaño y fecha dentro de ese top. Acciones: "Mostrar en el Explorador" y, para ubicaciones de la whitelist, "Revisar en Limpieza" (nunca borra).

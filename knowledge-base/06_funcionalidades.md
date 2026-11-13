# Funcionalidades

Organizadas por épica y luego por historia (US-NNN). Cada historia lleva **hito** (v0.1/v0.2/v0.3) y **gobernanza** (HIGH/MEDIUM/LOW; ver 08).

## Épica 1: Shell y dashboard

### US-001 — Dashboard en modo simple
**Hito**: v0.1 (health score completo en v0.2) · **Gobernanza**: LOW
**Como** usuario común **quiero** ver el estado de mi PC en tarjetas grandes con una acción primaria **para** entender qué hacer sin conocimientos técnicos.

**Criterios de aceptación**:
- [ ] La app arranca en modo simple.
- [ ] Una acción primaria por pantalla.
- [ ] Textos en lenguaje llano, desde el catálogo de strings.

**Reglas relacionadas**: RN-GLB-03

### US-002 — Dashboard en modo avanzado
**Hito**: v0.1 · **Gobernanza**: LOW
**Como** usuario avanzado **quiero** activar un dashboard denso (sidebar, gauges, gráficos, métricas crudas, rutas, hashes) **para** tener control y detalle.

- [ ] Toggle en Configuración, persistido en `settings`.
- [ ] Mismo tema oscuro en ambos modos.

**Reglas relacionadas**: RN-GLB-03

## Épica 2: Monitoreo

### US-003 — Monitor en vivo CPU/RAM/disco
**Hito**: v0.1 · **Gobernanza**: MEDIUM
**Como** usuario **quiero** ver uso de CPU, RAM y disco en vivo **para** detectar qué satura mi equipo.

- [ ] Muestreo cada 1-2 s.
- [ ] Sin impacto perceptible en el rendimiento.

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
**Hito**: v0.1 (**Suposición**; sujeto a pregunta abierta) · **Gobernanza**: HIGH
**Como** usuario **quiero** vaciar la Papelera con doble confirmación **para** recuperar el espacio realmente.

- [ ] Doble confirmación propia.
- [ ] Registro en el log.

**Reglas relacionadas**: RN-LIM-06

### US-011 — Duplicados
**Hito**: v0.1 · **Gobernanza**: HIGH
**Como** usuario **quiero** ver grupos de duplicados y elegir cuál conservar **para** liberar espacio sin perder datos.

- [ ] Etapas tamaño → hash parcial → blake3.
- [ ] Nunca auto-borrado; al menos una copia queda.
- [ ] Borrado por flujo de limpieza estándar.

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

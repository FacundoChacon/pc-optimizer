# Reglas de Negocio

Cada regla tiene un código único `RN-{DOMINIO}-{NN}`. Las reglas de LIMPIEZA y PROTECCION son de gobernanza **HIGH**: cualquier cambio requiere revisión explícita y tests.

## Dominio: Limpieza (RN-LIM)

- **RN-LIM-01**: Nunca se borra de forma permanente. Todo borrado = mover a la Papelera (crate `trash`). Única excepción: vaciar la Papelera (RN-LIM-06).
- **RN-LIM-02**: La limpieza actúa solo sobre una **whitelist** de categorías: `%TEMP%` del usuario, `Windows\Temp`, cachés de Chrome/Edge/Firefox/Brave, logs viejos, caché de Windows Update (`SoftwareDistribution\Download`), caché de miniaturas, volcados de fallos (crash dumps), y archivos que matcheen reglas personalizadas (RN-LIM-08).
- **RN-LIM-03**: Es obligatorio un **preview** antes de limpiar: lista de ítems con cantidad total y tamaño total. Sin preview no hay confirmación posible.
- **RN-LIM-04**: Se requiere **confirmación explícita** del usuario mostrando cantidad y tamaño. La confirmación aplica solo al preview exacto mostrado (si cambia el conjunto, se recalcula).
- **RN-LIM-05**: Archivos en uso/bloqueados se **omiten y reportan**; nunca se fuerzan (no kill de procesos, no reintentos al reiniciar).
- **RN-LIM-06**: Vaciar la Papelera es la única acción "permanente" y exige **doble confirmación** propia. **Pregunta abierta** (ver 10): si se permite o se deja solo "abrir Papelera".
- **RN-LIM-07**: Cada limpieza se registra en `cleanup_log` (qué, tamaño, cuándo, categoría, resultado). Registro append-only.
- **RN-LIM-08**: Reglas personalizadas (patrón de ruta + antigüedad/tamaño) están sujetas siempre a la lista de rutas protegidas; una regla que la viole se rechaza al guardarse y se vuelve a verificar al ejecutarse.
- **RN-LIM-09**: Archivos "cloud-only" / placeholders de OneDrive nunca se tocan (RN-PROT-05).

## Dominio: Protección (RN-PROT)

- **RN-PROT-01**: Rutas nunca tocadas: `C:\Windows` (excepto subrutas explícitas `Windows\Temp` y `SoftwareDistribution\Download`), `Program Files`, `Program Files (x86)`, `ProgramData` (excepto cachés whitelisted), `System32`, perfiles de otros usuarios.
- **RN-PROT-02**: La verificación de ruta protegida se hace sobre la ruta **canónica** (resolviendo `..`, symlinks, junctions, mayúsculas, prefijos `\\?\`) antes de cada operación.
- **RN-PROT-03**: Se evalúa la protección **en el momento de ejecutar** (no solo al escanear) para evitar carreras (TOCTOU).
- **RN-PROT-04**: Los junctions/symlinks no se siguen durante el recorrido para evitar escapar de la whitelist (**Suposición**: no se siguen en v0.x).
- **RN-PROT-05**: Todo archivo sincronizado con la nube (OneDrive u otro proveedor) se omite y se reporta como `skipped_cloud`, sea placeholder (atributos `RECALL_ON_DATA_ACCESS` / `RECALL_ON_OPEN` / `OFFLINE`) o esté hidratado (tag de reanálisis de proveedor de nube), porque borrarlo se propaga a la nube y a otros dispositivos (decidido por el dueño 2026-11-13).
- **RN-PROT-06**: La lista de rutas protegidas es código de dominio puro, con tests de tabla; no es editable por el usuario ni por reglas personalizadas.
- **RN-PROT-07**: Ante duda (ruta no clasificable, error de canonicalización), la política es **no tocar**.

## Dominio: Duplicados (RN-DUP)

- **RN-DUP-01**: Detección en tres etapas: agrupar por tamaño → hash parcial (primeros/últimos 64 KB) → hash completo blake3.
- **RN-DUP-02**: Solo se consideran duplicados los que coinciden en hash completo.
- **RN-DUP-03**: Nunca se auto-borran duplicados; el usuario elige cuál copia conservar en cada grupo.
- **RN-DUP-04**: Debe quedar al menos una copia por grupo; la UI no permite marcar todas para borrar.
- **RN-DUP-05**: El borrado de duplicados sigue RN-LIM (Papelera, preview, confirmación, log) y RN-PROT.
- **RN-DUP-06**: Los duplicados no se buscan dentro de rutas protegidas. **Suposición:** además se excluyen archivos < 1 MB por defecto (configurable en avanzado).

## Dominio: Mejoras (RN-MEJ)

- **RN-MEJ-01**: Solo existe una **lista cerrada** de mejoras reversibles: habilitar/deshabilitar programas de inicio, cambiar plan de energía, preset de efectos visuales, ejecutar una limpieza whitelisted. Sin servicios, sin tweaks genéricos de registro.
- **RN-MEJ-02**: Antes de aplicar se guarda un **snapshot** del estado previo en SQLite; si falla guardarlo, no se aplica.
- **RN-MEJ-03**: Cada mejora declara nivel de riesgo (bajo/medio/alto) e impacto estimado, mostrados antes de aplicar.
- **RN-MEJ-04**: El undo restaura el estado exacto del snapshot; si el estado actual cambió desde la aplicación, se avisa y se pide confirmar.
- **RN-MEJ-05**: La mejora "ejecutar limpieza" no es reversible por snapshot; su reversión es restaurar desde la Papelera (se informa al usuario). **Suposición**.
- **RN-MEJ-06**: Apps sin uso: solo se **sugieren**. "Desinstalar" lanza el desinstalador propio del programa; nunca desinstalación silenciosa.

## Dominio: Salud (RN-SAL)

- **RN-SAL-01**: Health score entero 0-100, calculado como suma ponderada de subpuntajes (cada uno 0-100) de: % libre de disco, tamaño de basura, cantidad de programas de inicio, presión de RAM, estado SMART, temperaturas (si hay).
- **RN-SAL-02**: Pesos por defecto — **Suposición** (ajustables, ver 10): disco libre 30 %, basura 15 %, inicio 15 %, RAM 20 %, SMART 10 %, temperaturas 10 %.
- **RN-SAL-03**: Si una métrica no está disponible (ej. temperatura), se **excluye y se renormalizan** los pesos restantes; la UI indica qué métricas no entraron.
- **RN-SAL-04**: SMART con estado crítico/predictivo de falla fuerza un tope al score (**Suposición:** máx. 40) y una alerta destacada.
- **RN-SAL-05**: La función de score es pura y determinista (mismos inputs → mismo score); versionada para comparar históricos.
- **RN-SAL-06**: El score se explica: la UI muestra qué factores lo bajan y qué acción sugerida mejora cada uno.

## Dominio: Monitoreo (RN-MON)

- **RN-MON-01**: Muestreo en vivo cada 1-2 s, solo en memoria.
- **RN-MON-02**: Se persisten agregados por minuto durante 24 h y agregados por hora durante ~90 días.
- **RN-MON-03**: Snapshots periódicos de tamaño de carpetas alimentan "qué carpeta crece más" (tendencia semanal).
- **RN-MON-04**: Temperaturas best-effort sin driver: NVMe/SMART disco, GPU vía NVML/ADLX, zona térmica ACPI vía WMI. Si no hay dato: "no disponible". Temperatura de CPU frecuentemente no estará disponible y se documenta honestamente en la UI.
- **RN-MON-05**: Nunca se instalan ni cargan drivers de kernel para obtener métricas.
- **RN-MON-06**: El muestreo no debe degradar el sistema (**Suposición:** presupuesto < 1 % CPU); en bandeja el intervalo puede relajarse.

## Dominio: Alertas (RN-ALE)

- **RN-ALE-01**: Alertas por defecto (umbrales **Suposición**, configurables): disco > 90 % usado, temperatura alta, RAM saturada.
- **RN-ALE-02**: **Cooldown anti-spam** por regla: tras disparar, no se repite hasta pasado `cooldown_s` o hasta que la condición se normalice y reaparezca.
- **RN-ALE-03**: Una alerta es solo informativa: nunca ejecuta acciones destructivas por sí misma.
- **RN-ALE-04**: Escaneos programados (diario/semanal) solo detectan y notifican; la limpieza requiere confirmación del usuario (RN-LIM-03/04).
- **RN-ALE-05**: Alertas se guardan en `alert` con reconocimiento (`acknowledged`).

## Dominio: Privilegios (RN-PRIV)

- **RN-PRIV-01**: La app corre siempre elevada (`requireAdministrator`); UAC en cada arranque.
- **RN-PRIV-02**: Autostart de bandeja es **opt-in** y se implementa con una tarea de Task Scheduler "run with highest privileges" (la clave Run del registro no puede elevar).
- **RN-PRIV-03**: Se detecta over-the-shoulder comparando el usuario del token elevado con el del usuario de la sesión interactiva; si difieren, se advierte y se **bloquea la limpieza por-usuario** (`%TEMP%`, cachés de navegador, Papelera, HKCU).
- **RN-PRIV-04**: Nunca drivers de kernel (WinRing0 rechazado: Defender lo marca como vulnerable).
- **RN-PRIV-05**: La elevación no amplía la whitelist: ser admin no habilita tocar rutas protegidas.
- **RN-PRIV-06**: Se documenta que UIPI bloquea drag & drop desde el Explorador hacia una ventana elevada.

## Dominio: Reportes (RN-REP)

- **RN-REP-01**: Se puede exportar un reporte en HTML y PDF con: score, hallazgos, limpiezas realizadas, mejoras aplicadas, tendencias.
- **RN-REP-02**: Benchmark liviano antes/después: throughput de lectura/escritura de disco, loop CPU single-core, cantidad de apps de inicio.
- **RN-REP-03**: Los reportes se generan localmente y se guardan donde el usuario elija; nada se envía por red.
- **RN-REP-04**: El reporte omite por defecto información sensible (**Suposición:** rutas completas con nombre de usuario se ofuscan salvo que el usuario lo desactive).

## Dominio: Excepciones globales

- **RN-GLB-01**: Ante conflicto entre reglas, prevalece la más restrictiva (seguridad > conveniencia).
- **RN-GLB-02**: Sin red: ninguna funcionalidad puede depender de conectividad ni enviar datos.
- **RN-GLB-03**: Todo texto de UI sale de un catálogo centralizado (preparado para i18n; ver 12).
- **RN-GLB-04**: Gobernanza: borrado y tweaks del sistema = HIGH; scanners/monitor = MEDIUM; UI/catálogos/tipos = LOW (ver 08).

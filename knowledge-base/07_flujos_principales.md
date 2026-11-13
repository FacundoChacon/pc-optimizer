# Flujos Principales

Cada flujo está documentado extremo a extremo. Actores técnicos: UI (React), IPC (comandos Tauri), Dominio (Rust puro), Adaptadores (FS/Trash/SQLite/Win32).

## Flujo 1: Primer arranque, UAC y verificación de perfil

**Disparador**: el usuario abre la app.
**Actor**: Usuario / Windows.

**Pasos**:
1. Windows muestra UAC (manifest `requireAdministrator`).
2. Si el usuario cancela, la app no inicia.
3. Backend consulta el contexto de privilegios: usuario del token elevado vs usuario de la sesión interactiva.
4. Si coinciden, se continúa; si difieren (over-the-shoulder), se muestra advertencia y se bloquea la limpieza por-usuario.
5. Primer arranque: se crean DB y settings seed, UI en modo simple.

```mermaid
sequenceDiagram
    actor U as Usuario
    participant W as Windows (UAC)
    participant B as Backend Rust
    participant UI as UI React
    participant DB as SQLite
    U->>W: Abre pc-optimizer
    W-->>U: Prompt UAC
    U->>W: Aprueba
    W->>B: Proceso elevado
    B->>B: token_user vs session_user
    B->>DB: Migraciones + seed (si primer arranque)
    B-->>UI: privilege_context {match | mismatch}
    alt mismatch
        UI-->>U: Advertencia, limpieza por-usuario bloqueada
    else match
        UI-->>U: Dashboard modo simple
    end
```

**Casos de error**:
- UAC cancelado → la app no arranca (sin degradación a modo no elevado).
- No se puede determinar el usuario de sesión → **Suposición:** se trata como mismatch (política de no tocar).
- DB corrupta o migración fallida → mensaje claro, se ofrece respaldo y recreación.

## Flujo 2: Escaneo → revisión → preview → confirmación → Papelera → log

**Disparador**: el usuario pulsa "Buscar basura" (o "Analizar").
**Actor**: Usuario.

**Pasos**:
1. UI invoca `start_scan`; backend recorre solo categorías whitelisted (sin seguir junctions).
2. Cada candidato pasa por `is_protected(path)` sobre ruta canónica.
3. Se emiten eventos de progreso; resultados en `junk_finding`.
4. Usuario revisa hallazgos (modo simple: tarjetas por categoría; avanzado: rutas).
5. UI pide `preview_cleanup`: dominio devuelve ítems, cantidad y tamaño totales.
6. Usuario confirma explícitamente.
7. `confirm_cleanup` re-verifica protección (anti-TOCTOU) y mueve cada ítem a la Papelera; omite bloqueados/cloud-only.
8. Se escribe `cleanup_log` y se muestra resumen (liberado, omitidos).

```mermaid
sequenceDiagram
    actor U as Usuario
    participant UI as UI
    participant C as commands
    participant D as domain
    participant FS as adapters fs/trash
    participant DB as SQLite
    U->>UI: Buscar basura
    UI->>C: start_scan
    C->>FS: recorrer categorías whitelist
    FS-->>D: candidatos
    D->>D: clasificar + is_protected
    D-->>DB: junk_finding
    C-->>UI: resultados
    U->>UI: Limpiar
    UI->>C: preview_cleanup
    C->>D: armar preview (n ítems, bytes)
    C-->>UI: preview
    U->>UI: Confirmar
    UI->>C: confirm_cleanup(preview_id)
    loop cada ítem
        C->>D: re-verificar protección
        C->>FS: mover a Papelera
        FS-->>C: ok / bloqueado / error
        C->>DB: cleanup_log
    end
    C-->>UI: resumen
```

**Casos de error**:
- Archivo en uso → `skipped_locked`, se reporta, no se fuerza.
- Ruta resultó protegida al re-verificar → `skipped_protected`.
- Placeholder OneDrive → `skipped_cloud`.
- Permiso denegado/IO error → `error` con detalle; el resto continúa.
- Usuario cierra el preview → no se ejecuta nada.

## Flujo 3: Aplicar mejora → snapshot → undo

**Disparador**: el usuario pulsa "Aplicar" en una mejora.
**Actor**: Usuario.

**Pasos**:
1. UI muestra riesgo (bajo/medio/alto) e impacto estimado.
2. Usuario confirma.
3. Backend lee el estado actual y lo guarda en `improvement_snapshot`.
4. Solo si el snapshot se persistió, aplica el cambio.
5. Para deshacer: lee el snapshot, compara con estado actual, restaura estado exacto y marca `reverted_at`.

```mermaid
sequenceDiagram
    actor U as Usuario
    participant UI as UI
    participant C as commands
    participant A as adapters windows
    participant DB as SQLite
    U->>UI: Aplicar mejora
    UI-->>U: Riesgo + impacto
    U->>UI: Confirmar
    UI->>C: apply_improvement(id)
    C->>A: leer estado actual
    C->>DB: guardar improvement_snapshot
    DB-->>C: ok
    C->>A: aplicar cambio
    C-->>UI: aplicada
    U->>UI: Deshacer
    UI->>C: undo_improvement(id)
    C->>DB: leer snapshot
    C->>A: leer estado actual
    alt cambió desde la aplicación
        C-->>UI: aviso + pedir confirmación
    end
    C->>A: restaurar estado previo
    C->>DB: marcar reverted_at
    C-->>UI: revertida
```

**Casos de error**:
- Falla el guardado del snapshot → no se aplica.
- Falla la aplicación → se descarta/marca el snapshot, se informa.
- Estado externo cambió antes del undo → advertencia y confirmación (RN-MEJ-04).

## Flujo 4: Escaneo programado → alerta desde la bandeja

**Disparador**: llega la hora del `schedule` o una métrica supera un umbral.
**Actor**: Sistema/Scheduler.

**Pasos**:
1. El scheduler (dentro del proceso de bandeja) dispara un escaneo `trigger=scheduled`.
2. El monitor evalúa `alert_rule` contra las métricas agregadas.
3. Si se supera un umbral y no está en cooldown, se crea `alert` y se muestra notificación de bandeja.
4. Usuario hace click en la notificación; se abre la app (ya elevada) en la pantalla relevante.
5. Cualquier limpieza posterior sigue el Flujo 2.

```mermaid
sequenceDiagram
    participant S as Scheduler (tray)
    participant M as Monitor
    participant D as domain
    participant DB as SQLite
    participant T as Notificación tray
    actor U as Usuario
    S->>M: escaneo programado
    M->>D: evaluar reglas de alerta
    D->>DB: ¿cooldown vigente?
    alt umbral superado y sin cooldown
        D->>DB: insertar alert
        D->>T: notificar
        T-->>U: "Disco al 92 %"
        U->>T: click
        T->>U: abre app en pantalla relevante
    else en cooldown o normal
        D-->>M: sin acción
    end
```

**Casos de error**:
- App cerrada: según pregunta abierta, el escaneo no corre (o lo dispara Task Scheduler; ver 10).
- Sensor no disponible → la regla de temperatura no evalúa y no alerta.

## Flujo 5: Exportar reporte

**Disparador**: el usuario pulsa "Exportar reporte".
**Actor**: Usuario.

**Pasos**:
1. UI elige formato (HTML/PDF) y destino.
2. Backend recopila score, hallazgos, `cleanup_log`, mejoras, tendencias y benchmark.
3. Dominio genera el HTML (plantilla local).
4. PDF: según decisión abierta (impresión del webview vs crate Rust).
5. Se guarda el archivo local y se ofrece abrir la carpeta.

```mermaid
sequenceDiagram
    actor U as Usuario
    participant UI as UI
    participant C as commands
    participant DB as SQLite
    participant FS as adapters fs
    U->>UI: Exportar (HTML/PDF)
    UI->>C: export_report(format, dest)
    C->>DB: recopilar datos
    C->>C: render HTML (rutas ofuscadas por defecto)
    opt PDF
        C->>C: convertir a PDF
    end
    C->>FS: escribir archivo
    C-->>UI: ruta del archivo
```

**Casos de error**:
- Destino sin permisos / disco lleno → mensaje y reintento con otro destino.
- Sin datos suficientes (ej. sin benchmark) → la sección se omite, indicándolo.

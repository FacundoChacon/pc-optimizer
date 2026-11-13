# Arquitectura Propuesta

## Patrones aplicados

| Patrón | Dónde se usa | Por qué |
|--------|--------------|---------|
| Hexagonal (puertos y adaptadores) | `domain` define traits; `adapters` los implementa | Dominio puro testeable con Strict TDD; el SO queda aislado |
| Dominio puro (funciones sin efectos) | Reglas, scoring, protección de rutas, clasificación de basura, undo | Reglas críticas de seguridad verificables con tests de tabla |
| Fakes detrás de traits | Tests de FS, Papelera, registro, WMI | Probar borrado sin tocar el disco real |
| Command (IPC) | `src-tauri/src/commands` | Superficie mínima y validada entre webview y backend |
| Snapshot / Memento | `improvement_snapshot` | Undo exacto de mejoras |
| Append-only log | `cleanup_log` | Auditoría de lo borrado |
| Feature folders | `src/features/*` en React | Cohesión por funcionalidad |
| Event stream | Métricas en vivo vía eventos Tauri | Evita polling desde la UI |
| Política de "ante duda no tocar" | Protección de rutas | Seguridad > conveniencia |

## Gobernanza por nivel de criticidad

| Nivel | Módulos | Comportamiento del agente/equipo |
|-------|---------|-----------------------------------|
| **HIGH** | Borrado de archivos (cleanup, duplicados), protección de rutas, tweaks del sistema (mejoras, startup, power plan), manejo de privilegios | Proponer y esperar revisión antes de escribir; tests obligatorios; revisión de PR cuidadosa |
| **MEDIUM** | Scanners, monitor, scheduler, alertas, benchmark, health score | Implementar con checkpoints; sacar decisiones no obvias a revisión |
| **LOW** | UI, catálogos de strings, tipos, exportación de reportes | Autonomía total si pasan los tests |

## Estructura de directorios

```
pc-optimizer/
├── src-tauri/
│   ├── Cargo.toml
│   ├── tauri.conf.json              # CSP estricta, bundle NSIS/MSI
│   ├── capabilities/                # permisos Tauri mínimos
│   ├── pc-optimizer.exe.manifest    # requireAdministrator
│   └── src/
│       ├── main.rs / lib.rs
│       ├── domain/                  # PURO, sin I/O
│       │   ├── protection.rs        # rutas protegidas (canónicas)
│       │   ├── junk.rs              # clasificación whitelist
│       │   ├── cleanup.rs           # preview, plan, confirmación
│       │   ├── duplicates.rs        # etapas tamaño/parcial/completo
│       │   ├── health_score.rs
│       │   ├── improvements.rs      # catálogo cerrado + undo
│       │   ├── alerts.rs            # umbrales + cooldown
│       │   └── ports.rs             # traits (FileSystem, Trash, Registry, Sensors, Store...)
│       ├── adapters/
│       │   ├── windows/             # registry, wmi, privileges, task_scheduler, sensors
│       │   ├── fs/                  # walkdir, hashing blake3
│       │   ├── trash/               # crate trash
│       │   └── sqlite/              # rusqlite + migraciones
│       └── commands/                # handlers IPC finos
├── src/
│   ├── app/                         # shell, routing, providers
│   ├── features/
│   │   ├── dashboard/  disk/  cleanup/  duplicates/
│   │   ├── startup/  improvements/  monitor/
│   │   ├── alerts/  reports/  settings/
│   ├── shared/                      # ui kit, hooks, ipc client, i18n (strings es)
│   └── tests/                       # Vitest
├── knowledge-base/
├── openspec/
└── CHANGES.md                       # (se genera después)
```

## Seguridad

- **Autenticación**: no hay; el control de acceso es UAC al iniciar.
- **Autorización**: allowlist de comandos IPC en `capabilities/`; el webview no recibe plugins `shell` ni `fs` más allá de lo estrictamente necesario (**Suposición:** ninguno; todo acceso a archivos pasa por comandos propios con validación en dominio).
- **CSP estricta** (**Suposición:** `default-src 'self'`; sin `unsafe-eval`; sin orígenes remotos; `connect-src` vacío, coherente con "sin red").
- **Validación de input**: toda ruta recibida por IPC se canonicaliza y se valida en dominio; el frontend nunca decide qué es borrable, solo referencia IDs de hallazgos/previews generados por el backend.
- **Sin red**: sin crates ni features de HTTP; verificable en `cargo tree` / CI (**Suposición**).
- **Secrets management**: no hay secretos. No se commitean datos de usuario; `.engram/` está en `.gitignore` a propósito (repo público).
- **Privilegios**: always-admin, sin drivers (ver 13). Dependencias auditadas con `cargo audit` y `npm audit` (**Suposición**).
- **Datos locales**: SQLite en el perfil del usuario real; sin cifrado (**Suposición:** no hay datos sensibles más allá de rutas).

## Testing (Strict TDD)

- Rust: `cargo test`; módulos de `domain` con tests unitarios/de tabla; adaptadores con tests de integración sobre directorios temporales y fakes.
- Frontend: Vitest + Testing Library (**Suposición**).
- Prioridad de cobertura: protección de rutas > limpieza > snapshots/undo > scoring > resto.

## Variables de entorno / configuración

No se requieren secretos. Configuración en `settings` (SQLite) y archivos de Tauri.

| Variable | Descripción | Ejemplo | Sensible |
|----------|-------------|---------|----------|
| `PCOPT_DATA_DIR` (**Suposición**, solo dev/tests) | Override del directorio de datos | `C:\tmp\pcopt-dev` | N |
| `PCOPT_LOG` (**Suposición**) | Nivel de log | `info` | N |
| `RUST_LOG` | Nivel de logging si se usa `tracing`/`env_logger` | `debug` | N |

## CI / Build (referencia)

- Build: `npm run tauri build` (bundle NSIS/MSI).
- Verificación: `cargo test`, `cargo clippy`, `npm test`, `npm run build`.
- Releases en GitHub, sin firma en v0.x.

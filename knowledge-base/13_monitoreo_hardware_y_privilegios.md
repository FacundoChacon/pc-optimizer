# Monitoreo de Hardware y Privilegios

Gobernanza: monitoreo = **MEDIUM**; manejo de privilegios y autostart = **HIGH**.

## Modelo de privilegios

- Manifest `requireAdministrator`: UAC en **cada** arranque (decisión del usuario, DD-02).
- **Nunca** drivers de kernel. WinRing0 rechazado explícitamente (Microsoft Defender lo marca como vulnerable).
- La elevación no amplía la whitelist de limpieza (RN-PRIV-05).

### Autostart de la bandeja (opt-in)

- Se implementa con una **tarea de Task Scheduler** con "run with highest privileges" y disparador "al iniciar sesión".
- No se usa la clave `Run` del registro (no puede elevar sin UAC).
- Desactivar la opción o desinstalar la app **elimina la tarea**.
- Crear/eliminar la tarea requiere elevación (ya garantizada).

### Over-the-shoulder y perfil correcto

- Escenario: usuario estándar escribe credenciales de **otra** cuenta admin en el prompt UAC → el proceso corre como esa cuenta.
- Consecuencia: `%TEMP%`, cachés de navegador, Papelera y claves HKCU corresponden al perfil equivocado.
- Detección (C-05): se compara el SID del token del proceso con dos señales de la **misma sesión de Windows** que el proceso: el dueño del shell (`GetShellWindow`) y el usuario de logon (WTS). Alcanza con una señal y todas las disponibles deben coincidir. Es fail-closed: sin señales, con señales contradictorias o con el token ilegible el resultado es mismatch (`undetermined`). Se calcula una vez al arrancar.
- Respuesta: advertencia visible, bloqueo de limpieza por-usuario y de acciones HKCU (`scope_access`, `restrict_protection_context`); el resto de la app sigue disponible. El directorio de datos de la app queda en el perfil del token del proceso (DD-16; SU-04 refutado).

### UIPI

El drag & drop desde Explorador (nivel de integridad medio) hacia la ventana elevada (alto) no funciona. Se ofrece selector de carpeta/archivo, que se implementa en su primer consumidor (C-07 o C-21), no en C-05.

## Fuentes de métricas

| Métrica | Fuente (sin driver) | Disponibilidad |
|---------|---------------------|----------------|
| CPU uso | `sysinfo` | Siempre |
| RAM uso/presión | `sysinfo` | Siempre |
| Disco uso y espacio libre | `sysinfo` / Win32 | Siempre |
| Disco actividad/throughput | contadores de rendimiento (PDH) (**Suposición**) | Normalmente |
| SMART (estado) | WMI (`MSStorageDriver_FailurePredictStatus`, `MSFT_PhysicalDisk`) | Depende del controlador |
| Temperatura disco NVMe/SATA | SMART/IOCTL de almacenamiento estándar (sin driver propio) | Frecuente en NVMe |
| Temperatura GPU | NVML (NVIDIA) / ADLX (AMD) si el driver está presente | Solo con driver del fabricante |
| Temperatura "CPU" | Zona térmica ACPI vía WMI (`MSAcpi_ThermalZoneTemperature`) si el firmware la expone | **Frecuentemente no disponible**; si aparece, puede no representar la CPU |
| Procesos | `sysinfo` | Siempre |

**Honestidad en la UI**: cuando una métrica no está disponible se muestra "no disponible" con una explicación breve; nunca se inventa ni se estima el valor. La temperatura de CPU será a menudo no disponible sin driver, y es un trade-off aceptado.

## Muestreo y retención

| Capa | Frecuencia | Almacenamiento | Retención |
|------|------------|----------------|-----------|
| Muestras en vivo | 1-2 s | Memoria (ring buffer) | Ventana corta (**Suposición:** ~5 min para gráficos) |
| Agregados por minuto | 1 min | SQLite `metric_sample_minute` | 24 h |
| Agregados por hora | 1 h | SQLite `metric_hourly` | ~90 días |
| Snapshots de carpetas | Periódico (**Suposición:** semanal) | `folder_snapshot` | ~90 días |

- En bandeja con ventana oculta, el muestreo puede reducirse (**Suposición:** a 5-10 s) para bajar consumo.
- El muestreo corre en hilo propio; si una lectura (WMI) se demora, no bloquea la UI.
- Tras suspensión/hibernación, los huecos se registran como ausencia de datos, no como ceros.

## Alertas y monitor

- Reglas: disco > 90 %, temperatura alta (sensores disponibles), RAM saturada (defaults **Suposición**, configurables).
- Cooldown anti-spam por regla (RN-ALE-02).
- Sin sensor disponible → la regla correspondiente no evalúa.

## Gobernanza de adaptadores

- Cada fuente (WMI, NVML, ADLX, PDH) es un adaptador tras un trait de `domain/ports.rs` con un fake para tests.
- NVML/ADLX se cargan dinámicamente y de forma opcional; su ausencia no es un error.

## Nota C-07: selector de carpeta y Explorador en el proceso elevado

`pick_folder_root` (`IFileOpenDialog` con `FOS_PICKFOLDERS`) y `reveal_in_explorer` (`SHOpenFolderAndSelectItems`, nunca `ShellExecute`) corren en hilos COM STA propios del backend, sin plugins `dialog`/`shell` ni capabilities nuevas. Riesgo conocido: el diálogo del shell carga extensiones de terceros dentro del proceso elevado (mitigado con `FOS_PICKFOLDERS`). En over-the-shoulder el Explorador está bloqueado.

## Nota C-06: fuentes reales, ciclo de vida y huecos

- CPU, memoria y bytes leídos/escritos acumulados del volumen del sistema: `sysinfo` (CPU por PDH interno, memoria `GlobalMemoryStatusEx`, disco `IOCTL_DISK_PERFORMANCE`). Espacio del volumen: `GetDiskFreeSpaceExW`. Tiempo activo del disco: PDH `\LogicalDisk(C:)\% Idle Time` (`PdhAddEnglishCounterW`, independiente del idioma) → `100 - idle` acotado a 0..=100; la primera colección es "en calentamiento". Único `unsafe` nuevo: `adapters/windows/pdh.rs`.
- El muestreo arranca con la app (después de la base y los privilegios) y corre siempre, con una ventana de 300 muestras de 1 s y desalojo por antigüedad (> 300 s). Un pánico en el hilo lo deja en `failed` (sin reinicio automático) y la app sigue.
- Huecos: delta monotónico > 2,5 intervalos, desacuerdo pared/monotónico > 2 s o pared hacia atrás (vacía la ventana). Tras un hueco CPU, tiempo activo y tasas quedan ausentes (`after_gap`); memoria y espacio se conservan.
- Una métrica ausente lleva su motivo (`warming_up`, `after_gap`, `source_error`, `not_supported`) y nunca se muestra como 0.

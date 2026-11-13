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
- Detección: comparar el SID/usuario del token del proceso con el usuario de la sesión interactiva (por ejemplo, propietario de `explorer.exe` / sesión de consola) (**Suposición** sobre el mecanismo exacto).
- Respuesta: advertencia visible, bloqueo de limpieza por-usuario y de acciones HKCU; el directorio de datos de la app se ubica en el perfil de la sesión interactiva (SU-04).

### UIPI

El drag & drop desde Explorador (nivel de integridad medio) hacia la ventana elevada (alto) no funciona. Se ofrece selector de carpeta/archivo.

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

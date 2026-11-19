//! Puerto de sensores de rendimiento (C-06, D-05).
//!
//! Devuelve lecturas CRUDAS: contadores acumulados y valores que la fuente ya calcula entre dos
//! lecturas (CPU, tiempo activo). Las tasas, los huecos y los promedios los calcula el dominio.
//! Una metrica que no se pudo leer es `Err(Unavailable)`: nunca un 0 inventado.

/// Motivo por el que una metrica no esta disponible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unavailable {
    /// La fuente necesita otra lectura para calcular el valor (p. ej. la primera de la CPU).
    WarmingUp,
    /// La fuente fallo en esta lectura.
    SourceError,
    /// Esta maquina no ofrece la fuente.
    NotSupported,
}

/// Valor de una metrica o el motivo por el que falta.
pub type Reading<T> = Result<T, Unavailable>;

/// Memoria fisica en bytes (`used_bytes <= total_bytes`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryReading {
    pub used_bytes: u64,
    pub total_bytes: u64,
}

/// Espacio del volumen del sistema en bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VolumeReading {
    pub total_bytes: u64,
    pub free_bytes: u64,
}

/// Bytes leidos y escritos ACUMULADOS del volumen del sistema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiskIoCounters {
    pub read_bytes: u64,
    pub written_bytes: u64,
}

/// Lectura instantanea del sistema; cada metrica falla por separado.
#[derive(Debug, Clone, PartialEq)]
pub struct SensorReading {
    /// 0..=100, calculado por la fuente entre dos lecturas.
    pub cpu_pct: Reading<f32>,
    pub memory: Reading<MemoryReading>,
    pub volume: Reading<VolumeReading>,
    pub disk_io: Reading<DiskIoCounters>,
    /// 0..=100, tiempo activo del disco calculado por la fuente.
    pub disk_active_pct: Reading<f32>,
}

impl SensorReading {
    /// Lectura con todas las metricas `NotSupported`.
    pub fn not_supported() -> Self {
        Self {
            cpu_pct: Err(Unavailable::NotSupported),
            memory: Err(Unavailable::NotSupported),
            volume: Err(Unavailable::NotSupported),
            disk_io: Err(Unavailable::NotSupported),
            disk_active_pct: Err(Unavailable::NotSupported),
        }
    }
}

/// Fuente de lecturas; `&mut self` porque la lectura refresca estado interno. Infalible como un
/// todo: cada metrica informa su propia indisponibilidad.
pub trait Sensors {
    fn read(&mut self) -> SensorReading;
}

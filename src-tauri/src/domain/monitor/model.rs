//! Tipos del monitor: muestra, disponibilidad, resumen y limites de la ventana (D-02, D-07, D-09).

use crate::domain::model::EpochMillis;

/// Intervalo de muestreo por defecto (RN-MON-01).
pub const INTERVAL_MS: u64 = 1_000;
/// Cantidad maxima de muestras en la ventana (5 min a 1 muestra por segundo).
pub const WINDOW_CAPACITY: usize = 300;
/// Antiguedad maxima de una muestra respecto de la mas reciente.
pub const WINDOW_MAX_AGE_MS: i64 = 300_000;
/// Periodo del promedio usado para los niveles.
pub const AVERAGE_WINDOW_MS: i64 = 60_000;

/// Estado de una metrica en una muestra: el valor esta (`Ok`) o falta con un motivo estable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Availability {
    Ok,
    WarmingUp,
    AfterGap,
    SourceError,
    NotSupported,
}

/// Disponibilidad de cada metrica de una muestra.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SampleAvailability {
    pub cpu: Availability,
    pub memory: Availability,
    pub volume: Availability,
    pub disk_active: Availability,
    pub disk_io: Availability,
}

/// Muestra del monitor. Cada metrica es `Option`: `None` = ausente (ver `availability`).
#[derive(Debug, Clone, PartialEq)]
pub struct MonitorSample {
    /// Estrictamente creciente desde 1.
    pub seq: u64,
    /// Reloj de pared (UTC) al tomar la muestra.
    pub at: EpochMillis,
    /// Hubo un hueco (suspension, salto del reloj) entre la muestra anterior y esta.
    pub gap_before: bool,
    pub cpu_pct: Option<f32>,
    pub mem_used_bytes: Option<u64>,
    pub mem_total_bytes: Option<u64>,
    pub volume_total_bytes: Option<u64>,
    pub volume_free_bytes: Option<u64>,
    pub disk_active_pct: Option<f32>,
    pub disk_read_bps: Option<u64>,
    pub disk_write_bps: Option<u64>,
    pub availability: SampleAvailability,
    /// Costo de la lectura de sensores de este ciclo, en microsegundos.
    pub read_micros: u64,
}

impl MonitorSample {
    /// Memoria usada en porcentaje (0..=100); `None` si falta alguno de los dos valores.
    pub fn mem_used_pct(&self) -> Option<f32> {
        let (used, total) = (self.mem_used_bytes?, self.mem_total_bytes?);
        pct_of(used, total)
    }

    /// Espacio usado del volumen en porcentaje (0..=100); `None` si falta algun valor.
    pub fn volume_used_pct(&self) -> Option<f32> {
        let (total, free) = (self.volume_total_bytes?, self.volume_free_bytes?);
        pct_of(total.saturating_sub(free), total)
    }
}

/// `part / total * 100` calculado en f64; `None` si `total` es 0.
fn pct_of(part: u64, total: u64) -> Option<f32> {
    if total == 0 {
        return None;
    }
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    let pct = (part as f64 * 100.0 / total as f64) as f32;
    Some(pct)
}

/// Nivel llano de una metrica de uso (D-09).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Normal,
    High,
    VeryHigh,
    Unknown,
}

/// Nivel del espacio del volumen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpaceLevel {
    Normal,
    AlmostFull,
    Unknown,
}

/// Resumen de una metrica: actual, promedio de 60 s, pico de la ventana y nivel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MetricSummary {
    pub current: Option<f32>,
    pub avg60: Option<f32>,
    pub peak: Option<f32>,
    pub level: Level,
}

/// Resumen del espacio del volumen del sistema.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VolumeSummary {
    pub used_pct: Option<f32>,
    pub level: SpaceLevel,
}

/// Resumen de la ventana.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MonitorSummary {
    pub cpu: MetricSummary,
    pub memory: MetricSummary,
    pub disk_active: MetricSummary,
    pub volume: VolumeSummary,
    /// Disponibilidad en la ultima muestra de la ventana.
    pub availability: SampleAvailability,
    pub gaps_in_window: u32,
    /// Costo medio de la lectura de sensores; `None` sin muestras.
    pub avg_read_micros: Option<u64>,
}

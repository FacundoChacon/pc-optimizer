//! Puerto de sensores de rendimiento.

use crate::domain::ports::error::PortError;

/// Muestra puntual del sistema.
#[derive(Debug, Clone, PartialEq)]
pub struct SensorSample {
    /// 0..=100.
    pub cpu_pct: f32,
    pub ram_used_bytes: u64,
    pub ram_total_bytes: u64,
    /// `None` = no disponible (nunca un 0 falso).
    pub disk_busy_pct: Option<f32>,
}

/// Fuente de muestras; `&mut self` porque el muestreo refresca estado interno.
pub trait Sensors {
    fn sample(&mut self) -> Result<SensorSample, PortError>;
}

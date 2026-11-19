//! `MonitorEngine`: convierte lecturas crudas de sensores en muestras (D-05, D-07).
//!
//! Calcula las tasas de disco a partir de contadores acumulados y del reloj monotonico, aplica
//! el calentamiento de la primera lectura y valida rangos. Una metrica que falta queda `None`
//! con su motivo; nunca se rellena con 0.

use crate::domain::model::EpochMillis;
use crate::domain::monitor::model::{Availability, MonitorSample, SampleAvailability};
use crate::domain::ports::{DiskIoCounters, Reading, SensorReading, Unavailable};

/// Instantes y costo de una lectura.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tick {
    /// Reloj monotonico (ms) al tomar la lectura.
    pub mono_ms: u64,
    /// Reloj de pared (UTC).
    pub wall: EpochMillis,
    /// Microsegundos que tardo la lectura de sensores.
    pub read_micros: u64,
}

/// Resultado de procesar una lectura.
#[derive(Debug, Clone, PartialEq)]
pub struct Ingested {
    pub sample: MonitorSample,
    /// La ventana debe vaciarse antes de agregar la muestra (el reloj de pared retrocedio).
    pub clear_window: bool,
}

/// Estado de la lectura anterior.
#[derive(Debug, Clone, Copy)]
struct Previous {
    wall: EpochMillis,
    mono_ms: u64,
    io: Option<DiskIoCounters>,
}

#[derive(Debug)]
pub struct MonitorEngine {
    interval_ms: u64,
    seq: u64,
    previous: Option<Previous>,
}

impl MonitorEngine {
    /// `interval_ms` es el intervalo de muestreo esperado.
    pub fn new(interval_ms: u64) -> Self {
        Self {
            interval_ms,
            seq: 0,
            previous: None,
        }
    }

    pub fn ingest(&mut self, reading: &SensorReading, tick: Tick) -> Ingested {
        self.seq += 1;
        let first = self.previous.is_none();
        let (gap, clear_window) = self.detect_gap(tick);
        let blank = if first {
            Some(Availability::WarmingUp)
        } else if gap {
            Some(Availability::AfterGap)
        } else {
            None
        };

        let (cpu_pct, cpu_av) = percentage(reading.cpu_pct, blank);
        let (disk_active_pct, disk_active_av) = percentage(reading.disk_active_pct, blank);
        let (mem_used, mem_total, memory_av) = match reading.memory {
            Ok(m) => (
                Some(m.used_bytes.min(m.total_bytes)),
                Some(m.total_bytes),
                Availability::Ok,
            ),
            Err(reason) => (None, None, reason_to_availability(reason)),
        };
        let (vol_total, vol_free, volume_av) = match reading.volume {
            Ok(v) => (
                Some(v.total_bytes),
                Some(v.free_bytes.min(v.total_bytes)),
                Availability::Ok,
            ),
            Err(reason) => (None, None, reason_to_availability(reason)),
        };
        let (read_bps, write_bps, disk_io_av) = self.rates(reading.disk_io, tick, blank);

        self.previous = Some(Previous {
            wall: tick.wall,
            mono_ms: tick.mono_ms,
            io: reading.disk_io.ok(),
        });

        Ingested {
            sample: MonitorSample {
                seq: self.seq,
                at: tick.wall,
                gap_before: gap,
                cpu_pct,
                mem_used_bytes: mem_used,
                mem_total_bytes: mem_total,
                volume_total_bytes: vol_total,
                volume_free_bytes: vol_free,
                disk_active_pct,
                disk_read_bps: read_bps,
                disk_write_bps: write_bps,
                availability: SampleAvailability {
                    cpu: cpu_av,
                    memory: memory_av,
                    volume: volume_av,
                    disk_active: disk_active_av,
                    disk_io: disk_io_av,
                },
                read_micros: tick.read_micros,
            },
            clear_window,
        }
    }

    /// `(hueco, vaciar_ventana)` respecto de la lectura anterior (D-07): hueco si el delta
    /// monotonico supera 2,5 intervalos, si pared y monotonico difieren en mas de 2 s o si la
    /// pared retrocede (ademas se vacia la ventana: ya no se puede ordenar por tiempo).
    fn detect_gap(&self, tick: Tick) -> (bool, bool) {
        let Some(previous) = self.previous else {
            return (false, false);
        };
        let mono_delta = tick.mono_ms.saturating_sub(previous.mono_ms);
        let wall_delta = tick.wall.0.saturating_sub(previous.wall.0);
        if wall_delta < 0 {
            return (true, true);
        }
        let mono_signed = i64::try_from(mono_delta).unwrap_or(i64::MAX);
        let too_long = u128::from(mono_delta) * 2 > u128::from(self.interval_ms) * 5;
        let disagree = (wall_delta - mono_signed).abs() > WALL_DISAGREEMENT_MS;
        (too_long || disagree, false)
    }

    /// Tasas (bytes/s) entre la lectura anterior y esta.
    fn rates(
        &self,
        io: Reading<DiskIoCounters>,
        tick: Tick,
        blank: Option<Availability>,
    ) -> (Option<u64>, Option<u64>, Availability) {
        let current = match io {
            Ok(counters) => counters,
            Err(reason) => return (None, None, reason_to_availability(reason)),
        };
        if let Some(reason) = blank {
            return (None, None, reason);
        }
        let Some(Previous {
            mono_ms,
            wall: _,
            io: Some(prev),
        }) = self.previous
        else {
            // La lectura anterior no tenia contadores: no hay base para calcular.
            return (None, None, Availability::WarmingUp);
        };
        let delta_ms = tick.mono_ms.saturating_sub(mono_ms);
        if delta_ms == 0 {
            return (None, None, Availability::SourceError);
        }
        let read = rate(prev.read_bytes, current.read_bytes, delta_ms);
        let write = rate(prev.written_bytes, current.written_bytes, delta_ms);
        let availability = if read.is_some() && write.is_some() {
            Availability::Ok
        } else {
            Availability::SourceError
        };
        (read, write, availability)
    }
}

/// Tolerancia de desacuerdo entre reloj de pared y monotonico antes de declarar un hueco.
const WALL_DISAGREEMENT_MS: i64 = 2_000;

/// `(actual - anterior) * 1000 / delta_ms`; `None` si el contador retrocedio.
fn rate(previous: u64, current: u64, delta_ms: u64) -> Option<u64> {
    let delta = current.checked_sub(previous)?;
    let bytes_per_s = u128::from(delta) * 1_000 / u128::from(delta_ms);
    Some(u64::try_from(bytes_per_s).unwrap_or(u64::MAX))
}

/// Porcentaje 0..=100 de una fuente. En la primera lectura nunca hay valor de intervalo.
fn percentage(reading: Reading<f32>, blank: Option<Availability>) -> (Option<f32>, Availability) {
    if let Some(reason) = blank {
        return (None, reason);
    }
    match reading {
        Ok(value) if value.is_finite() => (Some(value.clamp(0.0, 100.0)), Availability::Ok),
        Ok(_) => (None, Availability::SourceError),
        Err(reason) => (None, reason_to_availability(reason)),
    }
}

fn reason_to_availability(reason: Unavailable) -> Availability {
    match reason {
        Unavailable::WarmingUp => Availability::WarmingUp,
        Unavailable::SourceError => Availability::SourceError,
        Unavailable::NotSupported => Availability::NotSupported,
    }
}

#[cfg(test)]
#[path = "engine_tests.rs"]
mod tests;

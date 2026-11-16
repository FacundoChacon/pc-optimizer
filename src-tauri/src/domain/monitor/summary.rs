//! Resumen de la ventana y niveles llanos (D-09).
//!
//! Por metrica: valor actual, promedio de los ultimos 60 s, pico de la ventana y nivel segun el
//! promedio (un pico de un segundo no cambia el nivel). Las muestras ausentes no cuentan.

use crate::domain::model::EpochMillis;
use crate::domain::monitor::model::{
    AVERAGE_WINDOW_MS, Level, MetricSummary, MonitorSample, MonitorSummary, SpaceLevel,
    VolumeSummary, WINDOW_MAX_AGE_MS,
};

/// Umbrales (alto, muy alto) en porcentaje: CPU, memoria y tiempo activo del disco.
const CPU_THRESHOLDS: (f64, f64) = (70.0, 90.0);
const MEMORY_THRESHOLDS: (f64, f64) = (80.0, 90.0);
const DISK_ACTIVE_THRESHOLDS: (f64, f64) = (60.0, 90.0);
/// Espacio usado a partir del cual el volumen esta "casi lleno".
const ALMOST_FULL_PCT: f64 = 90.0;

/// Resume las muestras (de la mas vieja a la mas nueva) relativas a `now`. Ignora las que tienen
/// mas de 300 s de antiguedad. `None` si no queda ninguna.
pub fn summarize<'a>(
    window: impl Iterator<Item = &'a MonitorSample>,
    now: EpochMillis,
) -> Option<MonitorSummary> {
    let age_floor = now.0.saturating_sub(WINDOW_MAX_AGE_MS);
    let recent_from = now.0.saturating_sub(AVERAGE_WINDOW_MS);
    let samples: Vec<&MonitorSample> = window.filter(|s| s.at.0 >= age_floor).collect();
    let last = samples.last()?;

    let metric = |value: fn(&MonitorSample) -> Option<f32>, thresholds: (f64, f64)| {
        summarize_metric(&samples, recent_from, value, thresholds)
    };
    let volume_pct = samples.iter().rev().find_map(|s| s.volume_used_pct());
    let reads: u128 = samples.iter().map(|s| u128::from(s.read_micros)).sum();

    Some(MonitorSummary {
        cpu: metric(|s| s.cpu_pct, CPU_THRESHOLDS),
        memory: metric(MonitorSample::mem_used_pct, MEMORY_THRESHOLDS),
        disk_active: metric(|s| s.disk_active_pct, DISK_ACTIVE_THRESHOLDS),
        volume: VolumeSummary {
            used_pct: volume_pct,
            level: space_level(volume_pct),
        },
        availability: last.availability,
        gaps_in_window: u32::try_from(samples.iter().filter(|s| s.gap_before).count())
            .unwrap_or(u32::MAX),
        avg_read_micros: u64::try_from(reads / samples.len() as u128).ok(),
    })
}

fn summarize_metric(
    samples: &[&MonitorSample],
    recent_from: i64,
    value: fn(&MonitorSample) -> Option<f32>,
    thresholds: (f64, f64),
) -> MetricSummary {
    let current = samples.iter().rev().find_map(|s| value(s));
    let peak = samples
        .iter()
        .filter_map(|s| value(s))
        .fold(None, |max: Option<f32>, v| {
            Some(max.map_or(v, |m| m.max(v)))
        });
    let recent: Vec<f64> = samples
        .iter()
        .filter(|s| s.at.0 > recent_from)
        .filter_map(|s| value(s))
        .map(f64::from)
        .collect();
    let avg60 = if recent.is_empty() {
        None
    } else {
        #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
        let avg = (recent.iter().sum::<f64>() / recent.len() as f64) as f32;
        Some(avg)
    };
    MetricSummary {
        current,
        avg60,
        peak,
        level: avg60.map_or(Level::Unknown, |a| level_for(f64::from(a), thresholds)),
    }
}

fn level_for(avg: f64, (high, very_high): (f64, f64)) -> Level {
    if avg >= very_high {
        Level::VeryHigh
    } else if avg >= high {
        Level::High
    } else {
        Level::Normal
    }
}

fn space_level(used_pct: Option<f32>) -> SpaceLevel {
    match used_pct {
        None => SpaceLevel::Unknown,
        Some(p) if f64::from(p) >= ALMOST_FULL_PCT => SpaceLevel::AlmostFull,
        Some(_) => SpaceLevel::Normal,
    }
}

#[cfg(test)]
#[path = "summary_tests.rs"]
mod tests;

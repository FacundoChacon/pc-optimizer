//! Bucle de muestreo generico sobre los puertos (D-10): se prueba sin hilos ni `sleep`.

use crate::domain::model::EpochMillis;
use crate::domain::monitor::engine::{MonitorEngine, Tick};
use crate::domain::monitor::model::{MonitorSample, MonitorSummary};
use crate::domain::monitor::schedule::next_deadline;
use crate::domain::monitor::window::SampleWindow;
use crate::domain::ports::{Clock, MonotonicClock, Sensors};

/// Espera inyectable hasta un plazo monotonico. La real usa `recv_timeout` sobre un canal de
/// parada; el doble avanza los relojes falsos.
pub trait Ticker {
    /// Bloquea hasta `deadline_ms`; `false` si hay que terminar el bucle.
    fn wait_until(&mut self, deadline_ms: u64) -> bool;
}

/// Resultado de un ciclo.
#[derive(Debug, Clone, PartialEq)]
pub struct CycleResult {
    pub sample: MonitorSample,
    pub summary: MonitorSummary,
    /// La ventana se vacio antes de agregar la muestra.
    pub window_cleared: bool,
}

/// Muestreador: lee sensores, calcula la muestra, la agrega a la ventana y resume.
pub struct Sampler<S, C, M> {
    sensors: S,
    wall: C,
    mono: M,
    engine: MonitorEngine,
    window: SampleWindow,
    interval_ms: u64,
}

impl<S: Sensors, C: Clock, M: MonotonicClock> Sampler<S, C, M> {
    pub fn new(sensors: S, wall: C, mono: M, interval_ms: u64) -> Self {
        Self {
            sensors,
            wall,
            mono,
            engine: MonitorEngine::new(interval_ms),
            window: SampleWindow::new(),
            interval_ms,
        }
    }

    pub fn window(&self) -> &SampleWindow {
        &self.window
    }

    /// Un ciclo: leer (midiendo el costo), procesar, agregar a la ventana y resumir.
    /// `None` solo si el resumen no se pudo calcular (no ocurre: la ventana tiene la muestra).
    pub fn run_cycle(&mut self) -> Option<CycleResult> {
        let wall: EpochMillis = self.wall.now();
        let started_ms = self.mono.now_ms();
        let started_us = self.mono.now_micros();
        let reading = self.sensors.read();
        let read_micros = self.mono.now_micros().saturating_sub(started_us);
        let ingested = self.engine.ingest(
            &reading,
            Tick {
                mono_ms: started_ms,
                wall,
                read_micros,
            },
        );
        self.window
            .push(ingested.sample.clone(), ingested.clear_window);
        let summary = self.window.summary()?;
        Some(CycleResult {
            sample: ingested.sample,
            summary,
            window_cleared: ingested.clear_window,
        })
    }

    fn now_ms(&self) -> u64 {
        self.mono.now_ms()
    }
}

/// Corre ciclos hasta que el `ticker` pida terminar. El primer ciclo es inmediato; luego se
/// mantiene la cadencia por plazos y, tras un atraso largo, se reprograma desde "ahora".
pub fn run_sampler<S, C, M, T>(
    sampler: &mut Sampler<S, C, M>,
    ticker: &mut T,
    mut on_cycle: impl FnMut(&CycleResult),
) where
    S: Sensors,
    C: Clock,
    M: MonotonicClock,
    T: Ticker,
{
    let mut deadline: Option<u64> = None;
    loop {
        let now = sampler.now_ms();
        let next = match deadline {
            None => now,
            Some(previous) => next_deadline(previous, now, sampler.interval_ms),
        };
        if !ticker.wait_until(next) {
            break;
        }
        deadline = Some(next);
        if let Some(cycle) = sampler.run_cycle() {
            on_cycle(&cycle);
        }
    }
}

#[cfg(test)]
#[path = "runner_tests.rs"]
mod tests;

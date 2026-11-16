//! Ventana de muestras en memoria: hasta 300 muestras y ninguna con mas de 300 s de antiguedad
//! respecto de la mas reciente (D-02). Nada se persiste.

use crate::domain::monitor::model::{
    MonitorSample, MonitorSummary, WINDOW_CAPACITY, WINDOW_MAX_AGE_MS,
};
use crate::domain::monitor::ring::RingBuffer;
use crate::domain::monitor::summary::summarize;

#[derive(Debug, Clone)]
pub struct SampleWindow {
    ring: RingBuffer<MonitorSample>,
}

impl Default for SampleWindow {
    fn default() -> Self {
        Self::new()
    }
}

impl SampleWindow {
    pub fn new() -> Self {
        Self {
            ring: RingBuffer::bounded(WINDOW_CAPACITY),
        }
    }

    /// Agrega una muestra. `clear` vacia antes la ventana (el reloj de pared retrocedio);
    /// despues descarta las muestras con mas de 300 s respecto de la nueva.
    pub fn push(&mut self, sample: MonitorSample, clear: bool) {
        if clear {
            self.ring.clear();
        }
        let age_floor = sample.at.0.saturating_sub(WINDOW_MAX_AGE_MS);
        self.ring.push(sample);
        self.ring.retain(|s| s.at.0 >= age_floor);
    }

    pub fn len(&self) -> usize {
        self.ring.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ring.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &MonitorSample> {
        self.ring.iter()
    }

    pub fn latest(&self) -> Option<&MonitorSample> {
        self.ring.last()
    }

    /// Copia ordenada por `seq` creciente.
    pub fn to_vec(&self) -> Vec<MonitorSample> {
        self.ring.iter().cloned().collect()
    }

    /// Resumen relativo a la muestra mas reciente; `None` si la ventana esta vacia.
    pub fn summary(&self) -> Option<MonitorSummary> {
        let now = self.latest()?.at;
        summarize(self.iter(), now)
    }
}

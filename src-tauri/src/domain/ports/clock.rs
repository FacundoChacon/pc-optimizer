//! Puerto de reloj: el dominio no lee la hora del sistema directamente.

use crate::domain::model::EpochMillis;

pub trait Clock {
    fn now(&self) -> EpochMillis;
}

/// Reloj monotonico en milisegundos: nunca retrocede. Sirve para medir intervalos; el reloj de
/// pared (`Clock`) puede saltar por cambios de hora y no sirve solo para eso (C-06, D-05).
pub trait MonotonicClock {
    /// Milisegundos desde un origen arbitrario fijo para la vida de la instancia.
    fn now_ms(&self) -> u64;

    /// Microsegundos en la misma escala; permite medir lecturas de menos de 1 ms.
    fn now_micros(&self) -> u64 {
        self.now_ms().saturating_mul(1_000)
    }
}

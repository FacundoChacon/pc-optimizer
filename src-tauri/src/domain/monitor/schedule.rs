//! Programacion por plazo del muestreo (D-02): sin rafagas tras un atraso.

/// Proximo plazo (ms monotonicos). Mantiene la cadencia `prev + interval`; si el hilo se atraso
/// mas de un intervalo (bloqueo, suspension) recalcula desde `now` para no tomar muestras
/// seguidas intentando "ponerse al dia".
pub fn next_deadline(prev: u64, now: u64, interval: u64) -> u64 {
    let candidate = prev.saturating_add(interval);
    if now.saturating_sub(candidate) > interval {
        now.saturating_add(interval)
    } else {
        candidate
    }
}

#[cfg(test)]
#[path = "schedule_tests.rs"]
mod tests;

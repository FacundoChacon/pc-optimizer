//! Reloj real del sistema.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::domain::model::EpochMillis;
use crate::domain::ports::{Clock, MonotonicClock};

/// Reloj basado en `SystemTime`.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

/// Convierte una duracion a milisegundos, saturando en `i64::MAX`.
fn duration_to_millis(duration: Duration) -> i64 {
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}

impl Clock for SystemClock {
    fn now(&self) -> EpochMillis {
        let since_epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO);
        EpochMillis(duration_to_millis(since_epoch))
    }
}

/// Reloj monotonico basado en `Instant`; el origen es el momento de su construccion.
#[derive(Debug, Clone, Copy)]
pub struct SystemMonotonicClock {
    origin: Instant,
}

impl SystemMonotonicClock {
    pub fn new() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl Default for SystemMonotonicClock {
    fn default() -> Self {
        Self::new()
    }
}

impl MonotonicClock for SystemMonotonicClock {
    fn now_ms(&self) -> u64 {
        u64::try_from(self.origin.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    fn now_micros(&self) -> u64 {
        u64::try_from(self.origin.elapsed().as_micros()).unwrap_or(u64::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_should_be_after_2023_and_not_decrease() {
        let clock = SystemClock;
        let first = clock.now();
        let second = clock.now();
        assert!(first.0 > 1_700_000_000_000, "{first:?}");
        assert!(second >= first);
    }

    #[test]
    fn duration_to_millis_should_convert_and_saturate() {
        assert_eq!(duration_to_millis(Duration::from_millis(5)), 5);
        assert_eq!(duration_to_millis(Duration::from_secs(2)), 2000);
        assert_eq!(duration_to_millis(Duration::MAX), i64::MAX);
    }
}

#[cfg(test)]
mod monotonic_tests {
    use super::*;

    #[test]
    fn system_monotonic_clock_should_not_decrease_between_two_queries() {
        let clock = SystemMonotonicClock::new();
        let first = clock.now_ms();
        let second = clock.now_ms();
        assert!(second >= first, "{first} -> {second}");
    }

    #[test]
    fn system_monotonic_clock_should_start_near_zero_and_be_independent_per_instance() {
        let a = SystemMonotonicClock::new();
        let b = SystemMonotonicClock::new();
        assert!(a.now_ms() < 60_000);
        assert!(b.now_ms() < 60_000);
    }
}

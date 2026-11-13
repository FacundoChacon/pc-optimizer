//! Reloj real del sistema.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::domain::model::EpochMillis;
use crate::domain::ports::Clock;

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

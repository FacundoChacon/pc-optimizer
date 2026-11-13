//! Puerto de reloj: el dominio no lee la hora del sistema directamente.

use crate::domain::model::EpochMillis;

pub trait Clock {
    fn now(&self) -> EpochMillis;
}

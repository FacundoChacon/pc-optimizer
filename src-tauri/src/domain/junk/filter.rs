//! Filtros puros de candidatos (D-05, D-06, D-07): nombre y antiguedad.

use crate::domain::model::EpochMillis;

const HOUR_MS: i64 = 3_600_000;
const DAY_MS: i64 = 24 * HOUR_MS;

/// Antiguedad minima de los temporales (D-07). Constante del dominio, no un ajuste.
pub const TEMP_MIN_AGE_MS: i64 = 24 * HOUR_MS;
/// Antiguedad minima de `old_logs` (KB 11); sin ubicaciones en esta version (D-06).
pub const OLD_LOGS_MIN_AGE_MS: i64 = 30 * DAY_MS;

/// Filtro de nombre de archivo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameFilter {
    /// Cualquier nombre.
    Any,
    /// Solo `thumbcache_*.db` (sin distinguir mayusculas).
    ThumbCache,
}

/// Filtro de candidatos de una ubicacion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Filter {
    /// Ultima escritura minima hacia atras desde "ahora"; `None` = sin filtro de edad.
    pub min_age_ms: Option<i64>,
    pub name: NameFilter,
}

impl Filter {
    /// Sin filtros.
    pub const NONE: Filter = Filter {
        min_age_ms: None,
        name: NameFilter::Any,
    };

    /// Solo el filtro de nombre (los puntos de reanalisis no tienen una edad confiable).
    pub fn accepts_name(&self, name: &str) -> bool {
        match self.name {
            NameFilter::Any => true,
            NameFilter::ThumbCache => is_thumbcache_name(name),
        }
    }

    /// `true` si un archivo con ese nombre y fecha de ultima escritura es candidato.
    pub fn accepts(&self, name: &str, modified: Option<EpochMillis>, now: EpochMillis) -> bool {
        let name_ok = self.accepts_name(name);
        let age_ok = match self.min_age_ms {
            None => true,
            // Sin fecha, o con fecha futura, no se puede afirmar que sea vieja.
            Some(min_age) => {
                modified.is_some_and(|written| now.0.saturating_sub(written.0) >= min_age)
            }
        };
        name_ok && age_ok
    }
}

/// `thumbcache_*.db` sin distinguir mayusculas (como NTFS).
fn is_thumbcache_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.len() >= "thumbcache_.db".len()
        && lower.starts_with("thumbcache_")
        && lower.ends_with(".db")
}

#[cfg(test)]
#[path = "filter_tests.rs"]
mod tests;

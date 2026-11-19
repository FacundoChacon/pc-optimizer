//! Tiempo activo del disco por PDH (C-06, D-06): `\LogicalDisk(<letra>:)\% Idle Time`.
//!
//! Solo lectura. Se usa `% Idle Time` y no `% Disk Time` porque este ultimo puede superar 100.
//! `PdhAddEnglishCounterW` evita depender del idioma de Windows. Unico `unsafe` nuevo de C-06:
//! abrir la consulta, agregar el contador, colectar, leer el valor formateado y cerrar la
//! consulta en `Drop`; cada bloque es minimo y lleva su `// SAFETY:`. El mapeo de estados es puro.

use windows::Win32::System::Performance::{
    PDH_CSTATUS_NEW_DATA, PDH_CSTATUS_VALID_DATA, PDH_FMT_COUNTERVALUE, PDH_FMT_DOUBLE,
    PDH_HCOUNTER, PDH_HQUERY, PDH_INVALID_DATA, PdhAddEnglishCounterW, PdhCloseQuery,
    PdhCollectQueryData, PdhGetFormattedCounterValue, PdhOpenQueryW,
};
use windows::core::PCWSTR;

use crate::domain::ports::{Reading, Unavailable};

/// Ruta del contador de tiempo inactivo del volumen `drive`; `None` si no es una letra ASCII.
pub(crate) fn counter_path(drive: char) -> Option<String> {
    drive
        .is_ascii_alphabetic()
        .then(|| format!(r"\LogicalDisk({}:)\% Idle Time", drive.to_ascii_uppercase()))
}

/// Estado de una llamada PDH que fallo: `PDH_INVALID_DATA` es lo que devuelve la primera
/// coleccion (falta la segunda para calcular el valor); cualquier otro es un error de la fuente.
pub(crate) fn map_call_failure(status: u32) -> Unavailable {
    if status == PDH_INVALID_DATA {
        Unavailable::WarmingUp
    } else {
        Unavailable::SourceError
    }
}

/// Convierte el estado y el valor `% Idle Time` formateado en tiempo activo (0..=100).
pub(crate) fn map_idle(status: u32, idle: f64) -> Reading<f32> {
    if status != PDH_CSTATUS_VALID_DATA && status != PDH_CSTATUS_NEW_DATA {
        return Err(map_call_failure(status));
    }
    if !idle.is_finite() {
        return Err(Unavailable::SourceError);
    }
    #[allow(clippy::cast_possible_truncation)]
    let active = (100.0 - idle).clamp(0.0, 100.0) as f32;
    Ok(active)
}

/// Consulta PDH abierta sobre el contador de tiempo inactivo del volumen. Los handles son
/// punteros crudos: el valor no es `Send` y se crea en el hilo que lo usa.
pub struct PdhIdleTime {
    query: PDH_HQUERY,
    counter: PDH_HCOUNTER,
}

impl PdhIdleTime {
    /// Abre la consulta del volumen `drive` y toma la primera coleccion (linea base). Cualquier
    /// fallo al abrir o agregar el contador es `NotSupported` (contadores deshabilitados, etc.).
    pub fn open(drive: char) -> Result<Self, Unavailable> {
        let path = counter_path(drive).ok_or(Unavailable::NotSupported)?;
        let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
        let mut query = PDH_HQUERY(std::ptr::null_mut());
        // SAFETY: `query` es un local valido para escritura; la fuente de datos nula = datos en
        // tiempo real y la API no retiene el puntero.
        let status = unsafe { PdhOpenQueryW(PCWSTR::null(), 0, &mut query) };
        if status != PDH_CSTATUS_VALID_DATA || query.is_invalid() {
            return Err(Unavailable::NotSupported);
        }
        let mut counter = PDH_HCOUNTER(std::ptr::null_mut());
        // SAFETY: `query` es una consulta abierta arriba; `wide` es UTF-16 terminado en nulo que
        // vive hasta despues de la llamada; `counter` es un local valido para escritura.
        let status =
            unsafe { PdhAddEnglishCounterW(query, PCWSTR(wide.as_ptr()), 0, &mut counter) };
        if status != PDH_CSTATUS_VALID_DATA || counter.is_invalid() {
            // SAFETY: `query` esta abierta y no se usa despues de cerrarla.
            let _ = unsafe { PdhCloseQuery(query) };
            return Err(Unavailable::NotSupported);
        }
        let mut idle = Self { query, counter };
        // Linea base: la lectura de una tasa necesita dos colecciones.
        let _ = idle.collect();
        Ok(idle)
    }

    fn collect(&mut self) -> u32 {
        // SAFETY: `self.query` es una consulta abierta que este valor posee y cierra en `Drop`.
        unsafe { PdhCollectQueryData(self.query) }
    }

    /// Colecta y devuelve el tiempo activo (0..=100). `WarmingUp` si aun falta una coleccion.
    pub fn read_active_pct(&mut self) -> Reading<f32> {
        let status = self.collect();
        if status != PDH_CSTATUS_VALID_DATA {
            return Err(map_call_failure(status));
        }
        let mut value = PDH_FMT_COUNTERVALUE::default();
        // SAFETY: `self.counter` pertenece a `self.query` (abierta); `value` es un local valido
        // para escritura y se pide formato `double`; el tipo de contador se omite.
        let status =
            unsafe { PdhGetFormattedCounterValue(self.counter, PDH_FMT_DOUBLE, None, &mut value) };
        if status != PDH_CSTATUS_VALID_DATA {
            return Err(map_call_failure(status));
        }
        // SAFETY: la llamada tuvo exito pidiendo `PDH_FMT_DOUBLE`, asi que el miembro activo de
        // la union es `doubleValue`.
        let idle = unsafe { value.Anonymous.doubleValue };
        map_idle(value.CStatus, idle)
    }
}

impl Drop for PdhIdleTime {
    fn drop(&mut self) {
        // SAFETY: `self.query` esta abierta, es de este valor y no se usa despues; cerrarla
        // libera tambien el contador.
        let _ = unsafe { PdhCloseQuery(self.query) };
    }
}

#[cfg(test)]
#[path = "pdh_tests.rs"]
mod tests;

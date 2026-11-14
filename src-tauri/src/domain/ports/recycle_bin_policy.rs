//! Puerto de la politica de la Papelera (design D-12 B): hecho antes de mover, para que el shell
//! nunca destruya un archivo cuando la Papelera esta deshabilitada o el archivo no entra.
//!
//! La decision es una funcion pura sobre los valores crudos que lee el adaptador; cualquier
//! error de lectura distinto de "no existe" es `Err` y se trata como NO disponible (fail-closed).

use std::path::Path;

use super::error::PortError;
use super::trash::TrashError;

const BYTES_PER_MB: u64 = 1024 * 1024;

/// Valores crudos de registro de un volumen; `None` = el valor o la clave no existe (el default
/// de Windows: habilitada y con capacidad por defecto).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RawBinPolicy {
    /// `HKLM\...\Policies\Explorer\NoRecycleFiles`.
    pub no_recycle_files_machine: Option<u32>,
    /// `HKCU\...\Policies\Explorer\NoRecycleFiles`.
    pub no_recycle_files_user: Option<u32>,
    /// `HKCU\...\Explorer\BitBucket\NukeOnDelete` (global).
    pub nuke_on_delete_global: Option<u32>,
    /// `HKCU\...\Explorer\BitBucket\Volume\{GUID}\NukeOnDelete`.
    pub nuke_on_delete_volume: Option<u32>,
    /// `HKCU\...\Explorer\BitBucket\Volume\{GUID}\MaxCapacity`, en MB.
    pub max_capacity_mb: Option<u32>,
}

/// Por que la Papelera esta deshabilitada.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisabledReason {
    PolicyMachine,
    PolicyUser,
    NukeOnDeleteGlobal,
    NukeOnDeleteVolume,
}

/// Disponibilidad de la Papelera para un volumen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinAvailability {
    Disabled(DisabledReason),
    /// `max_capacity_bytes: None` = capacidad desconocida: no se omite por tamano.
    Enabled {
        max_capacity_bytes: Option<u64>,
    },
}

/// Cualquier valor distinto de 0 cuenta como "activado" (conservador).
fn is_set(value: Option<u32>) -> bool {
    value.is_some_and(|v| v != 0)
}

/// Decide la disponibilidad a partir de los valores crudos.
pub fn evaluate_policy(raw: &RawBinPolicy) -> BinAvailability {
    if is_set(raw.no_recycle_files_machine) {
        return BinAvailability::Disabled(DisabledReason::PolicyMachine);
    }
    if is_set(raw.no_recycle_files_user) {
        return BinAvailability::Disabled(DisabledReason::PolicyUser);
    }
    if is_set(raw.nuke_on_delete_global) {
        return BinAvailability::Disabled(DisabledReason::NukeOnDeleteGlobal);
    }
    if is_set(raw.nuke_on_delete_volume) {
        return BinAvailability::Disabled(DisabledReason::NukeOnDeleteVolume);
    }
    BinAvailability::Enabled {
        max_capacity_bytes: raw.max_capacity_mb.map(|mb| u64::from(mb) * BYTES_PER_MB),
    }
}

/// Si un archivo de `size_bytes` puede ir a la Papelera con esta disponibilidad.
pub fn check_move(availability: &BinAvailability, size_bytes: u64) -> Result<(), TrashError> {
    match availability {
        BinAvailability::Disabled(_) => Err(TrashError::BinUnavailable),
        BinAvailability::Enabled {
            max_capacity_bytes: Some(max),
        } if size_bytes > *max => Err(TrashError::TooLarge),
        BinAvailability::Enabled { .. } => Ok(()),
    }
}

/// Consulta la politica del volumen de `path` y decide; un error de lectura es `BinUnavailable`.
pub fn check_for_move<P: RecycleBinPolicy + ?Sized>(
    policy: &P,
    path: &Path,
    size_bytes: u64,
) -> Result<(), TrashError> {
    let availability = policy
        .availability(path)
        .map_err(|_| TrashError::BinUnavailable)?;
    check_move(&availability, size_bytes)
}

/// Lee la politica de la Papelera del volumen que contiene `path`.
pub trait RecycleBinPolicy {
    /// `Err` si la politica no pudo determinarse: el llamador MUST tratarlo como no disponible.
    fn availability(&self, path: &Path) -> Result<BinAvailability, PortError>;
}

impl<T: RecycleBinPolicy + ?Sized> RecycleBinPolicy for std::sync::Arc<T> {
    fn availability(&self, path: &Path) -> Result<BinAvailability, PortError> {
        (**self).availability(path)
    }
}

#[cfg(test)]
#[path = "recycle_bin_policy_tests.rs"]
mod tests;

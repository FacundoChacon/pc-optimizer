//! `WindowsSensors`: lecturas reales del sistema para el monitor (C-06, D-06). Solo lectura.
//!
//! - CPU y memoria: `sysinfo` (la CPU necesita dos lecturas: la primera es `WarmingUp`).
//! - Contadores acumulados de lectura/escritura: `sysinfo::Disks`, solo el disco cuyo punto de
//!   montaje es la raiz del volumen del sistema (`IOCTL_DISK_PERFORMANCE`).
//! - Espacio del volumen: `WindowsVolumeSpace` (C-07).
//! - Tiempo activo del disco: PDH (`pdh.rs`).
//!
//! No tiene `unsafe` propio (el unico esta en `pdh.rs`). Los handles de PDH no son `Send`: el
//! valor se construye en el hilo que lo usa.

use std::path::{Path, PathBuf};

use sysinfo::{DiskRefreshKind, Disks, MemoryRefreshKind, System};

use crate::adapters::windows::pdh::PdhIdleTime;
use crate::adapters::windows::volume::WindowsVolumeSpace;
use crate::domain::ports::{
    DiskIoCounters, MemoryReading, Reading, SensorReading, Sensors, Unavailable, VolumeReading,
    VolumeSpace,
};

/// Indice del disco montado en la raiz de `drive` (`X:\`); `NotSupported` si ninguno coincide.
/// Compara sin distinguir mayusculas y sin exigir la barra final.
pub(crate) fn select_system_disk(mount_points: &[PathBuf], drive: char) -> Reading<usize> {
    let wanted = drive.to_ascii_uppercase();
    mount_points
        .iter()
        .position(|mount| {
            let text = mount.to_string_lossy();
            let trimmed = text.trim_end_matches(['\\', '/']);
            let mut chars = trimmed.chars();
            matches!(
                (chars.next(), chars.next(), chars.next()),
                (Some(letter), Some(':'), None) if letter.to_ascii_uppercase() == wanted
            )
        })
        .ok_or(Unavailable::NotSupported)
}

/// Memoria usada = total - disponible (nunca mayor que el total). Total 0 = fuente rota.
pub(crate) fn memory_reading(total_bytes: u64, available_bytes: u64) -> Reading<MemoryReading> {
    if total_bytes == 0 {
        return Err(Unavailable::SourceError);
    }
    Ok(MemoryReading {
        used_bytes: total_bytes.saturating_sub(available_bytes).min(total_bytes),
        total_bytes,
    })
}

/// Espacio del volumen; cualquier falla de la consulta es `SourceError`.
pub(crate) fn volume_reading<V: VolumeSpace>(volume: &V, root: &Path) -> Reading<VolumeReading> {
    volume
        .space(root)
        .map(|info| VolumeReading {
            total_bytes: info.total_bytes,
            free_bytes: info.free_bytes,
        })
        .map_err(|_| Unavailable::SourceError)
}

pub struct WindowsSensors {
    system: System,
    disks: Disks,
    /// Indice del disco del sistema en `disks`, o el motivo por el que no hay.
    system_disk: Reading<usize>,
    root: PathBuf,
    cpu_primed: bool,
    pdh: Result<PdhIdleTime, Unavailable>,
    volume: WindowsVolumeSpace,
}

impl WindowsSensors {
    /// Prepara las fuentes del volumen `drive` (letra de la unidad del sistema). `Err` solo si
    /// `sysinfo` no puede inicializarse; las demas fuentes fallan por metrica.
    pub fn new(drive: char) -> Result<Self, Unavailable> {
        let system = System::new().map_err(|_| Unavailable::SourceError)?;
        let disks = Disks::new_with_refreshed_list_specifics(DiskRefreshKind::nothing())
            .map_err(|_| Unavailable::SourceError)?;
        let mounts: Vec<PathBuf> = disks
            .iter()
            .map(|disk| disk.mount_point().to_path_buf())
            .collect();
        Ok(Self {
            system,
            disks,
            system_disk: select_system_disk(&mounts, drive),
            root: PathBuf::from(format!(r"{}:\", drive.to_ascii_uppercase())),
            cpu_primed: false,
            pdh: PdhIdleTime::open(drive),
            volume: WindowsVolumeSpace,
        })
    }

    fn read_cpu(&mut self) -> Reading<f32> {
        self.system.refresh_cpu_usage();
        if !self.cpu_primed {
            // La primera lectura solo fija la base: sysinfo devuelve 0 y no es un valor real.
            self.cpu_primed = true;
            return Err(Unavailable::WarmingUp);
        }
        let usage = self.system.global_cpu_usage();
        if usage.is_finite() {
            Ok(usage.clamp(0.0, 100.0))
        } else {
            Err(Unavailable::SourceError)
        }
    }

    fn read_memory(&mut self) -> Reading<MemoryReading> {
        self.system
            .refresh_memory_specifics(MemoryRefreshKind::nothing().with_ram());
        memory_reading(self.system.total_memory(), self.system.available_memory())
    }

    fn read_disk_io(&mut self) -> Reading<DiskIoCounters> {
        let index = self.system_disk?;
        let disk = self.disks.get_mut(index).ok_or(Unavailable::SourceError)?;
        disk.refresh_specifics(DiskRefreshKind::nothing().with_io_usage());
        let usage = disk.usage();
        Ok(DiskIoCounters {
            read_bytes: usage.total_read_bytes,
            written_bytes: usage.total_written_bytes,
        })
    }
}

impl Sensors for WindowsSensors {
    fn read(&mut self) -> SensorReading {
        let disk_active_pct = match self.pdh.as_mut() {
            Ok(pdh) => pdh.read_active_pct(),
            Err(reason) => Err(*reason),
        };
        SensorReading {
            cpu_pct: self.read_cpu(),
            memory: self.read_memory(),
            volume: volume_reading(&self.volume, &self.root),
            disk_io: self.read_disk_io(),
            disk_active_pct,
        }
    }
}

#[cfg(test)]
#[path = "sensors_tests.rs"]
mod tests;

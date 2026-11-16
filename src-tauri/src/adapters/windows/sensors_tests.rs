//! Tests de `WindowsSensors`: la logica de eleccion y conversion es pura; la lectura de sensores
//! reales va con `#[ignore]`.

use std::path::{Path, PathBuf};

use super::*;
use crate::domain::ports::{
    MemoryReading, PortError, Unavailable, VolumeReading, VolumeSpace, VolumeSpaceInfo,
};

fn mounts(list: &[&str]) -> Vec<PathBuf> {
    list.iter().map(PathBuf::from).collect()
}

#[test]
fn the_system_disk_should_be_the_one_mounted_at_the_system_root() {
    let list = mounts(&[r"D:\", r"C:\", r"E:\"]);
    assert_eq!(select_system_disk(&list, 'C'), Ok(1));
    assert_eq!(select_system_disk(&list, 'E'), Ok(2));
}

#[test]
fn the_system_disk_match_should_ignore_case_and_a_missing_trailing_separator() {
    let list = mounts(&["c:", r"D:\"]);
    assert_eq!(select_system_disk(&list, 'C'), Ok(0));
    assert_eq!(select_system_disk(&list, 'd'), Ok(1));
}

#[test]
fn no_matching_mount_point_should_be_not_supported() {
    let list = mounts(&[r"D:\", r"E:\Datos"]);
    assert_eq!(
        select_system_disk(&list, 'C'),
        Err(Unavailable::NotSupported)
    );
    assert_eq!(select_system_disk(&[], 'C'), Err(Unavailable::NotSupported));
    // Un punto de montaje dentro de la unidad no es la raiz del volumen.
    assert_eq!(
        select_system_disk(&list, 'E'),
        Err(Unavailable::NotSupported)
    );
}

#[test]
fn memory_used_should_be_total_minus_available_and_never_above_total() {
    assert_eq!(
        memory_reading(16_000, 4_000),
        Ok(MemoryReading {
            used_bytes: 12_000,
            total_bytes: 16_000
        })
    );
    assert_eq!(
        memory_reading(8_000, 9_000),
        Ok(MemoryReading {
            used_bytes: 0,
            total_bytes: 8_000
        })
    );
}

#[test]
fn a_zero_total_memory_should_be_a_source_error_not_a_zero_reading() {
    assert_eq!(memory_reading(0, 0), Err(Unavailable::SourceError));
}

struct FixedVolume(Result<VolumeSpaceInfo, PortError>);

impl VolumeSpace for FixedVolume {
    fn space(&self, _root: &Path) -> Result<VolumeSpaceInfo, PortError> {
        self.0.clone()
    }
}

#[test]
fn volume_reading_should_map_the_space_and_a_failure_to_a_source_error() {
    let ok = FixedVolume(Ok(VolumeSpaceInfo {
        total_bytes: 100,
        free_bytes: 40,
    }));
    assert_eq!(
        volume_reading(&ok, Path::new(r"C:\")),
        Ok(VolumeReading {
            total_bytes: 100,
            free_bytes: 40
        })
    );
    let failing = FixedVolume(Err(PortError::Io("x".into())));
    assert_eq!(
        volume_reading(&failing, Path::new(r"C:\")),
        Err(Unavailable::SourceError)
    );
}

#[test]
#[ignore = "lee sensores reales de la maquina"]
fn real_sensors_should_read_plausible_values_on_the_second_reading() {
    let mut sensors = WindowsSensors::new('C').expect("sensores disponibles");
    let first = sensors.read();
    assert_eq!(first.cpu_pct, Err(Unavailable::WarmingUp));
    std::thread::sleep(
        sysinfo::MINIMUM_CPU_UPDATE_INTERVAL.max(std::time::Duration::from_millis(1_100)),
    );
    let second = sensors.read();
    let cpu = second
        .cpu_pct
        .expect("cpu disponible en la segunda lectura");
    assert!((0.0..=100.0).contains(&cpu), "{cpu}");
    let memory = second.memory.expect("memoria disponible");
    assert!(memory.used_bytes <= memory.total_bytes && memory.total_bytes > 0);
    let volume = second.volume.expect("espacio del volumen");
    assert!(volume.free_bytes <= volume.total_bytes);
}

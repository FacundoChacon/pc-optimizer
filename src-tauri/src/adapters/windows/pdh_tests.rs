//! Tests de `pdh`: el mapeo de estados es puro; la lectura real va con `#[ignore]`.

use windows::Win32::System::Performance::{PDH_CSTATUS_VALID_DATA, PDH_INVALID_DATA};

use super::*;
use crate::domain::ports::Unavailable;

#[test]
fn valid_idle_should_map_to_the_complement_as_active_time() {
    assert_eq!(map_idle(PDH_CSTATUS_VALID_DATA, 85.0), Ok(15.0));
    assert_eq!(map_idle(PDH_CSTATUS_VALID_DATA, 0.0), Ok(100.0));
    assert_eq!(map_idle(PDH_CSTATUS_VALID_DATA, 100.0), Ok(0.0));
}

#[test]
fn out_of_range_idle_should_be_bounded_between_0_and_100() {
    assert_eq!(map_idle(PDH_CSTATUS_VALID_DATA, 130.0), Ok(0.0));
    assert_eq!(map_idle(PDH_CSTATUS_VALID_DATA, -5.0), Ok(100.0));
}

#[test]
fn invalid_data_status_should_be_warming_up_and_other_errors_a_source_error() {
    assert_eq!(
        map_idle(PDH_INVALID_DATA, 50.0),
        Err(Unavailable::WarmingUp)
    );
    assert_eq!(map_idle(0xC000_0BB8, 50.0), Err(Unavailable::SourceError));
    assert_eq!(
        map_idle(PDH_CSTATUS_VALID_DATA, f64::NAN),
        Err(Unavailable::SourceError)
    );
}

#[test]
fn a_failed_call_status_should_map_invalid_data_to_warming_up_only() {
    assert_eq!(map_call_failure(PDH_INVALID_DATA), Unavailable::WarmingUp);
    assert_eq!(map_call_failure(0xC000_0BBB), Unavailable::SourceError);
}

#[test]
fn counter_path_should_use_the_volume_letter_in_upper_case() {
    assert_eq!(
        counter_path('C').as_deref(),
        Some(r"\LogicalDisk(C:)\% Idle Time")
    );
    assert_eq!(
        counter_path('d').as_deref(),
        Some(r"\LogicalDisk(D:)\% Idle Time")
    );
}

#[test]
fn counter_path_should_reject_anything_that_is_not_a_drive_letter() {
    assert_eq!(counter_path('1'), None);
    assert_eq!(counter_path('\\'), None);
    assert_eq!(counter_path('é'), None);
}

#[test]
#[ignore = "lee contadores reales de la maquina"]
fn real_counter_should_report_an_active_percentage_after_two_collections() {
    let mut idle = PdhIdleTime::open('C').expect("abre el contador del volumen C:");
    std::thread::sleep(std::time::Duration::from_millis(1_100));
    let value = idle.read_active_pct().expect("segunda coleccion valida");
    assert!((0.0..=100.0).contains(&value), "{value}");
}

#[test]
fn opening_a_counter_for_something_that_is_not_a_drive_letter_should_be_not_supported() {
    assert!(matches!(
        PdhIdleTime::open('7'),
        Err(Unavailable::NotSupported)
    ));
    assert!(matches!(
        PdhIdleTime::open('/'),
        Err(Unavailable::NotSupported)
    ));
}

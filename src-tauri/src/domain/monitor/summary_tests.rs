use super::*;
use crate::domain::model::EpochMillis;
use crate::domain::monitor::model::{Availability, SampleAvailability};

const T0: i64 = 1_700_000_000_000;

fn ok_availability() -> SampleAvailability {
    SampleAvailability {
        cpu: Availability::Ok,
        memory: Availability::Ok,
        volume: Availability::Ok,
        disk_active: Availability::Ok,
        disk_io: Availability::Ok,
    }
}

/// Muestra completa en `T0 + at_s` segundos.
fn sample(
    at_s: i64,
    cpu: Option<f32>,
    mem_used_of_100: Option<u64>,
    disk: Option<f32>,
) -> MonitorSample {
    MonitorSample {
        seq: u64::try_from(at_s).unwrap_or(0) + 1,
        at: EpochMillis(T0 + at_s * 1_000),
        gap_before: false,
        cpu_pct: cpu,
        mem_used_bytes: mem_used_of_100,
        mem_total_bytes: mem_used_of_100.map(|_| 100),
        volume_total_bytes: Some(1_000),
        volume_free_bytes: Some(500),
        disk_active_pct: disk,
        disk_read_bps: Some(0),
        disk_write_bps: Some(0),
        availability: ok_availability(),
        read_micros: 1_000,
    }
}

fn now(at_s: i64) -> EpochMillis {
    EpochMillis(T0 + at_s * 1_000)
}

/// Ventana de 60 muestras iguales (segundos 0..=59).
fn steady(cpu: f32, mem: u64, disk: f32) -> Vec<MonitorSample> {
    (0..60)
        .map(|s| sample(s, Some(cpu), Some(mem), Some(disk)))
        .collect()
}

fn summarize_steady(window: &[MonitorSample]) -> MonitorSummary {
    summarize(window.iter(), now(59)).unwrap()
}

#[test]
fn cpu_level_should_flip_at_the_documented_borders() {
    for (cpu, expected) in [
        (69.9_f32, Level::Normal),
        (70.0, Level::High),
        (89.9, Level::High),
        (90.0, Level::VeryHigh),
    ] {
        let summary = summarize_steady(&steady(cpu, 10, 5.0));
        assert_eq!(summary.cpu.level, expected, "cpu {cpu}");
    }
}

#[test]
fn memory_disk_and_space_levels_should_follow_their_thresholds() {
    let mem = summarize_steady(&steady(1.0, 85, 5.0));
    assert_eq!(mem.memory.level, Level::High);
    assert_eq!(
        summarize_steady(&steady(1.0, 79, 5.0)).memory.level,
        Level::Normal
    );
    assert_eq!(
        summarize_steady(&steady(1.0, 90, 5.0)).memory.level,
        Level::VeryHigh
    );

    assert_eq!(
        summarize_steady(&steady(1.0, 10, 95.0)).disk_active.level,
        Level::VeryHigh
    );
    assert_eq!(
        summarize_steady(&steady(1.0, 10, 60.0)).disk_active.level,
        Level::High
    );
    assert_eq!(
        summarize_steady(&steady(1.0, 10, 59.9)).disk_active.level,
        Level::Normal
    );
}

#[test]
fn volume_space_should_be_almost_full_from_90_percent_used() {
    let with_free = |free: u64| {
        let mut w = steady(1.0, 10, 5.0);
        for s in &mut w {
            s.volume_free_bytes = Some(free);
        }
        summarize_steady(&w).volume
    };
    assert_eq!(with_free(80).level, SpaceLevel::AlmostFull); // 92 % usado
    assert_eq!(with_free(100).level, SpaceLevel::AlmostFull); // 90,0 % usado
    assert_eq!(with_free(101).level, SpaceLevel::Normal); // 89,9 % usado
    assert_eq!(with_free(80).used_pct, Some(92.0));
}

#[test]
fn absent_values_should_be_ignored_and_all_absent_should_be_unknown_with_nulls() {
    let mut w = steady(50.0, 10, 5.0);
    for s in w.iter_mut().take(30) {
        s.cpu_pct = None;
    }
    let partial = summarize_steady(&w);
    assert_eq!(partial.cpu.avg60, Some(50.0));
    assert_eq!(partial.cpu.level, Level::Normal);

    let mut none = steady(50.0, 10, 5.0);
    for s in &mut none {
        s.cpu_pct = None;
    }
    let all_absent = summarize_steady(&none);
    assert_eq!(all_absent.cpu.current, None);
    assert_eq!(all_absent.cpu.avg60, None);
    assert_eq!(all_absent.cpu.peak, None);
    assert_eq!(all_absent.cpu.level, Level::Unknown);
}

#[test]
fn an_empty_window_should_have_no_summary() {
    assert_eq!(summarize(std::iter::empty(), now(0)), None);
}

#[test]
fn a_one_second_peak_should_not_change_the_level_but_should_be_the_peak() {
    let mut w = steady(10.0, 10, 5.0);
    w[20].cpu_pct = Some(100.0);
    let summary = summarize_steady(&w);
    assert_eq!(summary.cpu.peak, Some(100.0));
    assert_eq!(summary.cpu.level, Level::Normal);
    assert_eq!(summary.cpu.current, Some(10.0));
}

#[test]
fn average_should_only_cover_the_last_60_seconds_and_peak_the_whole_window() {
    // 120 s: los primeros 60 s al 100 %, los ultimos 60 s al 20 %.
    let w: Vec<MonitorSample> = (0..120)
        .map(|s| {
            sample(
                s,
                Some(if s < 60 { 100.0 } else { 20.0 }),
                Some(10),
                Some(1.0),
            )
        })
        .collect();
    let summary = summarize(w.iter(), now(119)).unwrap();
    assert_eq!(summary.cpu.avg60, Some(20.0));
    assert_eq!(summary.cpu.peak, Some(100.0));
    assert_eq!(summary.cpu.level, Level::Normal);
}

#[test]
fn gaps_and_read_cost_and_last_availability_should_be_reported() {
    let mut w = steady(10.0, 10, 5.0);
    w[10].gap_before = true;
    w[40].gap_before = true;
    w[59].availability.disk_active = Availability::NotSupported;
    w[59].disk_active_pct = None;
    for s in &mut w {
        s.read_micros = 2_000;
    }
    let summary = summarize_steady(&w);
    assert_eq!(summary.gaps_in_window, 2);
    assert_eq!(summary.avg_read_micros, Some(2_000));
    assert_eq!(summary.availability.disk_active, Availability::NotSupported);
    assert_eq!(summary.availability.cpu, Availability::Ok);
}

#[test]
fn samples_older_than_300_seconds_should_not_count_after_a_long_suspension() {
    // Muestras de los segundos 0..=59 y "ahora" 10 minutos despues: todo es mas viejo que 300 s.
    let w = steady(99.0, 95, 99.0);
    assert_eq!(summarize(w.iter(), now(59 + 600)), None);
    // Con una muestra nueva al 10 %, las viejas no aportan pico ni promedio.
    let mut w2 = w;
    w2.push(sample(659, Some(10.0), Some(10), Some(1.0)));
    let summary = summarize(w2.iter(), now(659)).unwrap();
    assert_eq!(summary.cpu.peak, Some(10.0));
    assert_eq!(summary.cpu.avg60, Some(10.0));
    assert_eq!(summary.cpu.level, Level::Normal);
}

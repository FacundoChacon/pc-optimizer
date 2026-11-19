use super::*;
use crate::domain::model::EpochMillis;
use crate::domain::ports::{
    DiskIoCounters, MemoryReading, SensorReading, Unavailable, VolumeReading,
};

const T0: i64 = 1_700_000_000_000;

fn reading(read: u64, written: u64) -> SensorReading {
    SensorReading {
        cpu_pct: Ok(40.0),
        memory: Ok(MemoryReading {
            used_bytes: 6,
            total_bytes: 8,
        }),
        volume: Ok(VolumeReading {
            total_bytes: 1_000,
            free_bytes: 250,
        }),
        disk_io: Ok(DiskIoCounters {
            read_bytes: read,
            written_bytes: written,
        }),
        disk_active_pct: Ok(12.5),
    }
}

fn tick(mono_ms: u64, wall_offset_ms: i64) -> Tick {
    Tick {
        mono_ms,
        wall: EpochMillis(T0 + wall_offset_ms),
        read_micros: 0,
    }
}

#[test]
fn first_reading_should_warm_up_interval_metrics_and_keep_instant_ones() {
    let mut engine = MonitorEngine::new(1_000);
    let out = engine.ingest(&reading(1_000_000, 500), tick(0, 0));
    let s = out.sample;
    assert_eq!(s.seq, 1);
    assert!(!s.gap_before);
    assert!(!out.clear_window);
    assert_eq!(s.cpu_pct, None);
    assert_eq!(s.disk_active_pct, None);
    assert_eq!((s.disk_read_bps, s.disk_write_bps), (None, None));
    assert_eq!(s.availability.cpu, Availability::WarmingUp);
    assert_eq!(s.availability.disk_active, Availability::WarmingUp);
    assert_eq!(s.availability.disk_io, Availability::WarmingUp);
    assert_eq!((s.mem_used_bytes, s.mem_total_bytes), (Some(6), Some(8)));
    assert_eq!(
        (s.volume_total_bytes, s.volume_free_bytes),
        (Some(1_000), Some(250))
    );
    assert_eq!(s.availability.memory, Availability::Ok);
    assert_eq!(s.availability.volume, Availability::Ok);
}

#[test]
fn second_reading_should_carry_source_values_and_compute_rates_from_counters() {
    let mut engine = MonitorEngine::new(1_000);
    engine.ingest(&reading(1_000_000, 0), tick(0, 0));
    let s = engine
        .ingest(&reading(3_000_000, 500_000), tick(1_000, 1_000))
        .sample;
    assert_eq!(s.cpu_pct, Some(40.0));
    assert_eq!(s.disk_active_pct, Some(12.5));
    assert_eq!(s.disk_read_bps, Some(2_000_000));
    assert_eq!(s.disk_write_bps, Some(500_000));
    assert_eq!(s.availability.disk_io, Availability::Ok);
}

#[test]
fn rates_should_scale_with_the_monotonic_delta() {
    let mut engine = MonitorEngine::new(1_000);
    engine.ingest(&reading(0, 0), tick(0, 0));
    let s = engine
        .ingest(&reading(3_000, 1_500), tick(1_500, 1_500))
        .sample;
    assert_eq!(s.disk_read_bps, Some(2_000));
    assert_eq!(s.disk_write_bps, Some(1_000));
}

#[test]
fn a_counter_that_goes_backwards_should_make_only_that_rate_absent() {
    let mut engine = MonitorEngine::new(1_000);
    engine.ingest(&reading(5_000, 100), tick(0, 0));
    let s = engine
        .ingest(&reading(4_000, 900), tick(1_000, 1_000))
        .sample;
    assert_eq!(s.disk_read_bps, None);
    assert_eq!(s.disk_write_bps, Some(800));
    assert_eq!(s.availability.disk_io, Availability::SourceError);
}

#[test]
fn a_zero_monotonic_delta_should_make_rates_absent() {
    let mut engine = MonitorEngine::new(1_000);
    engine.ingest(&reading(0, 0), tick(500, 0));
    let s = engine.ingest(&reading(10, 10), tick(500, 10)).sample;
    assert_eq!((s.disk_read_bps, s.disk_write_bps), (None, None));
    assert_eq!(s.availability.disk_io, Availability::SourceError);
}

#[test]
fn seq_should_increase_strictly_on_every_reading() {
    let mut engine = MonitorEngine::new(1_000);
    let seqs: Vec<u64> = (0..4)
        .map(|n| {
            engine
                .ingest(
                    &reading(n, n),
                    tick(n * 1_000, i64::try_from(n * 1_000).unwrap_or(0)),
                )
                .sample
                .seq
        })
        .collect();
    assert_eq!(seqs, vec![1, 2, 3, 4]);
}

#[test]
fn a_failing_source_should_only_blank_its_own_metric_with_its_reason() {
    let mut engine = MonitorEngine::new(1_000);
    engine.ingest(&reading(0, 0), tick(0, 0));
    let mut r = reading(1_000, 1_000);
    r.disk_active_pct = Err(Unavailable::SourceError);
    r.volume = Err(Unavailable::NotSupported);
    let s = engine.ingest(&r, tick(1_000, 1_000)).sample;
    assert_eq!(s.disk_active_pct, None);
    assert_eq!(s.availability.disk_active, Availability::SourceError);
    assert_eq!(s.volume_total_bytes, None);
    assert_eq!(s.availability.volume, Availability::NotSupported);
    assert_eq!(s.cpu_pct, Some(40.0));
    assert_eq!(s.mem_used_bytes, Some(6));
}

#[test]
fn out_of_range_values_should_be_bounded_or_rejected_never_invented() {
    let mut engine = MonitorEngine::new(1_000);
    engine.ingest(&reading(0, 0), tick(0, 0));
    let mut r = reading(10, 10);
    r.cpu_pct = Ok(130.0);
    r.disk_active_pct = Ok(f32::NAN);
    r.memory = Ok(MemoryReading {
        used_bytes: 20,
        total_bytes: 8,
    });
    let s = engine.ingest(&r, tick(1_000, 1_000)).sample;
    assert_eq!(s.cpu_pct, Some(100.0));
    assert_eq!(s.disk_active_pct, None);
    assert_eq!(s.availability.disk_active, Availability::SourceError);
    assert_eq!(s.mem_used_bytes, Some(8));
}

#[test]
fn the_read_cost_should_travel_with_the_sample() {
    let mut engine = MonitorEngine::new(1_000);
    let mut t = tick(0, 0);
    t.read_micros = 2_000;
    assert_eq!(engine.ingest(&reading(0, 0), t).sample.read_micros, 2_000);
}

// --- Huecos (D-07) ---

#[test]
fn a_long_monotonic_delta_should_mark_a_gap_and_drop_interval_metrics_but_keep_instant_ones() {
    let mut engine = MonitorEngine::new(1_000);
    engine.ingest(&reading(0, 0), tick(0, 0));
    let out = engine.ingest(&reading(9_000, 9_000), tick(3_000, 3_000));
    let s = out.sample;
    assert!(s.gap_before);
    assert!(!out.clear_window);
    assert_eq!(s.cpu_pct, None);
    assert_eq!(s.disk_active_pct, None);
    assert_eq!((s.disk_read_bps, s.disk_write_bps), (None, None));
    assert_eq!(s.availability.cpu, Availability::AfterGap);
    assert_eq!(s.availability.disk_active, Availability::AfterGap);
    assert_eq!(s.availability.disk_io, Availability::AfterGap);
    assert_eq!(s.mem_used_bytes, Some(6));
    assert_eq!(s.volume_free_bytes, Some(250));
    assert_eq!(s.availability.memory, Availability::Ok);
}

#[test]
fn wall_clock_jump_with_a_normal_monotonic_delta_should_mark_a_gap() {
    // Suspension que `Instant` no cuenta: 1 s monotonico, 10 min de pared.
    let mut engine = MonitorEngine::new(1_000);
    engine.ingest(&reading(0, 0), tick(0, 0));
    let out = engine.ingest(&reading(10, 10), tick(1_000, 600_000));
    assert!(out.sample.gap_before);
    assert!(!out.clear_window);
    assert_eq!(out.sample.availability.cpu, Availability::AfterGap);
}

#[test]
fn wall_clock_going_backwards_should_mark_a_gap_and_clear_the_window() {
    let mut engine = MonitorEngine::new(1_000);
    engine.ingest(&reading(0, 0), tick(0, 10_000));
    let out = engine.ingest(&reading(10, 10), tick(1_000, 4_000));
    assert!(out.sample.gap_before);
    assert!(out.clear_window);
}

#[test]
fn small_jitter_should_not_mark_a_gap() {
    let mut engine = MonitorEngine::new(1_000);
    engine.ingest(&reading(0, 0), tick(0, 0));
    let a = engine.ingest(&reading(1_200, 0), tick(1_200, 1_150));
    assert!(!a.sample.gap_before);
    assert_eq!(a.sample.cpu_pct, Some(40.0));
    // 2,5 intervalos exactos todavia es tolerable.
    let b = engine.ingest(&reading(3_700, 0), tick(3_700, 3_650));
    assert!(!b.sample.gap_before);
}

#[test]
fn the_sample_after_a_gap_should_resume_rates_from_the_gap_reading() {
    let mut engine = MonitorEngine::new(1_000);
    engine.ingest(&reading(0, 0), tick(0, 0));
    engine.ingest(&reading(1_000_000, 0), tick(10_000, 10_000));
    let s = engine
        .ingest(&reading(1_002_000, 0), tick(11_000, 11_000))
        .sample;
    assert!(!s.gap_before);
    assert_eq!(s.disk_read_bps, Some(2_000));
}

use super::*;
use crate::domain::model::EpochMillis;
use crate::domain::ports::fakes::{FakeMonotonicClock, FakeSensors, FakeTicker, FakeWallClock};
use crate::domain::ports::{
    DiskIoCounters, MemoryReading, MonotonicClock, SensorReading, Sensors, Unavailable,
    VolumeReading,
};

const WALL0: i64 = 1_700_000_000_000;

fn reading(n: u64) -> SensorReading {
    SensorReading {
        cpu_pct: Ok(30.0),
        memory: Ok(MemoryReading {
            used_bytes: 4,
            total_bytes: 8,
        }),
        volume: Ok(VolumeReading {
            total_bytes: 100,
            free_bytes: 60,
        }),
        disk_io: Ok(DiskIoCounters {
            read_bytes: n * 1_000,
            written_bytes: n * 500,
        }),
        disk_active_pct: Ok(10.0),
    }
}

fn script(count: u64) -> Vec<SensorReading> {
    (0..count).map(reading).collect()
}

struct Rig {
    mono: FakeMonotonicClock,
    wall: FakeWallClock,
}

fn rig() -> Rig {
    Rig {
        mono: FakeMonotonicClock::new(5_000),
        wall: FakeWallClock::new(WALL0),
    }
}

fn run<S: Sensors>(
    rig: &Rig,
    sensors: S,
    cycles: usize,
) -> (
    Sampler<S, FakeWallClock, FakeMonotonicClock>,
    Vec<MonitorSample>,
    FakeTicker,
) {
    let mut sampler = Sampler::new(sensors, rig.wall.clone(), rig.mono.clone(), 1_000);
    let mut ticker = FakeTicker::new(rig.mono.clone(), rig.wall.clone(), cycles);
    let mut seen = Vec::new();
    run_sampler(&mut sampler, &mut ticker, |cycle| {
        seen.push(cycle.sample.clone());
    });
    (sampler, seen, ticker)
}

#[test]
fn ten_cycles_should_produce_ten_samples_spaced_by_the_interval_with_a_summary() {
    let rig = rig();
    let (sampler, seen, _) = run(&rig, FakeSensors::new(script(10)), 10);
    assert_eq!(
        seen.iter().map(|s| s.seq).collect::<Vec<_>>(),
        (1..=10).collect::<Vec<_>>()
    );
    assert_eq!(sampler.window().len(), 10);
    for pair in seen.windows(2) {
        assert_eq!(pair[1].at.0 - pair[0].at.0, 1_000);
    }
    assert!(sampler.window().summary().is_some());
    assert_eq!(seen[0].at, EpochMillis(WALL0));
}

#[test]
fn the_cadence_should_be_requested_to_the_ticker_without_drift() {
    let rig = rig();
    let (_, _, ticker) = run(&rig, FakeSensors::new(script(4)), 4);
    assert_eq!(ticker.deadlines(), vec![5_000, 6_000, 7_000, 8_000]);
}

#[test]
fn read_cost_should_be_measured_with_the_monotonic_clock() {
    struct SlowSensors {
        inner: FakeSensors,
        mono: FakeMonotonicClock,
    }
    impl Sensors for SlowSensors {
        fn read(&mut self) -> SensorReading {
            self.mono.advance(2);
            self.inner.read()
        }
    }
    let rig = rig();
    let slow = SlowSensors {
        inner: FakeSensors::new(script(5)),
        mono: rig.mono.clone(),
    };
    let (sampler, seen, _) = run(&rig, slow, 5);
    assert!(seen.iter().all(|s| s.read_micros == 2_000));
    assert_eq!(
        sampler.window().summary().map(|s| s.avg_read_micros),
        Some(Some(2_000))
    );
}

#[test]
fn a_failing_sensor_in_one_cycle_should_only_blank_that_metric_in_that_sample() {
    let rig = rig();
    let mut readings = script(5);
    readings[2].disk_active_pct = Err(Unavailable::SourceError);
    let (_, seen, _) = run(&rig, FakeSensors::new(readings), 5);
    assert_eq!(seen[2].disk_active_pct, None);
    assert_eq!(seen[2].cpu_pct, Some(30.0));
    assert_eq!(seen[1].disk_active_pct, Some(10.0));
    assert_eq!(seen[3].disk_active_pct, Some(10.0));
}

#[test]
fn the_loop_should_end_when_the_ticker_stops() {
    let rig = rig();
    let (sampler, seen, _) = run(&rig, FakeSensors::new(script(50)), 3);
    assert_eq!(seen.len(), 3);
    assert_eq!(sampler.window().len(), 3);
}

#[test]
fn the_window_should_keep_300_samples_after_301_cycles() {
    let rig = rig();
    let (sampler, seen, _) = run(&rig, FakeSensors::new(script(301)), 301);
    assert_eq!(seen.len(), 301);
    assert_eq!(sampler.window().len(), 300);
    let seqs: Vec<u64> = sampler.window().iter().map(|s| s.seq).collect();
    assert_eq!(seqs.first(), Some(&2));
    assert_eq!(seqs.last(), Some(&301));
}

#[test]
fn a_wall_clock_going_backwards_should_clear_the_window_and_report_it() {
    let rig = rig();
    let mut sampler = Sampler::new(
        FakeSensors::new(script(3)),
        rig.wall.clone(),
        rig.mono.clone(),
        1_000,
    );
    assert!(sampler.run_cycle().is_some_and(|c| !c.window_cleared));
    rig.mono.advance(1_000);
    rig.wall.advance(1_000);
    assert!(sampler.run_cycle().is_some_and(|c| !c.window_cleared));
    rig.mono.advance(1_000);
    rig.wall.set(WALL0 - 60_000);
    let third = sampler.run_cycle();
    assert!(third.is_some_and(|c| c.window_cleared && c.sample.gap_before));
    assert_eq!(sampler.window().len(), 1);
}

#[test]
fn the_monotonic_fake_should_report_microseconds_consistent_with_milliseconds() {
    let mono = FakeMonotonicClock::new(3);
    assert_eq!(mono.now_micros(), 3_000);
}

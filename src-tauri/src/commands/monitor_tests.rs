use std::sync::{Arc, Mutex};

use serde_json::json;

use super::*;
use crate::adapters::clock::SystemMonotonicClock;
use crate::domain::monitor::runner::{CycleResult, Sampler, run_sampler};
use crate::domain::ports::fakes::{FakeMonotonicClock, FakeSensors, FakeTicker, FakeWallClock};
use crate::domain::ports::{
    DiskIoCounters, MemoryReading, SensorReading, Unavailable, VolumeReading,
};

const WALL0: i64 = 1_800_000_000_000;

fn reading(n: u64) -> SensorReading {
    SensorReading {
        cpu_pct: Ok(30.04),
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

/// Produce `count` ciclos reales del muestreador con sensores y relojes dobles.
fn cycles_with(count: usize, tweak: impl Fn(usize, &mut SensorReading)) -> Vec<CycleResult> {
    let mono = FakeMonotonicClock::new(1_000);
    let wall = FakeWallClock::new(WALL0);
    let script: Vec<SensorReading> = (0..count)
        .map(|n| {
            let mut r = reading(n as u64);
            tweak(n, &mut r);
            r
        })
        .collect();
    let mut sampler = Sampler::new(FakeSensors::new(script), wall.clone(), mono.clone(), 1_000);
    let mut ticker = FakeTicker::new(mono, wall, count);
    let mut out = Vec::new();
    run_sampler(&mut sampler, &mut ticker, |c| out.push(c.clone()));
    out
}

fn cycles(count: usize) -> Vec<CycleResult> {
    cycles_with(count, |_, _| {})
}

type Received = Arc<Mutex<Vec<MonitorEventDto>>>;

fn recorder() -> (Received, SendFn) {
    let received: Received = Arc::default();
    let sink = Arc::clone(&received);
    let send: SendFn = Box::new(move |event| {
        sink.lock().unwrap().push(event.clone());
        Ok(())
    });
    (received, send)
}

fn seq_of(event: &MonitorEventDto) -> u64 {
    match event {
        MonitorEventDto::Sample { sample, .. } => sample.seq,
        MonitorEventDto::Stopped { .. } => 0,
    }
}

#[test]
fn subscribing_with_120_samples_should_answer_them_and_continue_with_the_next_seq() {
    let hub = MonitorHub::new();
    let all = cycles(121);
    for cycle in &all[..120] {
        hub.push(cycle);
    }
    let (received, send) = recorder();
    let snapshot = hub.subscribe(send).unwrap();
    assert_eq!(snapshot.samples.len(), 120);
    assert_eq!(snapshot.samples.first().map(|s| s.seq), Some(1));
    assert_eq!(snapshot.samples.last().map(|s| s.seq), Some(120));
    assert!(snapshot.summary.is_some());

    hub.push(&all[120]);
    let events = received.lock().unwrap();
    assert_eq!(events.iter().map(seq_of).collect::<Vec<_>>(), vec![121]);
}

#[test]
fn subscribing_with_an_empty_window_should_answer_no_samples_and_a_null_summary() {
    let hub = MonitorHub::new();
    let (received, send) = recorder();
    let snapshot = hub.subscribe(send).unwrap();
    assert!(snapshot.samples.is_empty());
    assert!(snapshot.summary.is_none());
    hub.push(&cycles(1)[0]);
    assert_eq!(received.lock().unwrap().len(), 1);
}

#[test]
fn a_second_subscription_should_replace_the_first_one() {
    let hub = MonitorHub::new();
    let (first, send_first) = recorder();
    let (second, send_second) = recorder();
    let id_first = hub.subscribe(send_first).unwrap().subscription_id;
    let id_second = hub.subscribe(send_second).unwrap().subscription_id;
    assert_ne!(id_first, id_second);
    for cycle in cycles(3) {
        hub.push(&cycle);
    }
    assert!(first.lock().unwrap().is_empty());
    assert_eq!(second.lock().unwrap().len(), 3);
}

#[test]
fn unsubscribing_with_a_stale_id_should_not_affect_the_current_subscription() {
    let hub = MonitorHub::new();
    let (_first, send_first) = recorder();
    let (second, send_second) = recorder();
    let stale = hub.subscribe(send_first).unwrap().subscription_id;
    let current = hub.subscribe(send_second).unwrap().subscription_id;
    hub.unsubscribe(stale);
    hub.push(&cycles(1)[0]);
    assert_eq!(second.lock().unwrap().len(), 1);

    hub.unsubscribe(current);
    hub.unsubscribe(current);
    hub.push(&cycles(2)[1]);
    assert_eq!(second.lock().unwrap().len(), 1);
}

#[test]
fn a_failing_send_should_drop_the_subscriber_and_the_next_push_should_not_fail() {
    let hub = MonitorHub::new();
    let calls = Arc::new(Mutex::new(0_u32));
    let counter = Arc::clone(&calls);
    let send: SendFn = Box::new(move |_| {
        *counter.lock().unwrap() += 1;
        Err(())
    });
    hub.subscribe(send).unwrap();
    for cycle in cycles(3) {
        hub.push(&cycle);
    }
    assert_eq!(*calls.lock().unwrap(), 1);
    assert_eq!(hub.window_len(), 3);
}

#[test]
fn a_failed_hub_should_answer_monitor_unavailable_to_new_subscriptions() {
    let hub = MonitorHub::new();
    hub.mark_failed();
    let (_received, send) = recorder();
    assert!(matches!(
        hub.subscribe(send),
        Err(AppError::MonitorUnavailable)
    ));
    assert!(hub.is_failed());
}

#[test]
fn marking_the_hub_failed_should_tell_the_subscriber_once_and_release_it() {
    let hub = MonitorHub::new();
    let (received, send) = recorder();
    hub.subscribe(send).unwrap();
    hub.mark_failed();
    hub.mark_failed();
    hub.push(&cycles(1)[0]);
    let events = received.lock().unwrap();
    assert_eq!(
        events.as_slice(),
        &[MonitorEventDto::Stopped {
            reason: "sampler_failed"
        }]
    );
    assert_eq!(
        serde_json::to_value(&events[0]).unwrap(),
        json!({ "kind": "stopped", "reason": "sampler_failed" })
    );
}

#[test]
fn a_missing_sensor_should_not_stop_the_flow() {
    let hub = MonitorHub::new();
    let (received, send) = recorder();
    hub.subscribe(send).unwrap();
    for cycle in cycles_with(3, |n, r| {
        if n == 1 {
            r.cpu_pct = Err(Unavailable::SourceError);
        }
    }) {
        hub.push(&cycle);
    }
    assert_eq!(received.lock().unwrap().len(), 3);
}

// --- 5.3: hilo de muestreo ---

use std::sync::mpsc;
use std::time::Duration;

use crate::domain::monitor::runner::Ticker;

/// Sensores que entran en panico en la lectura numero `panic_at` (1-based).
struct PanicAt {
    inner: FakeSensors,
    calls: usize,
    panic_at: usize,
}

impl Sensors for PanicAt {
    fn read(&mut self) -> SensorReading {
        self.calls += 1;
        assert!(self.calls != self.panic_at, "fallo simulado del sensor");
        self.inner.read()
    }
}

fn panicking_rig(
    panic_at: usize,
    waits: usize,
) -> impl FnOnce() -> Option<(
    Sampler<PanicAt, FakeWallClock, FakeMonotonicClock>,
    FakeTicker,
)> + Send
+ 'static {
    move || {
        let mono = FakeMonotonicClock::new(1_000);
        let wall = FakeWallClock::new(WALL0);
        let script = (0..10).map(|n| reading(n as u64)).collect();
        let sensors = PanicAt {
            inner: FakeSensors::new(script),
            calls: 0,
            panic_at,
        };
        let sampler = Sampler::new(sensors, wall.clone(), mono.clone(), 1_000);
        Some((sampler, FakeTicker::new(mono, wall, waits)))
    }
}

#[test]
fn a_panic_in_the_third_cycle_should_fail_the_hub_notify_the_subscriber_and_end_the_thread() {
    let hub = MonitorHub::new();
    let (received, send) = recorder();
    hub.subscribe(send).unwrap();
    let handle = spawn_sampler_with(hub.clone(), panicking_rig(3, 10)).unwrap();
    handle.join().unwrap();

    assert!(hub.is_failed());
    let events = received.lock().unwrap();
    let kinds: Vec<&str> = events
        .iter()
        .map(|e| match e {
            MonitorEventDto::Sample { .. } => "sample",
            MonitorEventDto::Stopped { .. } => "stopped",
        })
        .collect();
    assert_eq!(kinds, vec!["sample", "sample", "stopped"]);
    assert_eq!(hub.window_len(), 2);
}

#[test]
fn a_sampler_without_panics_should_end_when_the_ticker_stops_and_leave_the_hub_healthy() {
    let hub = MonitorHub::new();
    let handle = spawn_sampler_with(hub.clone(), panicking_rig(99, 5)).unwrap();
    handle.join().unwrap();
    assert!(!hub.is_failed());
    assert_eq!(hub.window_len(), 5);
    // El hub sigue aceptando datos de otro origen.
    hub.push(&cycles(1)[0]);
    assert_eq!(hub.window_len(), 6);
}

#[test]
fn the_real_ticker_should_stop_when_the_stop_sender_is_dropped_or_signalled() {
    let mono = SystemMonotonicClock::new();
    let (tx, rx) = mpsc::channel::<()>();
    let mut ticker = RecvTicker::new(rx, mono);
    drop(tx);
    assert!(!ticker.wait_until(mono.now_ms() + 60_000));

    let (tx2, rx2) = mpsc::channel::<()>();
    let mut ticker2 = RecvTicker::new(rx2, mono);
    tx2.send(()).unwrap();
    assert!(!ticker2.wait_until(mono.now_ms() + 60_000));
}

#[test]
fn the_real_ticker_should_continue_after_the_deadline_without_a_stop_signal() {
    let mono = SystemMonotonicClock::new();
    let (tx, rx) = mpsc::channel::<()>();
    let mut ticker = RecvTicker::new(rx, mono);
    let started = std::time::Instant::now();
    assert!(ticker.wait_until(mono.now_ms() + 20));
    assert!(started.elapsed() >= Duration::from_millis(15));
    // Plazo ya vencido: no espera.
    assert!(ticker.wait_until(0));
    drop(tx);
}

#[test]
fn a_builder_without_sensors_should_leave_the_hub_failed_and_end_the_thread() {
    let hub = MonitorHub::new();
    let handle = spawn_sampler_with(hub.clone(), || -> Option<(
        Sampler<FakeSensors, FakeWallClock, FakeMonotonicClock>,
        FakeTicker,
    )> {
        None
    })
    .unwrap();
    handle.join().unwrap();
    assert!(hub.is_failed());

    let hub2 = MonitorHub::new();
    let (_tx, rx) = mpsc::channel::<()>();
    let handle2 = spawn_sampler::<FakeSensors, _>(hub2.clone(), || None, rx).unwrap();
    handle2.join().unwrap();
    assert!(hub2.is_failed());
}

// --- 5.4: DTOs y comandos ---

fn two_samples() -> MonitorHub {
    let hub = MonitorHub::new();
    for cycle in cycles(2) {
        hub.push(&cycle);
    }
    hub
}

#[test]
fn the_snapshot_should_serialize_with_the_exact_camel_case_contract() {
    let hub = two_samples();
    let (_received, send) = recorder();
    let snapshot = start_monitor_stream_run(&hub, send).unwrap();
    assert_eq!(
        serde_json::to_value(&snapshot).unwrap(),
        json!({
            "subscriptionId": 1,
            "intervalMs": 1000,
            "windowMs": 300000,
            "samples": [
                {
                    "seq": 1, "at": WALL0, "gapBefore": false,
                    "cpuPct": null,
                    "memUsedBytes": 4, "memTotalBytes": 8,
                    "volumeTotalBytes": 100, "volumeFreeBytes": 60,
                    "diskActivePct": null, "diskReadBps": null, "diskWriteBps": null
                },
                {
                    "seq": 2, "at": WALL0 + 1000, "gapBefore": false,
                    "cpuPct": 30.0,
                    "memUsedBytes": 4, "memTotalBytes": 8,
                    "volumeTotalBytes": 100, "volumeFreeBytes": 60,
                    "diskActivePct": 10.0, "diskReadBps": 1000, "diskWriteBps": 500
                }
            ],
            "summary": {
                "cpu": { "current": 30.0, "avg60": 30.0, "peak": 30.0, "level": "normal" },
                "memory": { "current": 50.0, "avg60": 50.0, "peak": 50.0, "level": "normal" },
                "diskActive": { "current": 10.0, "avg60": 10.0, "peak": 10.0, "level": "normal" },
                "volume": { "usedPct": 40.0, "level": "normal" },
                "availability": {
                    "cpu": "ok", "memory": "ok", "volume": "ok",
                    "diskActive": "ok", "diskIo": "ok"
                },
                "gapsInWindow": 0,
                "avgReadMicros": 0
            }
        })
    );
}

#[test]
fn an_empty_window_snapshot_should_have_a_null_summary_and_no_samples() {
    let hub = MonitorHub::new();
    let (_received, send) = recorder();
    let snapshot = start_monitor_stream_run(&hub, send).unwrap();
    let value = serde_json::to_value(&snapshot).unwrap();
    assert_eq!(value["samples"], json!([]));
    assert_eq!(value["summary"], serde_json::Value::Null);
    assert_eq!(value["intervalMs"], 1000);
    assert_eq!(value["windowMs"], 300_000);
}

#[test]
fn the_sample_event_should_carry_the_kind_the_sample_and_its_summary() {
    let hub = MonitorHub::new();
    let (received, send) = recorder();
    start_monitor_stream_run(&hub, send).unwrap();
    hub.push(&cycles(1)[0]);
    let events = received.lock().unwrap();
    let value = serde_json::to_value(&events[0]).unwrap();
    assert_eq!(value["kind"], "sample");
    assert_eq!(value["sample"]["seq"], 1);
    assert_eq!(value["sample"]["cpuPct"], serde_json::Value::Null);
    assert_eq!(value["summary"]["availability"]["cpu"], "warming_up");
    assert_eq!(value["summary"]["cpu"]["level"], "unknown");
    assert_eq!(value["summary"]["cpu"]["current"], serde_json::Value::Null);
    assert_eq!(value["summary"]["availability"]["diskIo"], "warming_up");
}

#[test]
fn absent_metrics_should_be_null_never_zero_and_percentages_have_one_decimal() {
    let hub = MonitorHub::new();
    let (received, send) = recorder();
    start_monitor_stream_run(&hub, send).unwrap();
    for cycle in cycles_with(2, |n, r| {
        if n == 1 {
            r.disk_active_pct = Err(Unavailable::NotSupported);
            r.cpu_pct = Ok(33.349);
        }
    }) {
        hub.push(&cycle);
    }
    let events = received.lock().unwrap();
    let value = serde_json::to_value(&events[1]).unwrap();
    assert_eq!(value["sample"]["diskActivePct"], serde_json::Value::Null);
    assert_eq!(
        value["summary"]["availability"]["diskActive"],
        "not_supported"
    );
    assert_eq!(value["sample"]["cpuPct"], 33.3);
}

#[test]
fn a_subscription_id_that_is_not_a_js_safe_integer_should_be_invalid_argument() {
    let hub = MonitorHub::new();
    for bad in [
        -1.0,
        1.5,
        f64::NAN,
        f64::INFINITY,
        9_007_199_254_740_992.0,
        1e300,
    ] {
        let error = stop_monitor_stream_run(&hub, bad).unwrap_err();
        assert!(
            matches!(error, AppError::InvalidArgument(_)),
            "{bad}: {error:?}"
        );
    }
}

#[test]
fn a_valid_subscription_id_should_be_accepted_even_if_unknown() {
    let hub = MonitorHub::new();
    assert!(stop_monitor_stream_run(&hub, 0.0).is_ok());
    assert!(stop_monitor_stream_run(&hub, 9_007_199_254_740_991.0).is_ok());
    let (received, send) = recorder();
    let id = start_monitor_stream_run(&hub, send)
        .unwrap()
        .subscription_id;
    assert!(stop_monitor_stream_run(&hub, id as f64).is_ok());
    hub.push(&cycles(1)[0]);
    assert!(received.lock().unwrap().is_empty());
}

#[test]
fn subscribing_while_a_scan_is_running_should_be_accepted() {
    use crate::commands::scan::ActiveJob;
    let active = ActiveJob::default();
    let _scan = active.begin_scan().unwrap();
    let hub = two_samples();
    let (_received, send) = recorder();
    assert_eq!(
        start_monitor_stream_run(&hub, send).unwrap().samples.len(),
        2
    );
}

#[test]
fn starting_a_scan_with_an_active_subscription_should_not_be_blocked_by_the_monitor() {
    use crate::commands::scan::ActiveJob;
    let hub = MonitorHub::new();
    let (_received, send) = recorder();
    start_monitor_stream_run(&hub, send).unwrap();
    let active = ActiveJob::default();
    assert!(active.begin_scan().is_ok());
}

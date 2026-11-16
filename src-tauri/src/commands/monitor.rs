//! Monitor en vivo (C-06): hub de muestras, hilo de muestreo y comandos IPC del flujo.
//!
//! - El muestreo corre siempre desde el arranque en un hilo propio; el `MonitorHub` guarda la
//!   ventana y entrega cada muestra al UNICO suscriptor por el `Channel` de su invocacion.
//! - El monitor NO es un `ActiveJob` (D-03) ni usa la base (D-16): nada se persiste y funciona con
//!   cualquier escaneo en curso, con la base caida y con el contexto de privilegios `mismatch`.
//! - Las respuestas no llevan rutas ni datos de usuario (solo metricas de la maquina).

use std::panic::{self, AssertUnwindSafe};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde::Serialize;
use tauri::State;
use tauri::ipc::Channel;

use crate::adapters::clock::{SystemClock, SystemMonotonicClock};
use crate::domain::monitor::model::{
    Availability, INTERVAL_MS, Level, MetricSummary, MonitorSample, MonitorSummary,
    SampleAvailability, SpaceLevel, VolumeSummary, WINDOW_MAX_AGE_MS,
};
use crate::domain::monitor::runner::{CycleResult, Sampler, Ticker, run_sampler};
use crate::domain::monitor::window::SampleWindow;
use crate::domain::ports::{Clock, MonotonicClock, Sensors};
use crate::error::AppError;

/// Mayor entero que JavaScript representa sin perdida (`Number.MAX_SAFE_INTEGER`).
pub const MAX_SAFE_INTEGER: u64 = (1 << 53) - 1;

/// Redondea a un decimal.
fn one_decimal(value: f32) -> f64 {
    (f64::from(value) * 10.0).round() / 10.0
}

fn safe_int(value: u64) -> u64 {
    value.min(MAX_SAFE_INTEGER)
}

// ---------------------------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------------------------

/// Muestra para la interfaz; cada metrica es `null` si esta ausente (nunca 0).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorSampleDto {
    pub seq: u64,
    pub at: i64,
    pub gap_before: bool,
    pub cpu_pct: Option<f64>,
    pub mem_used_bytes: Option<u64>,
    pub mem_total_bytes: Option<u64>,
    pub volume_total_bytes: Option<u64>,
    pub volume_free_bytes: Option<u64>,
    pub disk_active_pct: Option<f64>,
    pub disk_read_bps: Option<u64>,
    pub disk_write_bps: Option<u64>,
}

impl From<&MonitorSample> for MonitorSampleDto {
    fn from(sample: &MonitorSample) -> Self {
        Self {
            seq: safe_int(sample.seq),
            at: sample.at.0,
            gap_before: sample.gap_before,
            cpu_pct: sample.cpu_pct.map(one_decimal),
            mem_used_bytes: sample.mem_used_bytes.map(safe_int),
            mem_total_bytes: sample.mem_total_bytes.map(safe_int),
            volume_total_bytes: sample.volume_total_bytes.map(safe_int),
            volume_free_bytes: sample.volume_free_bytes.map(safe_int),
            disk_active_pct: sample.disk_active_pct.map(one_decimal),
            disk_read_bps: sample.disk_read_bps.map(safe_int),
            disk_write_bps: sample.disk_write_bps.map(safe_int),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LevelDto {
    Normal,
    High,
    VeryHigh,
    Unknown,
}

impl From<Level> for LevelDto {
    fn from(level: Level) -> Self {
        match level {
            Level::Normal => Self::Normal,
            Level::High => Self::High,
            Level::VeryHigh => Self::VeryHigh,
            Level::Unknown => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SpaceLevelDto {
    Normal,
    AlmostFull,
    Unknown,
}

impl From<SpaceLevel> for SpaceLevelDto {
    fn from(level: SpaceLevel) -> Self {
        match level {
            SpaceLevel::Normal => Self::Normal,
            SpaceLevel::AlmostFull => Self::AlmostFull,
            SpaceLevel::Unknown => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AvailabilityDto {
    Ok,
    WarmingUp,
    AfterGap,
    SourceError,
    NotSupported,
}

impl From<Availability> for AvailabilityDto {
    fn from(value: Availability) -> Self {
        match value {
            Availability::Ok => Self::Ok,
            Availability::WarmingUp => Self::WarmingUp,
            Availability::AfterGap => Self::AfterGap,
            Availability::SourceError => Self::SourceError,
            Availability::NotSupported => Self::NotSupported,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricSummaryDto {
    pub current: Option<f64>,
    pub avg60: Option<f64>,
    pub peak: Option<f64>,
    pub level: LevelDto,
}

impl From<&MetricSummary> for MetricSummaryDto {
    fn from(summary: &MetricSummary) -> Self {
        Self {
            current: summary.current.map(one_decimal),
            avg60: summary.avg60.map(one_decimal),
            peak: summary.peak.map(one_decimal),
            level: summary.level.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeSummaryDto {
    pub used_pct: Option<f64>,
    pub level: SpaceLevelDto,
}

impl From<&VolumeSummary> for VolumeSummaryDto {
    fn from(summary: &VolumeSummary) -> Self {
        Self {
            used_pct: summary.used_pct.map(one_decimal),
            level: summary.level.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailabilityMapDto {
    pub cpu: AvailabilityDto,
    pub memory: AvailabilityDto,
    pub volume: AvailabilityDto,
    pub disk_active: AvailabilityDto,
    pub disk_io: AvailabilityDto,
}

impl From<&SampleAvailability> for AvailabilityMapDto {
    fn from(value: &SampleAvailability) -> Self {
        Self {
            cpu: value.cpu.into(),
            memory: value.memory.into(),
            volume: value.volume.into(),
            disk_active: value.disk_active.into(),
            disk_io: value.disk_io.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorSummaryDto {
    pub cpu: MetricSummaryDto,
    pub memory: MetricSummaryDto,
    pub disk_active: MetricSummaryDto,
    pub volume: VolumeSummaryDto,
    pub availability: AvailabilityMapDto,
    pub gaps_in_window: u32,
    pub avg_read_micros: Option<u64>,
}

impl From<&MonitorSummary> for MonitorSummaryDto {
    fn from(summary: &MonitorSummary) -> Self {
        Self {
            cpu: (&summary.cpu).into(),
            memory: (&summary.memory).into(),
            disk_active: (&summary.disk_active).into(),
            volume: (&summary.volume).into(),
            availability: (&summary.availability).into(),
            gaps_in_window: summary.gaps_in_window,
            avg_read_micros: summary.avg_read_micros.map(safe_int),
        }
    }
}

/// Evento del flujo: una muestra con su resumen o el aviso de detencion.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MonitorEventDto {
    Sample {
        sample: MonitorSampleDto,
        summary: Box<MonitorSummaryDto>,
    },
    Stopped {
        reason: &'static str,
    },
}

/// Respuesta de `start_monitor_stream`: la ventana actual y el ultimo resumen.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorSnapshotDto {
    pub subscription_id: u64,
    pub interval_ms: u64,
    pub window_ms: u64,
    pub samples: Vec<MonitorSampleDto>,
    pub summary: Option<MonitorSummaryDto>,
}

// ---------------------------------------------------------------------------------------------
// Hub
// ---------------------------------------------------------------------------------------------

/// Envio al suscriptor: `Err(())` si el canal esta roto (el hub descarta al suscriptor).
pub type SendFn = Box<dyn FnMut(&MonitorEventDto) -> Result<(), ()> + Send>;

struct Subscriber {
    id: u64,
    send: SendFn,
}

struct HubState {
    window: SampleWindow,
    subscriber: Option<Subscriber>,
    next_id: u64,
    failed: bool,
}

/// Estado compartido del monitor: ventana, suscriptor unico y estado del muestreo.
#[derive(Clone)]
pub struct MonitorHub(Arc<Mutex<HubState>>);

impl Default for MonitorHub {
    fn default() -> Self {
        Self::new()
    }
}

fn lock(mutex: &Mutex<HubState>) -> MutexGuard<'_, HubState> {
    // Un panico de otro hilo no invalida estos datos: se sigue con su contenido.
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl MonitorHub {
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(HubState {
            window: SampleWindow::new(),
            subscriber: None,
            next_id: 1,
            failed: false,
        })))
    }

    /// Agrega el ciclo a la ventana y lo envia al suscriptor (si hay). Copiar la ventana en
    /// `subscribe` y agregar aca ocurren bajo el mismo candado: sin duplicados ni perdidas.
    pub fn push(&self, cycle: &CycleResult) {
        let mut state = lock(&self.0);
        if state.failed {
            return;
        }
        state
            .window
            .push(cycle.sample.clone(), cycle.window_cleared);
        let event = MonitorEventDto::Sample {
            sample: (&cycle.sample).into(),
            summary: Box::new((&cycle.summary).into()),
        };
        let delivered = match state.subscriber.as_mut() {
            Some(subscriber) => (subscriber.send)(&event).is_ok(),
            None => true,
        };
        if !delivered {
            state.subscriber = None;
        }
    }

    /// Registra el unico suscriptor (reemplaza al anterior) y responde con la ventana actual.
    pub fn subscribe(&self, send: SendFn) -> Result<MonitorSnapshotDto, AppError> {
        let mut state = lock(&self.0);
        if state.failed {
            return Err(AppError::MonitorUnavailable);
        }
        let id = state.next_id;
        state.next_id += 1;
        state.subscriber = Some(Subscriber { id, send });
        Ok(MonitorSnapshotDto {
            subscription_id: id,
            interval_ms: INTERVAL_MS,
            window_ms: u64::try_from(WINDOW_MAX_AGE_MS).unwrap_or(0),
            samples: state.window.iter().map(MonitorSampleDto::from).collect(),
            summary: state.window.summary().as_ref().map(MonitorSummaryDto::from),
        })
    }

    /// Cancela la suscripcion `id`; un id viejo o desconocido no hace nada (idempotente).
    pub fn unsubscribe(&self, id: u64) {
        let mut state = lock(&self.0);
        if state.subscriber.as_ref().is_some_and(|s| s.id == id) {
            state.subscriber = None;
        }
    }

    /// El muestreo fallo: avisa una vez al suscriptor, lo libera y rechaza nuevas suscripciones.
    pub fn mark_failed(&self) {
        let mut state = lock(&self.0);
        if state.failed {
            return;
        }
        state.failed = true;
        if let Some(mut subscriber) = state.subscriber.take() {
            let _ = (subscriber.send)(&MonitorEventDto::Stopped {
                reason: "sampler_failed",
            });
        }
    }

    pub fn is_failed(&self) -> bool {
        lock(&self.0).failed
    }

    pub fn window_len(&self) -> usize {
        lock(&self.0).window.len()
    }
}

// ---------------------------------------------------------------------------------------------
// Hilo de muestreo
// ---------------------------------------------------------------------------------------------

/// Espera real del muestreador: bloquea con `recv_timeout` sobre un canal de parada. Una senal o
/// el emisor soltado (fin del proceso) terminan el bucle; no hace falta `join` al salir.
pub struct RecvTicker<M> {
    stop: Receiver<()>,
    mono: M,
}

impl<M: MonotonicClock> RecvTicker<M> {
    pub fn new(stop: Receiver<()>, mono: M) -> Self {
        Self { stop, mono }
    }
}

impl<M: MonotonicClock> Ticker for RecvTicker<M> {
    fn wait_until(&mut self, deadline_ms: u64) -> bool {
        let remaining = deadline_ms.saturating_sub(self.mono.now_ms());
        match self.stop.recv_timeout(Duration::from_millis(remaining)) {
            Err(RecvTimeoutError::Timeout) => true,
            Ok(()) | Err(RecvTimeoutError::Disconnected) => false,
        }
    }
}

/// Corre el bucle de muestreo entregando cada ciclo al hub. Un panico dentro de un ciclo deja al
/// hub en `failed` (avisa al suscriptor); no hay reinicio automatico (D-11).
pub fn run_guarded<S, C, M, T>(hub: &MonitorHub, sampler: &mut Sampler<S, C, M>, ticker: &mut T)
where
    S: Sensors,
    C: Clock,
    M: MonotonicClock,
    T: Ticker,
{
    let outcome = panic::catch_unwind(AssertUnwindSafe(|| {
        run_sampler(sampler, ticker, |cycle| hub.push(cycle));
    }));
    if outcome.is_err() {
        hub.mark_failed();
    }
}

/// Crea el hilo de muestreo; `build` se ejecuta DENTRO del hilo (los handles de PDH no son
/// `Send`). `build` devuelve `None` si no hay sensores: el hub queda `failed`. Si no se puede
/// crear el hilo, el hub tambien queda `failed` y se devuelve el error de E/S.
pub fn spawn_sampler_with<S, C, M, T, B>(
    hub: MonitorHub,
    build: B,
) -> std::io::Result<JoinHandle<()>>
where
    S: Sensors + 'static,
    C: Clock + 'static,
    M: MonotonicClock + 'static,
    T: Ticker + 'static,
    B: FnOnce() -> Option<(Sampler<S, C, M>, T)> + Send + 'static,
{
    let worker_hub = hub.clone();
    let spawned = thread::Builder::new()
        .name("monitor-sampler".into())
        .spawn(move || {
            let Some((mut sampler, mut ticker)) = build() else {
                worker_hub.mark_failed();
                return;
            };
            run_guarded(&worker_hub, &mut sampler, &mut ticker);
        });
    if spawned.is_err() {
        hub.mark_failed();
    }
    spawned
}

/// Arranca el muestreo real con `sensors` (construido dentro del hilo por `make_sensors`) y el
/// canal de parada; `None` de la fabrica = sensores no disponibles (hub `failed`).
pub fn spawn_sampler<S, F>(
    hub: MonitorHub,
    make_sensors: F,
    stop: Receiver<()>,
) -> std::io::Result<JoinHandle<()>>
where
    S: Sensors + 'static,
    F: FnOnce() -> Option<S> + Send + 'static,
{
    spawn_sampler_with(hub, move || {
        let mono = SystemMonotonicClock::new();
        let sensors = make_sensors()?;
        Some((
            Sampler::new(sensors, SystemClock, mono, INTERVAL_MS),
            RecvTicker::new(stop, mono),
        ))
    })
}

// ---------------------------------------------------------------------------------------------
// Comandos IPC
// ---------------------------------------------------------------------------------------------

/// Suscribe `send` como unico suscriptor y responde con la ventana actual y el resumen. No toma
/// `ActiveJob` ni usa la base (D-03, D-16).
pub fn start_monitor_stream_run(
    hub: &MonitorHub,
    send: SendFn,
) -> Result<MonitorSnapshotDto, AppError> {
    hub.subscribe(send)
}

/// Valida un identificador de suscripcion que llega de JavaScript como numero.
fn parse_subscription_id(raw: f64) -> Result<u64, AppError> {
    let valid =
        raw.is_finite() && raw >= 0.0 && raw.fract() == 0.0 && raw <= MAX_SAFE_INTEGER as f64;
    if !valid {
        return Err(AppError::InvalidArgument("subscriptionId".into()));
    }
    // Conversion exacta: es un entero no negativo dentro del rango seguro de JavaScript.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(raw as u64)
}

/// Cancela la suscripcion; idempotente (un id viejo o desconocido no afecta a la vigente).
pub fn stop_monitor_stream_run(hub: &MonitorHub, subscription_id: f64) -> Result<(), AppError> {
    hub.unsubscribe(parse_subscription_id(subscription_id)?);
    Ok(())
}

/// Comando IPC: se suscribe al flujo del monitor con el canal de la invocacion.
#[tauri::command]
pub async fn start_monitor_stream(
    on_sample: Channel<MonitorEventDto>,
    hub: State<'_, MonitorHub>,
) -> Result<MonitorSnapshotDto, AppError> {
    // Un error de envio descarta al suscriptor: la UI pudo recargarse o cerrarse.
    let send: SendFn = Box::new(move |event| on_sample.send(event.clone()).map_err(|_| ()));
    start_monitor_stream_run(hub.inner(), send)
}

/// Comando IPC: cancela la suscripcion `subscription_id` (idempotente).
#[tauri::command]
pub async fn stop_monitor_stream(
    subscription_id: f64,
    hub: State<'_, MonitorHub>,
) -> Result<(), AppError> {
    stop_monitor_stream_run(hub.inner(), subscription_id)
}

#[cfg(test)]
#[path = "monitor_tests.rs"]
mod tests;

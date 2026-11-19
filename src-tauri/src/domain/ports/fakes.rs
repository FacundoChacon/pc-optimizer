//! Fakes en memoria de los puertos (solo tests).

use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::domain::junk::category::JunkCategory;
use crate::domain::junk::model::{
    CategoryResult, CategoryStatus, JunkFinding, JunkFindingPage, JunkScanResults, JunkScanSummary,
    StoredFinding,
};
use crate::domain::model::{
    CleanupLogEntry, DEFAULT_SETTINGS, EpochMillis, NewCleanupLogEntry, NewScan, SCAN_RETENTION,
    Scan, ScanId, ScanKind, ScanStatus, scans_to_prune,
};
use crate::domain::ports::recycle_bin_empty::EmptyError;
use crate::domain::ports::{
    AncestorPin, BinAvailability, CleanupLogStore, Clock, DirEntry, EntryKind, FileIdentity,
    FileMeta, FileSystem, JunkFindingStore, PortError, RecycleBinEmptier, RecycleBinInfo,
    RecycleBinPolicy, RecycleBinUsage, ScanStore, SensorSample, Sensors, SessionIdentity,
    SettingsStore, StoreError, Trash, TrashError,
};
use crate::domain::privileges::UserSid;
use crate::domain::protection::service::DeleteTarget;
use crate::domain::recycle_bin::root::VolumeRoot;

/// Operacion del puerto de archivos (para registrar llamadas e inyectar fallas por operacion).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum FsOp {
    Metadata,
    Canonicalize,
    ReadDir,
    Pin,
}

/// Volumen de las identidades automaticas del fake.
const FAKE_VOLUME_SERIAL: u64 = 1;

#[derive(Default)]
struct FsState {
    entries: BTreeMap<PathBuf, FileMeta>,
    canonical: BTreeMap<PathBuf, PathBuf>,
    canonical_as_is: bool,
    failures: BTreeMap<PathBuf, PortError>,
    op_failures: BTreeMap<(FsOp, PathBuf), PortError>,
    calls: Vec<(FsOp, PathBuf)>,
    pinned: Vec<PathBuf>,
    /// Tamano y fecha que `read_dir` informa en lugar de los reales (listado desactualizado).
    stale_listing: BTreeMap<PathBuf, (u64, Option<EpochMillis>)>,
    next_identity: u128,
}

impl FsState {
    fn auto_meta(
        &mut self,
        kind: EntryKind,
        size_bytes: u64,
        modified: Option<EpochMillis>,
    ) -> FileMeta {
        self.next_identity += 1;
        FileMeta {
            kind,
            size_bytes,
            modified,
            cloud_placeholder: false,
            identity: Some(FileIdentity {
                volume_serial: FAKE_VOLUME_SERIAL,
                file_id: self.next_identity,
            }),
            hard_links: Some(1),
        }
    }

    /// Registra la llamada y devuelve la falla inyectada para esa ruta u operacion, si hay.
    fn begin(&mut self, op: FsOp, path: &Path) -> Result<(), PortError> {
        self.calls.push((op, path.to_path_buf()));
        if let Some(error) = self.failures.get(path) {
            return Err(error.clone());
        }
        if let Some(error) = self.op_failures.get(&(op, path.to_path_buf())) {
            return Err(error.clone());
        }
        Ok(())
    }
}

/// Sistema de archivos falso configurable.
///
/// Toda entrada nace con una identidad unica y `hard_links = Some(1)`; `read_dir` las omite.
#[derive(Default)]
pub(crate) struct FakeFileSystem {
    state: Arc<Mutex<FsState>>,
}

/// Fijacion falsa: libera sus rutas al soltarse.
struct FakePin {
    state: Arc<Mutex<FsState>>,
    paths: Vec<PathBuf>,
}

impl AncestorPin for FakePin {}

impl Drop for FakePin {
    fn drop(&mut self) {
        if let Ok(mut state) = self.state.lock() {
            for path in &self.paths {
                if let Some(index) = state.pinned.iter().position(|p| p == path) {
                    state.pinned.remove(index);
                }
            }
        }
    }
}

impl FakeFileSystem {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    fn with_state(self, f: impl FnOnce(&mut FsState)) -> Self {
        self.mutate(f);
        self
    }

    fn mutate(&self, f: impl FnOnce(&mut FsState)) {
        if let Ok(mut state) = self.state.lock() {
            f(&mut state);
        }
    }

    pub(crate) fn with_dir(self, path: &str) -> Self {
        self.with_state(|s| {
            let meta = s.auto_meta(EntryKind::Directory, 0, None);
            s.entries.insert(PathBuf::from(path), meta);
        })
    }

    pub(crate) fn with_file(self, path: &str, size: u64, modified: Option<EpochMillis>) -> Self {
        self.with_state(|s| {
            let meta = s.auto_meta(EntryKind::File, size, modified);
            s.entries.insert(PathBuf::from(path), meta);
        })
    }

    pub(crate) fn with_reparse_point(self, path: &str) -> Self {
        self.with_state(|s| {
            let meta = s.auto_meta(EntryKind::ReparsePoint, 0, None);
            s.entries.insert(PathBuf::from(path), meta);
        })
    }

    pub(crate) fn with_cloud_placeholder(self, path: &str, size: u64) -> Self {
        self.with_state(|s| {
            let mut meta = s.auto_meta(EntryKind::File, size, None);
            meta.cloud_placeholder = true;
            s.entries.insert(PathBuf::from(path), meta);
        })
    }

    /// Un reparse point de nube (tag de proveedor): `ReparsePoint` + `cloud_placeholder`.
    pub(crate) fn with_cloud_reparse_point(self, path: &str) -> Self {
        self.with_state(|s| {
            let mut meta = s.auto_meta(EntryKind::ReparsePoint, 0, None);
            meta.cloud_placeholder = true;
            s.entries.insert(PathBuf::from(path), meta);
        })
    }

    /// Fija la identidad de una entrada existente.
    pub(crate) fn with_identity(self, path: &str, identity: FileIdentity) -> Self {
        self.with_state(|s| {
            if let Some(meta) = s.entries.get_mut(Path::new(path)) {
                meta.identity = Some(identity);
            }
        })
    }

    /// Quita la identidad de una entrada (simula un FS que no la informa).
    pub(crate) fn without_identity(self, path: &str) -> Self {
        self.with_state(|s| {
            if let Some(meta) = s.entries.get_mut(Path::new(path)) {
                meta.identity = None;
            }
        })
    }

    /// Fija la cantidad de hard links de una entrada existente.
    pub(crate) fn with_hard_links(self, path: &str, links: u32) -> Self {
        self.with_state(|s| {
            if let Some(meta) = s.entries.get_mut(Path::new(path)) {
                meta.hard_links = Some(links);
            }
        })
    }

    /// Quita la cantidad de hard links de una entrada (simula un FS que no la informa).
    pub(crate) fn without_hard_links(self, path: &str) -> Self {
        self.with_state(|s| {
            if let Some(meta) = s.entries.get_mut(Path::new(path)) {
                meta.hard_links = None;
            }
        })
    }

    /// Crea `path` como segundo nombre de `existing`: misma identidad, 2 hard links en ambos.
    pub(crate) fn with_hard_link(self, path: &str, existing: &str) -> Self {
        self.with_state(|s| {
            if let Some(original) = s.entries.get_mut(Path::new(existing)) {
                original.hard_links = Some(2);
                let copy = original.clone();
                s.entries.insert(PathBuf::from(path), copy);
            }
        })
    }

    /// `canonicalize` devuelve la misma ruta si la entrada existe (el mapeo explicito gana).
    pub(crate) fn canonical_as_is(self) -> Self {
        self.with_state(|s| s.canonical_as_is = true)
    }

    /// Carrera: la entrada pasa a ser un reparse point.
    pub(crate) fn replace_with_reparse_point(&self, path: &Path) {
        self.mutate(|s| {
            if let Some(meta) = s.entries.get_mut(path) {
                meta.kind = EntryKind::ReparsePoint;
            }
        });
    }

    /// Carrera: el archivo se borra y se crea otro con el mismo nombre (otra identidad).
    pub(crate) fn recreate_file(&self, path: &Path, new_identity: FileIdentity) {
        self.mutate(|s| {
            if let Some(meta) = s.entries.get_mut(path) {
                meta.kind = EntryKind::File;
                meta.identity = Some(new_identity);
            }
        });
    }

    /// Carrera: la entrada pasa a ser un directorio (otra identidad).
    pub(crate) fn replace_with_directory(&self, path: &Path, new_identity: FileIdentity) {
        self.mutate(|s| {
            if let Some(meta) = s.entries.get_mut(path) {
                meta.kind = EntryKind::Directory;
                meta.identity = Some(new_identity);
            }
        });
    }

    /// Rutas fijadas ahora, en orden de fijacion.
    pub(crate) fn pinned(&self) -> Vec<PathBuf> {
        self.state
            .lock()
            .map(|s| s.pinned.clone())
            .unwrap_or_default()
    }

    /// Llamadas registradas al puerto, en orden.
    pub(crate) fn calls(&self) -> Vec<(FsOp, PathBuf)> {
        self.state
            .lock()
            .map(|s| s.calls.clone())
            .unwrap_or_default()
    }

    /// Cuantas veces se llamo a `op` con `path`.
    pub(crate) fn call_count(&self, op: FsOp, path: &str) -> usize {
        self.calls()
            .iter()
            .filter(|(o, p)| *o == op && p == Path::new(path))
            .count()
    }

    /// Cantidad total de llamadas al puerto.
    pub(crate) fn total_calls(&self) -> usize {
        self.calls().len()
    }

    /// Falla solo la operacion `op` sobre `path`.
    pub(crate) fn fail_on_op(self, path: &str, op: FsOp, error: PortError) -> Self {
        self.with_state(|s| {
            s.op_failures.insert((op, PathBuf::from(path)), error);
        })
    }

    pub(crate) fn with_canonical(self, path: &str, canonical: &str) -> Self {
        self.with_state(|s| {
            s.canonical
                .insert(PathBuf::from(path), PathBuf::from(canonical));
        })
    }

    pub(crate) fn fail_on(self, path: &str, error: PortError) -> Self {
        self.with_state(|s| {
            s.failures.insert(PathBuf::from(path), error);
        })
    }

    /// Indica si la ruta existe en el fake.
    pub(crate) fn exists(&self, path: &Path) -> bool {
        self.state
            .lock()
            .map(|s| s.entries.contains_key(path))
            .unwrap_or(false)
    }

    /// `read_dir` informa `size_bytes` y `modified` desactualizados para `path`, como NTFS con un
    /// archivo abierto para escritura; `metadata` sigue informando los reales.
    pub(crate) fn with_stale_listing(
        self,
        path: &str,
        size_bytes: u64,
        modified: Option<EpochMillis>,
    ) -> Self {
        self.with_state(|s| {
            s.stale_listing
                .insert(PathBuf::from(path), (size_bytes, modified));
        })
    }

    /// Quita la ruta (la usa `FakeTrash`).
    pub(crate) fn remove(&self, path: &Path) {
        if let Ok(mut s) = self.state.lock() {
            s.entries.remove(path);
        }
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, FsState>, PortError> {
        self.state
            .lock()
            .map_err(|_| PortError::Io("fake fs lock poisoned".into()))
    }
}

impl FileSystem for FakeFileSystem {
    fn canonicalize(&self, path: &Path) -> Result<PathBuf, PortError> {
        let mut state = self.lock()?;
        state.begin(FsOp::Canonicalize, path)?;
        if let Some(mapped) = state.canonical.get(path) {
            return Ok(mapped.clone());
        }
        if state.canonical_as_is && state.entries.contains_key(path) {
            return Ok(path.to_path_buf());
        }
        Err(PortError::NotFound(path.display().to_string()))
    }

    fn metadata(&self, path: &Path) -> Result<FileMeta, PortError> {
        let mut state = self.lock()?;
        state.begin(FsOp::Metadata, path)?;
        state
            .entries
            .get(path)
            .cloned()
            .ok_or_else(|| PortError::NotFound(path.display().to_string()))
    }

    fn pin_ancestors(&self, path: &Path) -> Result<Box<dyn AncestorPin>, PortError> {
        let mut state = self.lock()?;
        state.begin(FsOp::Pin, path)?;
        let mut ancestors: Vec<PathBuf> = path
            .ancestors()
            .skip(1)
            .filter(|a| a.parent().is_some())
            .map(Path::to_path_buf)
            .collect();
        ancestors.reverse();
        for ancestor in &ancestors {
            if let Some(error) = state.failures.get(ancestor) {
                return Err(error.clone());
            }
            let meta = state
                .entries
                .get(ancestor)
                .ok_or_else(|| PortError::NotFound(ancestor.display().to_string()))?;
            match meta.kind {
                EntryKind::Directory => {}
                EntryKind::ReparsePoint => {
                    return Err(PortError::Io(format!(
                        "reparse point: {}",
                        ancestor.display()
                    )));
                }
                EntryKind::File => {
                    return Err(PortError::Io(format!(
                        "not a directory: {}",
                        ancestor.display()
                    )));
                }
            }
        }
        state.pinned.extend(ancestors.iter().cloned());
        Ok(Box::new(FakePin {
            state: Arc::clone(&self.state),
            paths: ancestors,
        }))
    }

    fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>, PortError> {
        let mut state = self.lock()?;
        state.begin(FsOp::ReadDir, path)?;
        let own = state
            .entries
            .get(path)
            .ok_or_else(|| PortError::NotFound(path.display().to_string()))?;
        match own.kind {
            EntryKind::File => Err(PortError::Io(format!(
                "not a directory: {}",
                path.display()
            ))),
            // Un reparse point nunca expone hijos.
            EntryKind::ReparsePoint => Ok(Vec::new()),
            EntryKind::Directory => Ok(state
                .entries
                .iter()
                .filter(|(child, _)| child.parent() == Some(path))
                .map(|(child, meta)| {
                    let mut listed = FileMeta {
                        identity: None,
                        hard_links: None,
                        ..meta.clone()
                    };
                    if let Some((size_bytes, modified)) = state.stale_listing.get(child) {
                        listed.size_bytes = *size_bytes;
                        listed.modified = *modified;
                    }
                    DirEntry {
                        path: child.clone(),
                        meta: listed,
                    }
                })
                .collect()),
        }
    }
}

/// Accion que corre justo antes de mover una ruta (simula un reemplazo en la ventana residual).
type BeforeMove = Box<dyn Fn()>;

/// Papelera falsa: registra lo movido y lo quita del sistema de archivos compartido.
///
/// Mueve lo que haya en la ruta del objetivo en el momento de mover, igual que el shell: si un
/// enlace reemplazo al archivo, se mueve el enlace y su destino sigue existiendo.
pub(crate) struct FakeTrash {
    fs: Arc<FakeFileSystem>,
    failures: Mutex<BTreeMap<PathBuf, TrashError>>,
    hooks: Mutex<BTreeMap<PathBuf, BeforeMove>>,
    no_ops: Mutex<std::collections::BTreeSet<PathBuf>>,
    moved: Mutex<Vec<PathBuf>>,
    preflight_error: Mutex<Option<TrashError>>,
    preflight_calls: Mutex<Vec<PathBuf>>,
}

impl FakeTrash {
    /// La comprobacion previa falla con `error` (Papelera no disponible).
    pub(crate) fn preflight_fails(self, error: TrashError) -> Self {
        if let Ok(mut e) = self.preflight_error.lock() {
            *e = Some(error);
        }
        self
    }

    pub(crate) fn preflight_calls(&self) -> Vec<PathBuf> {
        self.preflight_calls
            .lock()
            .map(|c| c.clone())
            .unwrap_or_default()
    }

    pub(crate) fn new(fs: Arc<FakeFileSystem>) -> Self {
        Self {
            fs,
            failures: Mutex::new(BTreeMap::new()),
            hooks: Mutex::new(BTreeMap::new()),
            no_ops: Mutex::new(std::collections::BTreeSet::new()),
            moved: Mutex::new(Vec::new()),
            preflight_error: Mutex::new(None),
            preflight_calls: Mutex::new(Vec::new()),
        }
    }

    /// Mover `path` falla con `error` y no lo mueve.
    pub(crate) fn fail_on(self, path: &str, error: TrashError) -> Self {
        if let Ok(mut f) = self.failures.lock() {
            f.insert(PathBuf::from(path), error);
        }
        self
    }

    /// Corre `hook` justo antes de mover `path` (el objetivo sigue fijado).
    pub(crate) fn before_move(self, path: &str, hook: impl Fn() + 'static) -> Self {
        if let Ok(mut h) = self.hooks.lock() {
            h.insert(PathBuf::from(path), Box::new(hook));
        }
        self
    }

    /// Mover `path` responde exito pero no lo mueve (Papelera mentirosa).
    pub(crate) fn succeed_without_moving(self, path: &str) -> Self {
        if let Ok(mut n) = self.no_ops.lock() {
            n.insert(PathBuf::from(path));
        }
        self
    }

    pub(crate) fn moved(&self) -> Vec<PathBuf> {
        self.moved.lock().map(|m| m.clone()).unwrap_or_default()
    }
}

fn poisoned() -> TrashError {
    TrashError::Other("fake_trash_lock_poisoned".into())
}

impl Trash for FakeTrash {
    fn preflight(&self, sample: &Path) -> Result<(), TrashError> {
        self.preflight_calls
            .lock()
            .map_err(|_| poisoned())?
            .push(sample.to_path_buf());
        match self.preflight_error.lock().map_err(|_| poisoned())?.clone() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    fn move_to_trash(&self, target: &DeleteTarget) -> Result<(), TrashError> {
        let path = target.path();
        if let Some(hook) = self.hooks.lock().map_err(|_| poisoned())?.get(path) {
            hook();
        }
        let failure = self
            .failures
            .lock()
            .map_err(|_| poisoned())?
            .get(path)
            .cloned();
        if let Some(error) = failure {
            return Err(error);
        }
        if !self.fs.exists(path) {
            return Err(TrashError::NotFound);
        }
        if self.no_ops.lock().map_err(|_| poisoned())?.contains(path) {
            return Ok(());
        }
        self.fs.remove(path);
        self.moved
            .lock()
            .map_err(|_| poisoned())?
            .push(path.to_path_buf());
        Ok(())
    }
}

/// Consulta de la Papelera falsa: devuelve un valor o un error configurado y registra las raices
/// consultadas.
pub(crate) struct FakeRecycleBin {
    /// Estado actual de la Papelera: lo comparte con su `FakeRecycleBinEmptier` (C-10).
    result: Arc<Mutex<Result<RecycleBinUsage, PortError>>>,
    queried: Mutex<Vec<PathBuf>>,
}

impl FakeRecycleBin {
    pub(crate) fn with_usage(items: u64, bytes: u64) -> Self {
        Self {
            result: Arc::new(Mutex::new(Ok(RecycleBinUsage { items, bytes }))),
            queried: Mutex::new(Vec::new()),
        }
    }

    pub(crate) fn failing(error: PortError) -> Self {
        Self {
            result: Arc::new(Mutex::new(Err(error))),
            queried: Mutex::new(Vec::new()),
        }
    }

    /// Cambia lo que mide la Papelera (otra app mando algo, el usuario restauro algo...).
    pub(crate) fn set_usage(&self, items: u64, bytes: u64) {
        if let Ok(mut state) = self.result.lock() {
            *state = Ok(RecycleBinUsage { items, bytes });
        }
    }

    /// A partir de ahora la medicion falla con `error`.
    pub(crate) fn set_failing(&self, error: PortError) {
        if let Ok(mut state) = self.result.lock() {
            *state = Err(error);
        }
    }

    /// Vaciador falso que actua sobre esta misma Papelera: tras `empty` la medicion cambia segun
    /// `after`. Nunca toca la Papelera real.
    pub(crate) fn emptier(&self, after: AfterEmpty) -> FakeRecycleBinEmptier {
        FakeRecycleBinEmptier {
            bin: Arc::clone(&self.result),
            after,
            result: Ok(()),
            calls: Mutex::new(Vec::new()),
        }
    }

    /// Raices consultadas, en orden.
    pub(crate) fn queried(&self) -> Vec<PathBuf> {
        self.queried.lock().map(|q| q.clone()).unwrap_or_default()
    }
}

impl RecycleBinInfo for FakeRecycleBin {
    fn query(&self, volume_root: &Path) -> Result<RecycleBinUsage, PortError> {
        if let Ok(mut queried) = self.queried.lock() {
            queried.push(volume_root.to_path_buf());
        }
        self.result
            .lock()
            .map_err(|_| PortError::Io("lock poisoned".to_owned()))?
            .clone()
    }
}

/// Que mide la Papelera despues de que el vaciador falso corre.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AfterEmpty {
    /// Queda en 0 elementos.
    Empty,
    /// Quedan `items` elementos y `bytes` bytes (vaciado parcial).
    Leaves { items: u64, bytes: u64 },
    /// No cambia nada.
    Unchanged,
    /// La medicion posterior falla.
    MeasureFails(PortError),
}

/// Vaciador falso: registra las raices recibidas y deja la Papelera falsa en el estado pedido.
pub(crate) struct FakeRecycleBinEmptier {
    bin: Arc<Mutex<Result<RecycleBinUsage, PortError>>>,
    after: AfterEmpty,
    result: Result<(), EmptyError>,
    calls: Mutex<Vec<PathBuf>>,
}

impl FakeRecycleBinEmptier {
    /// La llamada devuelve `error` (el estado posterior sigue segun `AfterEmpty`).
    pub(crate) fn failing_with(mut self, error: EmptyError) -> Self {
        self.result = Err(error);
        self
    }

    /// Raices recibidas, en orden.
    pub(crate) fn calls(&self) -> Vec<PathBuf> {
        self.calls.lock().map(|c| c.clone()).unwrap_or_default()
    }
}

impl RecycleBinEmptier for FakeRecycleBinEmptier {
    fn empty(&self, root: &VolumeRoot) -> Result<(), EmptyError> {
        if let Ok(mut calls) = self.calls.lock() {
            calls.push(root.as_path());
        }
        if let Ok(mut state) = self.bin.lock() {
            match &self.after {
                AfterEmpty::Empty => *state = Ok(RecycleBinUsage { items: 0, bytes: 0 }),
                AfterEmpty::Leaves { items, bytes } => {
                    *state = Ok(RecycleBinUsage {
                        items: *items,
                        bytes: *bytes,
                    });
                }
                AfterEmpty::Unchanged => {}
                AfterEmpty::MeasureFails(error) => *state = Err(error.clone()),
            }
        }
        self.result.clone()
    }
}

/// Sensores guionados.
pub(crate) struct FakeSensors {
    queue: VecDeque<Result<SensorSample, PortError>>,
}

impl FakeSensors {
    pub(crate) fn new(script: Vec<Result<SensorSample, PortError>>) -> Self {
        Self {
            queue: script.into(),
        }
    }
}

impl Sensors for FakeSensors {
    fn sample(&mut self) -> Result<SensorSample, PortError> {
        self.queue
            .pop_front()
            .unwrap_or_else(|| Err(PortError::Unsupported("no more scripted samples".into())))
    }
}

/// Reloj fijo.
pub(crate) struct FixedClock(pub(crate) EpochMillis);

impl Clock for FixedClock {
    fn now(&self) -> EpochMillis {
        self.0
    }
}

#[derive(Default)]
struct StoreInner {
    settings: BTreeMap<String, serde_json::Value>,
    scans: Vec<Scan>,
    next_scan_id: i64,
    log: Vec<CleanupLogEntry>,
    next_log_id: i64,
    /// `(id, escaneo, hallazgo)`.
    junk_findings: Vec<(i64, ScanId, JunkFinding)>,
    junk_results: Vec<(ScanId, CategoryResult)>,
    next_finding_id: i64,
}

/// Persistencia en memoria con el mismo comportamiento observable que SQLite.
pub(crate) struct InMemoryStore {
    inner: Mutex<StoreInner>,
}

impl InMemoryStore {
    pub(crate) fn new() -> Self {
        let settings = DEFAULT_SETTINGS
            .iter()
            .filter_map(|(key, raw)| {
                serde_json::from_str(raw)
                    .ok()
                    .map(|value| ((*key).to_string(), value))
            })
            .collect();
        Self {
            inner: Mutex::new(StoreInner {
                settings,
                next_scan_id: 1,
                next_log_id: 1,
                next_finding_id: 1,
                ..StoreInner::default()
            }),
        }
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, StoreInner>, StoreError> {
        self.inner
            .lock()
            .map_err(|_| StoreError::Unavailable("lock poisoned".into()))
    }
}

impl SettingsStore for InMemoryStore {
    fn get_setting(&self, key: &str) -> Result<Option<serde_json::Value>, StoreError> {
        Ok(self.lock()?.settings.get(key).cloned())
    }

    fn list_settings(&self) -> Result<Vec<(String, serde_json::Value)>, StoreError> {
        Ok(self
            .lock()?
            .settings
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect())
    }

    fn set_setting(&self, key: &str, value: &serde_json::Value) -> Result<(), StoreError> {
        if key.is_empty() {
            return Err(StoreError::Constraint("empty setting key".into()));
        }
        self.lock()?.settings.insert(key.to_string(), value.clone());
        Ok(())
    }
}

impl ScanStore for InMemoryStore {
    fn create_scan(&self, scan: &NewScan) -> Result<ScanId, StoreError> {
        let mut inner = self.lock()?;
        let id = ScanId(inner.next_scan_id);
        inner.next_scan_id += 1;
        inner.scans.push(Scan {
            id,
            kind: scan.kind,
            trigger: scan.trigger,
            status: ScanStatus::Running,
            started_at: scan.started_at,
            finished_at: None,
        });
        let pruned = scans_to_prune(&inner.scans, SCAN_RETENTION);
        inner.scans.retain(|s| !pruned.contains(&s.id));
        // Cascada como `ON DELETE CASCADE`: los hallazgos y resumenes caen con su escaneo.
        inner
            .junk_findings
            .retain(|(_, scan, _)| !pruned.contains(scan));
        inner
            .junk_results
            .retain(|(scan, _)| !pruned.contains(scan));
        Ok(id)
    }

    fn finish_scan(
        &self,
        id: ScanId,
        status: ScanStatus,
        finished_at: EpochMillis,
    ) -> Result<(), StoreError> {
        if !status.is_terminal() {
            return Err(StoreError::InvalidState(format!(
                "cannot finish a scan with status {}",
                status.as_str()
            )));
        }
        let mut inner = self.lock()?;
        let scan = inner
            .scans
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or_else(|| StoreError::NotFound(format!("scan {}", id.0)))?;
        if scan.status.is_terminal() {
            return Err(StoreError::InvalidState(format!(
                "scan {} already finished",
                id.0
            )));
        }
        scan.status = status;
        scan.finished_at = Some(finished_at);
        Ok(())
    }

    fn get_scan(&self, id: ScanId) -> Result<Option<Scan>, StoreError> {
        Ok(self.lock()?.scans.iter().find(|s| s.id == id).cloned())
    }

    fn list_scans(&self) -> Result<Vec<Scan>, StoreError> {
        let mut scans = self.lock()?.scans.clone();
        scans.sort_by_key(|s| std::cmp::Reverse((s.started_at, s.id)));
        Ok(scans)
    }
}

impl CleanupLogStore for InMemoryStore {
    fn append_cleanup_log(&self, entries: &[NewCleanupLogEntry]) -> Result<(), StoreError> {
        for entry in entries {
            if entry.category.is_empty() || entry.path.is_empty() {
                return Err(StoreError::Constraint("empty category or path".into()));
            }
            if i64::try_from(entry.size_bytes).is_err() {
                return Err(StoreError::Constraint("size_bytes exceeds i64::MAX".into()));
            }
        }
        let mut inner = self.lock()?;
        for entry in entries {
            let id = inner.next_log_id;
            inner.next_log_id += 1;
            inner.log.push(CleanupLogEntry {
                id,
                ts: entry.ts,
                category: entry.category.clone(),
                path: entry.path.clone(),
                size_bytes: entry.size_bytes,
                outcome: entry.outcome,
                scan_id: entry.scan_id,
                error_detail: entry.error_detail.clone(),
            });
        }
        Ok(())
    }

    fn recent_cleanup_log(&self, limit: u32) -> Result<Vec<CleanupLogEntry>, StoreError> {
        let mut log = self.lock()?.log.clone();
        log.sort_by_key(|e| std::cmp::Reverse((e.ts, e.id)));
        log.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
        Ok(log)
    }

    fn scan_cleaned_at(
        &self,
        scan_id: ScanId,
        since: EpochMillis,
    ) -> Result<Option<EpochMillis>, StoreError> {
        Ok(self
            .lock()?
            .log
            .iter()
            .filter(|entry| entry.scan_id == Some(scan_id) && entry.ts >= since)
            .map(|entry| entry.ts)
            .max())
    }
}

impl JunkFindingStore for InMemoryStore {
    fn complete_junk_scan(
        &self,
        scan_id: ScanId,
        results: &JunkScanResults,
        finished_at: EpochMillis,
    ) -> Result<(), StoreError> {
        let mut inner = self.lock()?;
        let scan = inner
            .scans
            .iter()
            .find(|s| s.id == scan_id)
            .ok_or_else(|| StoreError::NotFound(format!("scan {}", scan_id.0)))?;
        if scan.kind != ScanKind::Junk || scan.status != ScanStatus::Running {
            return Err(StoreError::InvalidState(format!(
                "scan {} is not a running junk scan",
                scan_id.0
            )));
        }
        // Se valida TODO antes de tocar el estado: el guardado es atomico.
        let mut paths = std::collections::BTreeSet::new();
        for finding in &results.findings {
            if finding.path.is_empty() || i64::try_from(finding.size_bytes).is_err() {
                return Err(StoreError::Constraint("invalid finding".into()));
            }
            if !paths.insert(finding.path.clone()) {
                return Err(StoreError::Constraint("duplicate finding path".into()));
            }
        }
        let mut categories = std::collections::BTreeSet::new();
        for result in &results.categories {
            let reasoned = matches!(
                result.status,
                CategoryStatus::Partial | CategoryStatus::Blocked
            );
            let counters = [
                result.item_count,
                result.size_bytes,
                result.skipped_count,
                result.inaccessible_count,
            ];
            if reasoned != result.block_reason.is_some()
                || counters.iter().any(|c| i64::try_from(*c).is_err())
                || !categories.insert(result.category.as_str())
            {
                return Err(StoreError::Constraint("invalid category result".into()));
            }
        }
        let other_junk: Vec<ScanId> = inner
            .scans
            .iter()
            .filter(|s| s.kind == ScanKind::Junk && s.id != scan_id)
            .map(|s| s.id)
            .collect();
        inner
            .junk_findings
            .retain(|(_, scan, _)| !other_junk.contains(scan));
        for finding in &results.findings {
            let id = inner.next_finding_id;
            inner.next_finding_id += 1;
            inner.junk_findings.push((id, scan_id, finding.clone()));
        }
        inner
            .junk_results
            .extend(results.categories.iter().map(|r| (scan_id, *r)));
        if let Some(scan) = inner.scans.iter_mut().find(|s| s.id == scan_id) {
            scan.status = ScanStatus::Done;
            scan.finished_at = Some(finished_at);
        }
        Ok(())
    }

    fn last_done_junk_scan(&self) -> Result<Option<JunkScanSummary>, StoreError> {
        let inner = self.lock()?;
        let latest = inner
            .scans
            .iter()
            .filter(|s| s.kind == ScanKind::Junk && s.status == ScanStatus::Done)
            .max_by_key(|s| (s.started_at, s.id));
        Ok(latest.map(|scan| {
            let mut categories: Vec<CategoryResult> = inner
                .junk_results
                .iter()
                .filter(|(id, _)| *id == scan.id)
                .map(|(_, result)| *result)
                .collect();
            categories.sort_by_key(|r| JunkCategory::ALL.iter().position(|c| *c == r.category));
            JunkScanSummary {
                scan: scan.clone(),
                categories,
            }
        }))
    }

    fn list_junk_findings(
        &self,
        scan_id: ScanId,
        category: JunkCategory,
        offset: u64,
        limit: u32,
    ) -> Result<JunkFindingPage, StoreError> {
        let inner = self.lock()?;
        let mut mine: Vec<(i64, &JunkFinding)> = inner
            .junk_findings
            .iter()
            .filter(|(_, scan, finding)| *scan == scan_id && finding.category == category)
            .map(|(id, _, finding)| (*id, finding))
            .collect();
        mine.sort_by_key(|(id, finding)| (std::cmp::Reverse(finding.size_bytes), *id));
        let total = mine.len() as u64;
        let items = mine
            .into_iter()
            .skip(usize::try_from(offset).unwrap_or(usize::MAX))
            .take(usize::try_from(limit).unwrap_or(usize::MAX))
            .map(|(id, finding)| StoredFinding {
                id,
                finding: finding.clone(),
            })
            .collect();
        Ok(JunkFindingPage { total, items })
    }

    fn cleanup_candidates(
        &self,
        scan_id: ScanId,
        categories: &[JunkCategory],
    ) -> Result<Vec<StoredFinding>, StoreError> {
        let inner = self.lock()?;
        let mut mine: Vec<StoredFinding> = inner
            .junk_findings
            .iter()
            .filter(|(_, scan, finding)| {
                *scan == scan_id && finding.is_eligible() && categories.contains(&finding.category)
            })
            .map(|(id, _, finding)| StoredFinding {
                id: *id,
                finding: finding.clone(),
            })
            .collect();
        mine.sort_by_key(|stored| {
            (
                JunkCategory::ALL
                    .iter()
                    .position(|known| *known == stored.finding.category),
                std::cmp::Reverse(stored.finding.size_bytes),
                stored.id,
            )
        });
        Ok(mine)
    }
}

/// SID de usuario de prueba `S-1-5-21-1-2-3-<rid>` (28 bytes).
pub(crate) fn user_sid(rid: u32) -> UserSid {
    let mut bytes = vec![1, 5, 0, 0, 0, 0, 0, 5];
    for part in [21, 1, 2, 3, rid] {
        bytes.extend_from_slice(&u32::to_le_bytes(part));
    }
    UserSid::from_bytes(bytes).expect("SID de prueba valido")
}

/// Identidad de sesion falsa: cada respuesta es un valor, ausencia (`Ok(None)`) o un error.
pub(crate) struct FakeSessionIdentity {
    pub(crate) process_user: Result<UserSid, PortError>,
    pub(crate) process_elevated: Result<bool, PortError>,
    pub(crate) shell_user: Result<Option<UserSid>, PortError>,
    pub(crate) logon_user: Result<Option<UserSid>, PortError>,
}

impl FakeSessionIdentity {
    /// Proceso del usuario `process` sin elevar y sin senales de sesion.
    pub(crate) fn new(process: UserSid) -> Self {
        Self {
            process_user: Ok(process),
            process_elevated: Ok(false),
            shell_user: Ok(None),
            logon_user: Ok(None),
        }
    }

    pub(crate) fn elevated(mut self, elevated: bool) -> Self {
        self.process_elevated = Ok(elevated);
        self
    }

    pub(crate) fn shell(mut self, user: Option<UserSid>) -> Self {
        self.shell_user = Ok(user);
        self
    }

    pub(crate) fn logon(mut self, user: Option<UserSid>) -> Self {
        self.logon_user = Ok(user);
        self
    }
}

impl SessionIdentity for FakeSessionIdentity {
    fn process_user(&self) -> Result<UserSid, PortError> {
        self.process_user.clone()
    }

    fn process_elevated(&self) -> Result<bool, PortError> {
        self.process_elevated.clone()
    }

    fn shell_user(&self) -> Result<Option<UserSid>, PortError> {
        self.shell_user.clone()
    }

    fn logon_user(&self) -> Result<Option<UserSid>, PortError> {
        self.logon_user.clone()
    }
}

/// Politica de la Papelera falsa: devuelve una disponibilidad o un error configurado y registra
/// las rutas consultadas.
pub(crate) struct FakeRecycleBinPolicy {
    result: Result<BinAvailability, PortError>,
    queried: Mutex<Vec<PathBuf>>,
}

impl FakeRecycleBinPolicy {
    pub(crate) fn with(availability: BinAvailability) -> Self {
        Self {
            result: Ok(availability),
            queried: Mutex::new(Vec::new()),
        }
    }

    pub(crate) fn failing(error: PortError) -> Self {
        Self {
            result: Err(error),
            queried: Mutex::new(Vec::new()),
        }
    }

    pub(crate) fn queried(&self) -> Vec<PathBuf> {
        self.queried.lock().map(|q| q.clone()).unwrap_or_default()
    }
}

impl RecycleBinPolicy for FakeRecycleBinPolicy {
    fn availability(&self, path: &Path) -> Result<BinAvailability, PortError> {
        if let Ok(mut q) = self.queried.lock() {
            q.push(path.to_path_buf());
        }
        self.result.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ports::store_contract as contract;
    use crate::domain::protection::decision::ProtectionDecision;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn ports_should_be_object_safe() {
        let fs = FakeFileSystem::new();
        let _: &dyn FileSystem = &fs;
        let _: Option<Box<dyn AncestorPin>> = None;
        let trash = FakeTrash::new(Arc::new(FakeFileSystem::new()));
        let _: &dyn Trash = &trash;
        let clock = FixedClock(EpochMillis(1));
        let _: &dyn Clock = &clock;
        let bin = FakeRecycleBin::with_usage(0, 0);
        let _: &dyn RecycleBinInfo = &bin;
        let mut sensors = FakeSensors::new(vec![]);
        let _: &mut dyn Sensors = &mut sensors;
    }

    // --- FakeRecycleBin ---
    #[test]
    fn recycle_bin_fake_should_return_the_configured_usage_and_record_each_root() {
        let bin = FakeRecycleBin::with_usage(12, 340_000_000);
        let usage = bin.query(Path::new(r"C:\")).unwrap();
        assert_eq!(
            usage,
            RecycleBinUsage {
                items: 12,
                bytes: 340_000_000
            }
        );
        assert_eq!(bin.query(Path::new(r"D:\")).unwrap().items, 12);
        assert_eq!(bin.queried(), vec![p(r"C:\"), p(r"D:\")]);
    }

    #[test]
    fn recycle_bin_fake_should_return_the_configured_error_and_still_record_the_root() {
        let bin = FakeRecycleBin::failing(PortError::PermissionDenied("papelera".into()));
        assert_eq!(
            bin.query(Path::new(r"C:\")),
            Err(PortError::PermissionDenied("papelera".into()))
        );
        assert_eq!(bin.queried(), vec![p(r"C:\")]);
    }

    // --- FakeRecycleBinEmptier (C-10) ---
    fn c_root() -> VolumeRoot {
        VolumeRoot::from_drive_letter('C').unwrap()
    }

    #[test]
    fn emptier_fake_should_leave_the_shared_bin_empty_and_record_the_root() {
        let bin = FakeRecycleBin::with_usage(12, 340_000_000);
        let emptier = bin.emptier(AfterEmpty::Empty);
        assert_eq!(emptier.calls(), Vec::<PathBuf>::new());
        assert_eq!(emptier.empty(&c_root()), Ok(()));
        assert_eq!(emptier.calls(), vec![p(r"C:\")]);
        assert_eq!(
            bin.query(Path::new(r"C:\")),
            Ok(RecycleBinUsage { items: 0, bytes: 0 })
        );
    }

    #[test]
    fn emptier_fake_should_leave_a_partial_state_when_asked() {
        let bin = FakeRecycleBin::with_usage(12, 340_000_000);
        let emptier = bin.emptier(AfterEmpty::Leaves {
            items: 2,
            bytes: 40_000_000,
        });
        emptier.empty(&c_root()).unwrap();
        assert_eq!(
            bin.query(Path::new(r"C:\")),
            Ok(RecycleBinUsage {
                items: 2,
                bytes: 40_000_000
            })
        );
    }

    #[test]
    fn emptier_fake_should_keep_the_measure_when_the_call_fails_and_state_is_unchanged() {
        let bin = FakeRecycleBin::with_usage(12, 340_000_000);
        let emptier = bin
            .emptier(AfterEmpty::Unchanged)
            .failing_with(EmptyError::PermissionDenied);
        assert_eq!(emptier.empty(&c_root()), Err(EmptyError::PermissionDenied));
        assert_eq!(bin.query(Path::new(r"C:\")).unwrap().items, 12);
        assert_eq!(emptier.calls().len(), 1);
    }

    #[test]
    fn emptier_fake_should_make_the_following_measure_fail_when_asked() {
        let bin = FakeRecycleBin::with_usage(3, 30);
        let emptier = bin.emptier(AfterEmpty::MeasureFails(PortError::Io("x".into())));
        emptier.empty(&c_root()).unwrap();
        assert_eq!(bin.query(Path::new(r"C:\")), Err(PortError::Io("x".into())));
    }

    // --- FakeFileSystem ---
    #[test]
    fn metadata_should_return_configured_size_and_modified() {
        let fs = FakeFileSystem::new().with_file(
            r"C:\t\a.bin",
            42,
            Some(EpochMillis(1_700_000_000_000)),
        );
        let meta = fs.metadata(&p(r"C:\t\a.bin")).unwrap();
        assert_eq!(meta.kind, EntryKind::File);
        assert_eq!(meta.size_bytes, 42);
        assert_eq!(meta.modified, Some(EpochMillis(1_700_000_000_000)));
        assert!(!meta.cloud_placeholder);
    }

    #[test]
    fn metadata_should_fail_not_found_with_the_path_when_missing() {
        let fs = FakeFileSystem::new();
        let err = fs.metadata(&p(r"C:\no\existe")).unwrap_err();
        assert!(
            matches!(&err, PortError::NotFound(m) if m.contains(r"C:\no\existe")),
            "{err:?}"
        );
    }

    #[test]
    fn read_dir_should_return_only_direct_children() {
        let fs = FakeFileSystem::new()
            .with_dir(r"C:\t")
            .with_file(r"C:\t\a.txt", 1, None)
            .with_dir(r"C:\t\sub")
            .with_file(r"C:\t\sub\deep.txt", 2, None)
            .with_file(r"C:\otro\x.txt", 3, None);
        let mut names: Vec<PathBuf> = fs
            .read_dir(&p(r"C:\t"))
            .unwrap()
            .into_iter()
            .map(|e| e.path)
            .collect();
        names.sort();
        assert_eq!(names, vec![p(r"C:\t\a.txt"), p(r"C:\t\sub")]);
        assert!(fs.read_dir(&p(r"C:\no\existe")).is_err());
    }

    #[test]
    fn junction_should_be_reported_as_reparse_point_and_hide_its_target_contents() {
        let fs = FakeFileSystem::new()
            .with_dir(r"C:\t")
            .with_reparse_point(r"C:\t\junction")
            .with_file(r"C:\t\junction\secret.txt", 9, None);
        let children = fs.read_dir(&p(r"C:\t")).unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].meta.kind, EntryKind::ReparsePoint);
        assert!(fs.read_dir(&p(r"C:\t\junction")).unwrap().is_empty());
    }

    #[test]
    fn cloud_placeholder_should_be_flagged() {
        let fs = FakeFileSystem::new()
            .with_cloud_placeholder(r"C:\OneDrive\big.iso", 10)
            .with_file(r"C:\local.txt", 10, None);
        assert!(
            fs.metadata(&p(r"C:\OneDrive\big.iso"))
                .unwrap()
                .cloud_placeholder
        );
        assert!(!fs.metadata(&p(r"C:\local.txt")).unwrap().cloud_placeholder);
    }

    #[test]
    fn canonicalize_should_use_explicit_mapping_and_fail_without_one() {
        let fs = FakeFileSystem::new().with_canonical(r"C:\T\..\t", r"C:\t");
        assert_eq!(fs.canonicalize(&p(r"C:\T\..\t")).unwrap(), p(r"C:\t"));
        assert!(matches!(
            fs.canonicalize(&p(r"C:\otra")),
            Err(PortError::NotFound(_))
        ));
    }

    #[test]
    fn fail_on_should_return_the_configured_error() {
        let fs = FakeFileSystem::new()
            .with_file(r"C:\lock.dat", 1, None)
            .fail_on(r"C:\lock.dat", PortError::InUse("lock.dat".into()));
        assert_eq!(
            fs.metadata(&p(r"C:\lock.dat")),
            Err(PortError::InUse("lock.dat".into()))
        );
        assert!(fs.metadata(&p(r"C:\otro.dat")).is_err());
    }

    // --- FakeFileSystem: identidad, hard links, canonicalize, carreras y pin (C-03) ---
    fn id(n: u128) -> FileIdentity {
        FileIdentity {
            volume_serial: 7,
            file_id: n,
        }
    }

    #[test]
    fn metadata_should_return_configured_identity_and_hard_links() {
        let fs = FakeFileSystem::new()
            .with_file(r"C:\t\a.bin", 1, None)
            .with_identity(r"C:\t\a.bin", id(99))
            .with_hard_links(r"C:\t\a.bin", 3);
        let meta = fs.metadata(&p(r"C:\t\a.bin")).unwrap();
        assert_eq!(meta.identity, Some(id(99)));
        assert_eq!(meta.hard_links, Some(3));
    }

    #[test]
    fn metadata_should_give_distinct_default_identities_and_a_single_link() {
        let fs = FakeFileSystem::new()
            .with_file(r"C:\t\a", 1, None)
            .with_file(r"C:\t\b", 1, None);
        let a = fs.metadata(&p(r"C:\t\a")).unwrap();
        let b = fs.metadata(&p(r"C:\t\b")).unwrap();
        assert!(a.identity.is_some());
        assert_ne!(a.identity, b.identity);
        assert_eq!(a.hard_links, Some(1));
    }

    #[test]
    fn without_identity_should_report_none_from_metadata() {
        let fs = FakeFileSystem::new()
            .with_file(r"C:\t\a", 1, None)
            .without_identity(r"C:\t\a");
        assert_eq!(fs.metadata(&p(r"C:\t\a")).unwrap().identity, None);
    }

    #[test]
    fn read_dir_should_not_report_identity_nor_hard_links() {
        let fs = FakeFileSystem::new()
            .with_dir(r"C:\t")
            .with_file(r"C:\t\a", 1, None);
        let children = fs.read_dir(&p(r"C:\t")).unwrap();
        assert_eq!(children[0].meta.identity, None);
        assert_eq!(children[0].meta.hard_links, None);
    }

    #[test]
    fn with_hard_link_should_share_identity_and_count_two_links() {
        let fs = FakeFileSystem::new()
            .with_file(r"C:\t\a", 4, None)
            .with_hard_link(r"C:\t\b", r"C:\t\a");
        let a = fs.metadata(&p(r"C:\t\a")).unwrap();
        let b = fs.metadata(&p(r"C:\t\b")).unwrap();
        assert_eq!(a.identity, b.identity);
        assert_eq!((a.hard_links, b.hard_links), (Some(2), Some(2)));
    }

    #[test]
    fn canonical_as_is_should_resolve_existing_paths_only() {
        let fs = FakeFileSystem::new()
            .with_file(r"C:\t\a", 1, None)
            .canonical_as_is();
        assert_eq!(fs.canonicalize(&p(r"C:\t\a")).unwrap(), p(r"C:\t\a"));
        assert!(matches!(
            fs.canonicalize(&p(r"C:\t\nope")),
            Err(PortError::NotFound(_))
        ));
    }

    #[test]
    fn canonical_as_is_should_give_priority_to_the_explicit_mapping() {
        let fs = FakeFileSystem::new()
            .with_file(r"C:\PROGRA~1\a", 1, None)
            .with_canonical(r"C:\PROGRA~1\a", r"C:\Program Files\a")
            .canonical_as_is();
        assert_eq!(
            fs.canonicalize(&p(r"C:\PROGRA~1\a")).unwrap(),
            p(r"C:\Program Files\a")
        );
    }

    #[test]
    fn recreate_file_should_change_the_identity_keeping_the_path() {
        let fs = FakeFileSystem::new()
            .with_file(r"C:\t\a", 1, None)
            .with_identity(r"C:\t\a", id(1));
        fs.recreate_file(&p(r"C:\t\a"), id(2));
        assert_eq!(fs.metadata(&p(r"C:\t\a")).unwrap().identity, Some(id(2)));
        assert_eq!(fs.metadata(&p(r"C:\t\a")).unwrap().kind, EntryKind::File);
    }

    #[test]
    fn replace_with_reparse_point_should_turn_the_entry_into_a_link() {
        let fs = FakeFileSystem::new().with_file(r"C:\t\a", 1, None);
        fs.replace_with_reparse_point(&p(r"C:\t\a"));
        assert_eq!(
            fs.metadata(&p(r"C:\t\a")).unwrap().kind,
            EntryKind::ReparsePoint
        );
    }

    #[test]
    fn replace_with_directory_should_change_the_kind() {
        let fs = FakeFileSystem::new().with_file(r"C:\t\a", 1, None);
        fs.replace_with_directory(&p(r"C:\t\a"), id(5));
        let meta = fs.metadata(&p(r"C:\t\a")).unwrap();
        assert_eq!(
            (meta.kind, meta.identity),
            (EntryKind::Directory, Some(id(5)))
        );
    }

    #[test]
    fn fail_on_op_should_fail_only_the_given_operation() {
        let fs = FakeFileSystem::new()
            .with_file(r"C:\t\a", 1, None)
            .canonical_as_is()
            .fail_on_op(r"C:\t\a", FsOp::Canonicalize, PortError::InUse("x".into()));
        assert!(fs.metadata(&p(r"C:\t\a")).is_ok());
        assert_eq!(
            fs.canonicalize(&p(r"C:\t\a")),
            Err(PortError::InUse("x".into()))
        );
    }

    #[test]
    fn calls_should_record_operations_in_order() {
        let fs = FakeFileSystem::new()
            .with_dir(r"C:\t")
            .with_file(r"C:\t\a", 1, None)
            .canonical_as_is();
        let _ = fs.metadata(&p(r"C:\t"));
        let _ = fs.canonicalize(&p(r"C:\t\a"));
        assert_eq!(
            fs.calls(),
            vec![
                (FsOp::Metadata, p(r"C:\t")),
                (FsOp::Canonicalize, p(r"C:\t\a"))
            ]
        );
    }

    #[test]
    fn without_hard_links_should_report_none_from_metadata_only_for_that_entry() {
        let fs = FakeFileSystem::new()
            .with_file(r"C:\t\a", 1, None)
            .with_file(r"C:\t\b", 1, None)
            .without_hard_links(r"C:\t\a");
        assert_eq!(fs.metadata(&p(r"C:\t\a")).unwrap().hard_links, None);
        assert_eq!(fs.metadata(&p(r"C:\t\b")).unwrap().hard_links, Some(1));
    }

    #[test]
    fn call_count_and_total_calls_should_count_per_path_and_overall() {
        let fs = FakeFileSystem::new()
            .with_dir(r"C:\t")
            .with_file(r"C:\t\a", 1, None);
        let _ = fs.metadata(&p(r"C:\t"));
        let _ = fs.metadata(&p(r"C:\t"));
        let _ = fs.metadata(&p(r"C:\t\a"));
        let _ = fs.read_dir(&p(r"C:\t"));
        assert_eq!(fs.call_count(FsOp::Metadata, r"C:\t"), 2);
        assert_eq!(fs.call_count(FsOp::Metadata, r"C:\t\a"), 1);
        assert_eq!(fs.call_count(FsOp::ReadDir, r"C:\t"), 1);
        assert_eq!(fs.call_count(FsOp::Canonicalize, r"C:\t"), 0);
        assert_eq!(fs.total_calls(), 4);
    }

    #[test]
    fn with_cloud_reparse_point_should_be_a_reparse_point_flagged_as_cloud() {
        let fs = FakeFileSystem::new().with_cloud_reparse_point(r"C:\od");
        let meta = fs.metadata(&p(r"C:\od")).unwrap();
        assert_eq!(
            (meta.kind, meta.cloud_placeholder),
            (EntryKind::ReparsePoint, true)
        );
        let plain = FakeFileSystem::new().with_reparse_point(r"C:\j");
        assert!(!plain.metadata(&p(r"C:\j")).unwrap().cloud_placeholder);
    }

    fn fs_for_pin() -> FakeFileSystem {
        FakeFileSystem::new()
            .with_dir(r"C:\a")
            .with_dir(r"C:\a\b")
            .with_file(r"C:\a\b\f.txt", 1, None)
    }

    #[test]
    fn pin_ancestors_should_pin_from_the_root_down_and_release_on_drop() {
        let fs = fs_for_pin();
        let pin = fs.pin_ancestors(&p(r"C:\a\b\f.txt")).unwrap();
        assert_eq!(fs.pinned(), vec![p(r"C:\a"), p(r"C:\a\b")]);
        drop(pin);
        assert!(fs.pinned().is_empty());
    }

    #[test]
    fn pin_ancestors_should_fail_on_a_reparse_ancestor_leaving_nothing_pinned() {
        let fs = FakeFileSystem::new()
            .with_dir(r"C:\a")
            .with_reparse_point(r"C:\a\j")
            .with_file(r"C:\a\j\f.txt", 1, None);
        assert!(fs.pin_ancestors(&p(r"C:\a\j\f.txt")).is_err());
        assert!(fs.pinned().is_empty());
    }

    #[test]
    fn pin_ancestors_should_fail_when_an_ancestor_is_configured_to_fail() {
        let fs = fs_for_pin().fail_on(r"C:\a\b", PortError::InUse("b".into()));
        assert_eq!(
            fs.pin_ancestors(&p(r"C:\a\b\f.txt")).err(),
            Some(PortError::InUse("b".into()))
        );
        assert!(fs.pinned().is_empty());
    }

    #[test]
    fn pin_ancestors_should_fail_with_not_found_when_an_ancestor_is_missing() {
        let fs = FakeFileSystem::new().with_dir(r"C:\a");
        assert!(matches!(
            fs.pin_ancestors(&p(r"C:\a\b\f.txt")),
            Err(PortError::NotFound(_))
        ));
        assert!(fs.pinned().is_empty());
    }

    // --- Contrato del puerto FileSystem contra el fake ---
    fn fake_scenario() -> crate::domain::ports::fs_contract::Scenario<FakeFileSystem> {
        let root = p(r"C:\fixture");
        let target = root.join("target");
        let junction = root.join("junction");
        let fs = FakeFileSystem::new()
            .with_dir(r"C:\fixture")
            .with_file(r"C:\fixture\file.txt", 5, None)
            .with_dir(r"C:\fixture\dir")
            .with_file(r"C:\fixture\dir\inner.txt", 1, None)
            .with_dir(r"C:\fixture\target")
            .with_file(r"C:\fixture\target\secret.txt", 1, None)
            .with_reparse_point(r"C:\fixture\junction")
            .with_file(r"C:\fixture\hard_a.dat", 3, None)
            .with_hard_link(r"C:\fixture\hard_b.dat", r"C:\fixture\hard_a.dat")
            .with_canonical(
                r"C:\fixture\junction\secret.txt",
                r"C:\fixture\target\secret.txt",
            )
            .canonical_as_is();
        crate::domain::ports::fs_contract::Scenario {
            fs,
            file: root.join("file.txt"),
            dir: root.join("dir"),
            dir_child: root.join("dir").join("inner.txt"),
            target_child: target.join("secret.txt"),
            target,
            junction,
            hard_a: root.join("hard_a.dat"),
            hard_b: root.join("hard_b.dat"),
            missing: root.join("missing"),
            root,
        }
    }

    mod fake_fs_contract {
        use super::*;
        crate::domain::ports::fs_contract::fs_contract_tests!(fake_scenario());
    }

    // --- FakeTrash (C-09 3.1: solo acepta objetivos re-verificados) ---
    const TEMP_DIR: &str = r"C:\Users\ana\AppData\Local\Temp";

    fn temp_fs() -> FakeFileSystem {
        FakeFileSystem::new()
            .with_dir(r"C:\Users")
            .with_dir(r"C:\Users\ana")
            .with_dir(r"C:\Users\ana\AppData")
            .with_dir(r"C:\Users\ana\AppData\Local")
            .with_dir(TEMP_DIR)
            .with_file(&format!(r"{TEMP_DIR}\a.txt"), 5, None)
            .with_file(&format!(r"{TEMP_DIR}\b.txt"), 5, None)
            .with_file(r"C:\Users\ana\Documents\keep.dll", 9, None)
            .canonical_as_is()
    }

    fn fs_with_files() -> Arc<FakeFileSystem> {
        Arc::new(temp_fs())
    }

    /// Re-verifica `path` con el servicio real y entrega el objetivo (la unica forma de
    /// obtener uno).
    fn target_for(fs: &FakeFileSystem, path: &str) -> DeleteTarget {
        use crate::domain::protection::service::PathProtection;
        use crate::domain::protection::test_support::ctx_c_ana;
        let service = PathProtection::new(fs, Ok(ctx_c_ana()));
        let checked = match service.check(Path::new(path)) {
            ProtectionDecision::Allowed(checked) => checked,
            other => panic!("se esperaba Allowed: {other:?}"),
        };
        match service.prepare_delete(&checked) {
            Ok(target) => target,
            Err(decision) => panic!("prepare_delete fallo: {decision:?}"),
        }
    }

    fn a_txt() -> String {
        format!(r"{TEMP_DIR}\a.txt")
    }

    fn b_txt() -> String {
        format!(r"{TEMP_DIR}\b.txt")
    }

    #[test]
    fn trash_should_record_move_and_remove_path_from_shared_fs() {
        let fs = fs_with_files();
        let trash = FakeTrash::new(fs.clone());
        let target = target_for(&fs, &a_txt());
        assert_eq!(trash.move_to_trash(&target), Ok(()));
        assert_eq!(trash.moved(), vec![p(&a_txt())]);
        assert!(!fs.exists(&p(&a_txt())));
        assert!(fs.exists(&p(&b_txt())));
    }

    #[test]
    fn trash_should_fail_and_keep_the_file_when_a_configured_error_is_set() {
        let fs = fs_with_files();
        let trash = FakeTrash::new(fs.clone())
            .fail_on(&a_txt(), TrashError::InUse)
            .fail_on(&b_txt(), TrashError::BinUnavailable);
        let a = target_for(&fs, &a_txt());
        let b = target_for(&fs, &b_txt());
        assert_eq!(trash.move_to_trash(&a), Err(TrashError::InUse));
        assert_eq!(trash.move_to_trash(&b), Err(TrashError::BinUnavailable));
        assert!(fs.exists(&p(&a_txt())) && fs.exists(&p(&b_txt())));
        assert!(trash.moved().is_empty());
    }

    #[test]
    fn trash_should_report_not_found_when_the_entry_vanished_after_verification() {
        let fs = fs_with_files();
        let trash = FakeTrash::new(fs.clone());
        let target = target_for(&fs, &a_txt());
        fs.remove(&p(&a_txt()));
        assert_eq!(trash.move_to_trash(&target), Err(TrashError::NotFound));
    }

    #[test]
    fn trash_before_move_hook_should_replace_the_entry_with_a_link_and_move_the_link_only() {
        let fs = fs_with_files();
        let victim = a_txt();
        let hook_fs = Arc::clone(&fs);
        let hook_path = p(&victim);
        let trash = FakeTrash::new(fs.clone()).before_move(&victim, move || {
            hook_fs.replace_with_reparse_point(&hook_path);
        });
        let target = target_for(&fs, &victim);
        assert_eq!(trash.move_to_trash(&target), Ok(()));
        // Se movio lo que habia en la ruta (el enlace); el destino real sigue existiendo.
        assert_eq!(trash.moved(), vec![p(&victim)]);
        assert!(!fs.exists(&p(&victim)));
        assert!(fs.exists(&p(r"C:\Users\ana\Documents\keep.dll")));
    }

    #[test]
    fn trash_before_move_hook_should_run_while_the_ancestors_are_still_pinned() {
        let fs = fs_with_files();
        let seen = Arc::new(Mutex::new(None));
        let (hook_fs, hook_seen) = (Arc::clone(&fs), Arc::clone(&seen));
        let trash = FakeTrash::new(fs.clone()).before_move(&a_txt(), move || {
            if let Ok(mut slot) = hook_seen.lock() {
                *slot = Some(hook_fs.pinned().len());
            }
        });
        let target = target_for(&fs, &a_txt());
        assert_eq!(trash.move_to_trash(&target), Ok(()));
        assert_eq!(*seen.lock().unwrap(), Some(5));
        drop(target);
        assert!(fs.pinned().is_empty());
    }

    #[test]
    fn trash_should_report_success_without_moving_when_configured_to_do_so() {
        let fs = fs_with_files();
        let trash = FakeTrash::new(fs.clone()).succeed_without_moving(&a_txt());
        let target = target_for(&fs, &a_txt());
        assert_eq!(trash.move_to_trash(&target), Ok(()));
        assert!(fs.exists(&p(&a_txt())));
        assert!(trash.moved().is_empty());
    }

    // --- FakeSensors / FixedClock ---
    fn sample(cpu: f32, disk: Option<f32>) -> SensorSample {
        SensorSample {
            cpu_pct: cpu,
            ram_used_bytes: 1,
            ram_total_bytes: 2,
            disk_busy_pct: disk,
        }
    }

    #[test]
    fn sensors_should_return_scripted_samples_in_order_then_unsupported() {
        let mut s = FakeSensors::new(vec![Ok(sample(10.0, Some(5.0))), Ok(sample(20.0, None))]);
        assert_eq!(s.sample().unwrap().cpu_pct, 10.0);
        let second = s.sample().unwrap();
        assert_eq!(second.cpu_pct, 20.0);
        assert_eq!(second.disk_busy_pct, None);
        assert!(matches!(s.sample(), Err(PortError::Unsupported(_))));
    }

    #[test]
    fn sensors_should_replay_scripted_errors() {
        let mut s = FakeSensors::new(vec![Err(PortError::Io("x".into())), Ok(sample(1.0, None))]);
        assert_eq!(s.sample(), Err(PortError::Io("x".into())));
        assert!(s.sample().is_ok());
    }

    #[test]
    fn fixed_clock_should_return_its_value_and_differ_between_instances() {
        let a = FixedClock(EpochMillis(1_700_000_000_000));
        let b = FixedClock(EpochMillis(1_800_000_000_000));
        assert_eq!(a.now(), EpochMillis(1_700_000_000_000));
        assert_ne!(a.now(), b.now());
    }

    // --- InMemoryStore contra la bateria de contrato ---
    #[test]
    fn in_memory_store_should_seed_defaults() {
        contract::contract_seed_defaults(InMemoryStore::new);
    }

    #[test]
    fn in_memory_store_should_satisfy_settings_contract() {
        contract::contract_settings(InMemoryStore::new);
    }

    #[test]
    fn in_memory_store_should_satisfy_scan_contracts() {
        contract::contract_scan_lifecycle(InMemoryStore::new);
        contract::contract_scan_listing_order(InMemoryStore::new);
    }

    #[test]
    fn in_memory_store_should_satisfy_retention_contracts() {
        contract::contract_retention(InMemoryStore::new);
        contract::contract_log_survives_scan_pruning(InMemoryStore::new);
    }

    #[test]
    fn in_memory_store_should_satisfy_log_contracts() {
        contract::contract_log(InMemoryStore::new);
        contract::contract_log_batch_is_atomic(InMemoryStore::new);
    }

    #[test]
    fn in_memory_store_should_satisfy_contract_junk_scan_round_trip() {
        contract::contract_junk_scan_round_trip(InMemoryStore::new);
    }

    #[test]
    fn in_memory_store_should_satisfy_contract_junk_second_scan_purges_previous_findings_keeping_summaries()
     {
        contract::contract_junk_second_scan_purges_previous_findings_keeping_summaries(
            InMemoryStore::new,
        );
    }

    #[test]
    fn in_memory_store_should_satisfy_contract_junk_failed_save_is_atomic() {
        contract::contract_junk_failed_save_is_atomic(InMemoryStore::new);
    }

    #[test]
    fn in_memory_store_should_satisfy_contract_junk_complete_requires_a_running_junk_scan() {
        contract::contract_junk_complete_requires_a_running_junk_scan(InMemoryStore::new);
    }

    #[test]
    fn in_memory_store_should_satisfy_contract_junk_findings_pagination() {
        contract::contract_junk_findings_pagination(InMemoryStore::new);
    }

    #[test]
    fn in_memory_store_should_satisfy_contract_last_done_junk_scan_ignores_unfinished_and_other_kinds()
     {
        contract::contract_last_done_junk_scan_ignores_unfinished_and_other_kinds(
            InMemoryStore::new,
        );
    }

    #[test]
    fn in_memory_store_should_satisfy_contract_junk_findings_follow_scan_retention() {
        contract::contract_junk_findings_follow_scan_retention(InMemoryStore::new);
    }

    #[test]
    fn in_memory_store_should_satisfy_contract_cleanup_candidates() {
        contract::contract_cleanup_candidates_only_eligible_of_requested_categories(
            InMemoryStore::new,
        );
        contract::contract_cleanup_candidates_order_by_category_then_size_then_id(
            InMemoryStore::new,
        );
        contract::contract_cleanup_candidates_keep_the_identity_bits_intact(InMemoryStore::new);
        contract::contract_cleanup_candidates_are_empty_without_findings(InMemoryStore::new);
    }

    #[test]
    fn in_memory_store_should_satisfy_contract_scan_cleaned_at() {
        contract::contract_scan_cleaned_at_is_the_latest_log_entry_since_the_scan_ended(
            InMemoryStore::new,
        );
        contract::contract_scan_cleaned_at_ignores_other_scans_and_older_entries(
            InMemoryStore::new,
        );
        contract::contract_empty_bin_entry_does_not_mark_the_scan_as_cleaned(InMemoryStore::new);
    }
}

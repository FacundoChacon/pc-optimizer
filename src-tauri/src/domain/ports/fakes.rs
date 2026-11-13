//! Fakes en memoria de los puertos (solo tests).

use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::domain::model::{
    CleanupLogEntry, DEFAULT_SETTINGS, EpochMillis, NewCleanupLogEntry, NewScan, SCAN_RETENTION,
    Scan, ScanId, ScanStatus, scans_to_prune,
};
use crate::domain::ports::{
    CleanupLogStore, Clock, DirEntry, EntryKind, FileMeta, FileSystem, PortError, ScanStore,
    SensorSample, Sensors, SettingsStore, StoreError, Trash,
};

#[derive(Default)]
struct FsState {
    entries: BTreeMap<PathBuf, FileMeta>,
    canonical: BTreeMap<PathBuf, PathBuf>,
    failures: BTreeMap<PathBuf, PortError>,
}

/// Sistema de archivos falso configurable.
#[derive(Default)]
pub(crate) struct FakeFileSystem {
    state: Mutex<FsState>,
}

impl FakeFileSystem {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    fn with_state(self, f: impl FnOnce(&mut FsState)) -> Self {
        if let Ok(mut state) = self.state.lock() {
            f(&mut state);
        }
        self
    }

    fn meta(kind: EntryKind, size_bytes: u64, modified: Option<EpochMillis>) -> FileMeta {
        FileMeta {
            kind,
            size_bytes,
            modified,
            cloud_placeholder: false,
        }
    }

    pub(crate) fn with_dir(self, path: &str) -> Self {
        self.with_state(|s| {
            s.entries.insert(
                PathBuf::from(path),
                Self::meta(EntryKind::Directory, 0, None),
            );
        })
    }

    pub(crate) fn with_file(self, path: &str, size: u64, modified: Option<EpochMillis>) -> Self {
        self.with_state(|s| {
            s.entries.insert(
                PathBuf::from(path),
                Self::meta(EntryKind::File, size, modified),
            );
        })
    }

    pub(crate) fn with_reparse_point(self, path: &str) -> Self {
        self.with_state(|s| {
            s.entries.insert(
                PathBuf::from(path),
                Self::meta(EntryKind::ReparsePoint, 0, None),
            );
        })
    }

    pub(crate) fn with_cloud_placeholder(self, path: &str, size: u64) -> Self {
        self.with_state(|s| {
            let mut meta = Self::meta(EntryKind::File, size, None);
            meta.cloud_placeholder = true;
            s.entries.insert(PathBuf::from(path), meta);
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
        let state = self.lock()?;
        if let Some(error) = state.failures.get(path) {
            return Err(error.clone());
        }
        state
            .canonical
            .get(path)
            .cloned()
            .ok_or_else(|| PortError::NotFound(path.display().to_string()))
    }

    fn metadata(&self, path: &Path) -> Result<FileMeta, PortError> {
        let state = self.lock()?;
        if let Some(error) = state.failures.get(path) {
            return Err(error.clone());
        }
        state
            .entries
            .get(path)
            .cloned()
            .ok_or_else(|| PortError::NotFound(path.display().to_string()))
    }

    fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>, PortError> {
        let state = self.lock()?;
        if let Some(error) = state.failures.get(path) {
            return Err(error.clone());
        }
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
                .map(|(child, meta)| DirEntry {
                    path: child.clone(),
                    meta: meta.clone(),
                })
                .collect()),
        }
    }
}

/// Papelera falsa: registra lo movido y lo quita del sistema de archivos compartido.
pub(crate) struct FakeTrash {
    fs: Arc<FakeFileSystem>,
    failures: Mutex<BTreeMap<PathBuf, PortError>>,
    moved: Mutex<Vec<PathBuf>>,
}

impl FakeTrash {
    pub(crate) fn new(fs: Arc<FakeFileSystem>) -> Self {
        Self {
            fs,
            failures: Mutex::new(BTreeMap::new()),
            moved: Mutex::new(Vec::new()),
        }
    }

    pub(crate) fn fail_on(self, path: &str, error: PortError) -> Self {
        if let Ok(mut f) = self.failures.lock() {
            f.insert(PathBuf::from(path), error);
        }
        self
    }

    pub(crate) fn moved(&self) -> Vec<PathBuf> {
        self.moved.lock().map(|m| m.clone()).unwrap_or_default()
    }
}

impl Trash for FakeTrash {
    fn move_to_trash(&self, path: &Path) -> Result<(), PortError> {
        let failure = self
            .failures
            .lock()
            .map_err(|_| PortError::Io("fake trash lock poisoned".into()))?
            .get(path)
            .cloned();
        if let Some(error) = failure {
            return Err(error);
        }
        if !self.fs.exists(path) {
            return Err(PortError::NotFound(path.display().to_string()));
        }
        self.fs.remove(path);
        self.moved
            .lock()
            .map_err(|_| PortError::Io("fake trash lock poisoned".into()))?
            .push(path.to_path_buf());
        Ok(())
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ports::store_contract as contract;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn ports_should_be_object_safe() {
        let fs = FakeFileSystem::new();
        let _: &dyn FileSystem = &fs;
        let trash = FakeTrash::new(Arc::new(FakeFileSystem::new()));
        let _: &dyn Trash = &trash;
        let clock = FixedClock(EpochMillis(1));
        let _: &dyn Clock = &clock;
        let mut sensors = FakeSensors::new(vec![]);
        let _: &mut dyn Sensors = &mut sensors;
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

    // --- FakeTrash ---
    fn fs_with_files() -> Arc<FakeFileSystem> {
        Arc::new(
            FakeFileSystem::new()
                .with_file(r"C:\t\a.txt", 5, None)
                .with_file(r"C:\t\b.txt", 5, None),
        )
    }

    #[test]
    fn trash_should_record_move_and_remove_path_from_shared_fs() {
        let fs = fs_with_files();
        let trash = FakeTrash::new(fs.clone());
        trash.move_to_trash(&p(r"C:\t\a.txt")).unwrap();
        assert_eq!(trash.moved(), vec![p(r"C:\t\a.txt")]);
        assert!(!fs.exists(&p(r"C:\t\a.txt")));
        assert!(fs.exists(&p(r"C:\t\b.txt")));
    }

    #[test]
    fn trash_should_fail_and_keep_the_file_when_in_use_or_unsupported() {
        let fs = fs_with_files();
        let trash = FakeTrash::new(fs.clone())
            .fail_on(r"C:\t\a.txt", PortError::InUse("a".into()))
            .fail_on(
                r"C:\t\b.txt",
                PortError::Unsupported("no recycle bin".into()),
            );
        assert_eq!(
            trash.move_to_trash(&p(r"C:\t\a.txt")),
            Err(PortError::InUse("a".into()))
        );
        assert!(matches!(
            trash.move_to_trash(&p(r"C:\t\b.txt")),
            Err(PortError::Unsupported(_))
        ));
        assert!(fs.exists(&p(r"C:\t\a.txt")) && fs.exists(&p(r"C:\t\b.txt")));
        assert!(trash.moved().is_empty());
    }

    #[test]
    fn trash_should_report_not_found_for_missing_path() {
        let trash = FakeTrash::new(fs_with_files());
        assert!(matches!(
            trash.move_to_trash(&p(r"C:\no")),
            Err(PortError::NotFound(_))
        ));
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
}

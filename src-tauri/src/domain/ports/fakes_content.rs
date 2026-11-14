//! Doble en memoria del puerto `ContentReader` (solo tests).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::domain::ports::fakes::FakeFileSystem;
use crate::domain::ports::{ContentHandle, ContentReader, FileMeta, PortError};

/// Contenido de un archivo del doble de lectura.
enum FakeContent {
    Bytes(Vec<u8>),
    /// Archivo virtual de `size` bytes todos iguales a `byte` (para tamanos enormes).
    Filled {
        size: u64,
        byte: u8,
    },
}

impl FakeContent {
    fn len(&self) -> u64 {
        match self {
            Self::Bytes(bytes) => bytes.len() as u64,
            Self::Filled { size, .. } => *size,
        }
    }

    fn fill(&self, offset: u64, buf: &mut [u8]) -> usize {
        let remaining = self.len().saturating_sub(offset);
        let count = usize::try_from(remaining).map_or(buf.len(), |r| r.min(buf.len()));
        match self {
            Self::Bytes(bytes) => {
                let start = usize::try_from(offset).unwrap_or(usize::MAX);
                if start < bytes.len() {
                    buf[..count].copy_from_slice(&bytes[start..start + count]);
                }
            }
            Self::Filled { byte, .. } => buf[..count].fill(*byte),
        }
        count
    }
}

type Hook = Arc<dyn Fn() + Send + Sync>;

#[derive(Default)]
struct ContentState {
    contents: BTreeMap<PathBuf, FakeContent>,
    open_errors: BTreeMap<PathBuf, PortError>,
    read_errors: BTreeMap<PathBuf, PortError>,
    opened: Vec<PathBuf>,
    reads: BTreeMap<PathBuf, u64>,
    bytes_read: BTreeMap<PathBuf, u64>,
    /// `(ruta, n)`: despues de la lectura numero `n` (1-based) de esa ruta corre el gancho.
    after_read: Vec<(PathBuf, u64, Hook)>,
    /// Corre al abrir esa ruta, antes de leer sus metadatos.
    on_open: BTreeMap<PathBuf, Hook>,
}

/// Lector de contenido falso: el contenido va por ruta y los metadatos se leen del
/// `FakeFileSystem` compartido (sin registrar llamadas), asi identidad, tamano y fecha son los de
/// la entrada de ese momento.
pub(crate) struct FakeContentReader {
    fs: Arc<FakeFileSystem>,
    state: Arc<Mutex<ContentState>>,
}

impl FakeContentReader {
    pub(crate) fn new(fs: Arc<FakeFileSystem>) -> Self {
        Self {
            fs,
            state: Arc::new(Mutex::new(ContentState::default())),
        }
    }

    fn with_state(self, f: impl FnOnce(&mut ContentState)) -> Self {
        self.mutate(f);
        self
    }

    fn mutate(&self, f: impl FnOnce(&mut ContentState)) {
        if let Ok(mut state) = self.state.lock() {
            f(&mut state);
        }
    }

    pub(crate) fn with_content(self, path: &str, bytes: Vec<u8>) -> Self {
        self.with_state(|s| {
            s.contents
                .insert(PathBuf::from(path), FakeContent::Bytes(bytes));
        })
    }

    /// Archivo virtual de `size` bytes iguales a `byte`.
    pub(crate) fn with_filled(self, path: &str, size: u64, byte: u8) -> Self {
        self.with_state(|s| {
            s.contents
                .insert(PathBuf::from(path), FakeContent::Filled { size, byte });
        })
    }

    pub(crate) fn fail_open(self, path: &str, error: PortError) -> Self {
        self.with_state(|s| {
            s.open_errors.insert(PathBuf::from(path), error);
        })
    }

    pub(crate) fn fail_read(self, path: &str, error: PortError) -> Self {
        self.with_state(|s| {
            s.read_errors.insert(PathBuf::from(path), error);
        })
    }

    /// Cambia el contenido de una ruta (carrera: se edita el archivo).
    pub(crate) fn replace_content(&self, path: &Path, bytes: Vec<u8>) {
        self.mutate(|s| {
            s.contents
                .insert(path.to_path_buf(), FakeContent::Bytes(bytes));
        });
    }

    /// Mutacion programada: corre `hook` justo despues de la lectura `n` (1-based) de `path`.
    pub(crate) fn after_read(
        self,
        path: &str,
        n: u64,
        hook: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        self.with_state(|s| s.after_read.push((PathBuf::from(path), n, Arc::new(hook))))
    }

    /// Corre `hook` cada vez que se abre `path`, antes de leer sus metadatos.
    pub(crate) fn on_open(self, path: &str, hook: impl Fn() + Send + Sync + 'static) -> Self {
        self.with_state(|s| {
            s.on_open.insert(PathBuf::from(path), Arc::new(hook));
        })
    }

    /// Rutas abiertas, en orden (una entrada por apertura).
    pub(crate) fn opened(&self) -> Vec<PathBuf> {
        self.state
            .lock()
            .map(|s| s.opened.clone())
            .unwrap_or_default()
    }

    pub(crate) fn open_count(&self, path: &str) -> usize {
        self.opened()
            .iter()
            .filter(|opened| opened.as_path() == Path::new(path))
            .count()
    }

    /// Bytes leidos en total (todas las rutas).
    pub(crate) fn bytes_read(&self) -> u64 {
        self.state
            .lock()
            .map(|s| s.bytes_read.values().sum())
            .unwrap_or(0)
    }

    pub(crate) fn bytes_read_of(&self, path: &str) -> u64 {
        self.state
            .lock()
            .map(|s| s.bytes_read.get(Path::new(path)).copied().unwrap_or(0))
            .unwrap_or(0)
    }

    /// Cantidad de lecturas (`read_at`) hechas sobre `path`.
    pub(crate) fn reads_of(&self, path: &str) -> u64 {
        self.state
            .lock()
            .map(|s| s.reads.get(Path::new(path)).copied().unwrap_or(0))
            .unwrap_or(0)
    }
}

struct FakeContentHandle {
    path: PathBuf,
    fs: Arc<FakeFileSystem>,
    state: Arc<Mutex<ContentState>>,
}

impl ContentReader for FakeContentReader {
    fn open(&self, path: &Path) -> Result<Box<dyn ContentHandle>, PortError> {
        let hook = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| PortError::Io("fake content lock poisoned".into()))?;
            state.opened.push(path.to_path_buf());
            if let Some(error) = state.open_errors.get(path) {
                return Err(error.clone());
            }
            state.on_open.get(path).cloned()
        };
        // Fuera del cerrojo: el gancho puede tocar el doble.
        if let Some(hook) = hook {
            hook();
        }
        let present = self
            .state
            .lock()
            .map(|s| s.contents.contains_key(path))
            .unwrap_or(false);
        if !present || self.fs.peek(path).is_none() {
            return Err(PortError::NotFound(path.display().to_string()));
        }
        Ok(Box::new(FakeContentHandle {
            path: path.to_path_buf(),
            fs: Arc::clone(&self.fs),
            state: Arc::clone(&self.state),
        }))
    }
}

impl ContentHandle for FakeContentHandle {
    fn meta(&self) -> Result<FileMeta, PortError> {
        self.fs
            .peek(&self.path)
            .ok_or_else(|| PortError::NotFound(self.path.display().to_string()))
    }

    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<usize, PortError> {
        let (count, hooks) = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| PortError::Io("fake content lock poisoned".into()))?;
            if let Some(error) = state.read_errors.get(&self.path) {
                return Err(error.clone());
            }
            let count = match state.contents.get(&self.path) {
                Some(content) => content.fill(offset, buf),
                None => return Err(PortError::NotFound(self.path.display().to_string())),
            };
            let reads = state.reads.entry(self.path.clone()).or_insert(0);
            *reads += 1;
            let reads = *reads;
            *state.bytes_read.entry(self.path.clone()).or_insert(0) += count as u64;
            let hooks: Vec<Hook> = state
                .after_read
                .iter()
                .filter(|(path, n, _)| *path == self.path && *n == reads)
                .map(|(_, _, hook)| Arc::clone(hook))
                .collect();
            (count, hooks)
        };
        for hook in hooks {
            hook();
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::model::EpochMillis;
    use crate::domain::ports::content_contract as contract;
    use crate::domain::ports::{EntryKind, FileIdentity};

    const PATH: &str = r"C:\Users\ana\Downloads\a.bin";

    fn data(len: usize) -> Vec<u8> {
        contract::sample_bytes(len)
    }

    fn fake(bytes: Vec<u8>) -> (Arc<FakeFileSystem>, FakeContentReader) {
        let fs = Arc::new(FakeFileSystem::new().with_file(
            PATH,
            bytes.len() as u64,
            Some(EpochMillis(1)),
        ));
        let reader = FakeContentReader::new(Arc::clone(&fs)).with_content(PATH, bytes);
        (fs, reader)
    }

    #[test]
    fn fake_should_satisfy_the_shared_content_contract() {
        let bytes = data(contract::SAMPLE_LEN);
        let (_fs, reader) = fake(bytes);
        let scenario = contract::Scenario {
            reader,
            file: PathBuf::from(PATH),
            missing: PathBuf::from(r"C:\Users\ana\Downloads\missing.bin"),
        };
        contract::run_all(&scenario);
    }

    #[test]
    fn read_at_should_return_the_exact_head_and_tail_of_a_3_mib_file() {
        let bytes = data(3 * 1024 * 1024);
        let (_fs, reader) = fake(bytes.clone());
        let mut handle = reader.open(Path::new(PATH)).unwrap();
        let mut head = vec![0u8; 64 * 1024];
        let mut tail = vec![0u8; 64 * 1024];
        assert_eq!(handle.read_at(0, &mut head).unwrap(), 64 * 1024);
        assert_eq!(
            handle
                .read_at((bytes.len() - 64 * 1024) as u64, &mut tail)
                .unwrap(),
            64 * 1024
        );
        assert_eq!(head, bytes[..64 * 1024]);
        assert_eq!(tail, bytes[bytes.len() - 64 * 1024..]);
    }

    #[test]
    fn read_at_should_return_zero_past_the_end_and_a_short_count_at_the_end() {
        let (_fs, reader) = fake(data(100));
        let mut handle = reader.open(Path::new(PATH)).unwrap();
        let mut buf = [0u8; 64];
        assert_eq!(handle.read_at(100, &mut buf).unwrap(), 0);
        assert_eq!(handle.read_at(1_000, &mut buf).unwrap(), 0);
        assert_eq!(handle.read_at(90, &mut buf).unwrap(), 10);
    }

    #[test]
    fn open_should_report_the_configured_errors() {
        let (_fs, reader) = fake(data(10));
        let reader = reader.fail_open(PATH, PortError::InUse("x".into()));
        assert!(matches!(
            reader.open(Path::new(PATH)),
            Err(PortError::InUse(_))
        ));
        // Aunque falle, la apertura queda registrada.
        assert_eq!(reader.open_count(PATH), 1);
        assert!(matches!(
            reader.open(Path::new(r"C:\no\esta.bin")),
            Err(PortError::NotFound(_))
        ));
    }

    #[test]
    fn read_should_report_a_configured_read_error() {
        let (_fs, reader) = fake(data(10));
        let reader = reader.fail_read(PATH, PortError::Io("disco".into()));
        let mut handle = reader.open(Path::new(PATH)).unwrap();
        let mut buf = [0u8; 4];
        assert!(matches!(handle.read_at(0, &mut buf), Err(PortError::Io(_))));
    }

    #[test]
    fn meta_should_show_a_mutation_scheduled_after_the_first_read() {
        let (fs, reader) = fake(data(1_000));
        let fs_hook = Arc::clone(&fs);
        let reader = reader.after_read(PATH, 1, move || {
            fs_hook.set_size_and_time(Path::new(PATH), 2_000, Some(EpochMillis(9)));
        });
        let mut handle = reader.open(Path::new(PATH)).unwrap();
        assert_eq!(handle.meta().unwrap().size_bytes, 1_000);
        let mut buf = [0u8; 8];
        handle.read_at(0, &mut buf).unwrap();
        let after = handle.meta().unwrap();
        assert_eq!(
            (after.size_bytes, after.modified),
            (2_000, Some(EpochMillis(9)))
        );
    }

    #[test]
    fn meta_should_show_a_replaced_identity_when_the_hook_recreates_the_file() {
        let (fs, reader) = fake(data(10));
        let fs_hook = Arc::clone(&fs);
        let reader = reader.on_open(PATH, move || {
            fs_hook.recreate_file(
                Path::new(PATH),
                FileIdentity {
                    volume_serial: 7,
                    file_id: 77,
                },
            );
        });
        let handle = reader.open(Path::new(PATH)).unwrap();
        let meta = handle.meta().unwrap();
        assert_eq!(meta.kind, EntryKind::File);
        assert_eq!(meta.identity.map(|i| i.file_id), Some(77));
    }

    #[test]
    fn filled_files_should_read_without_allocating_their_size() {
        let fs = Arc::new(FakeFileSystem::new().with_file(PATH, 2 << 30, Some(EpochMillis(1))));
        let reader = FakeContentReader::new(Arc::clone(&fs)).with_filled(PATH, 2 << 30, 7);
        let mut handle = reader.open(Path::new(PATH)).unwrap();
        let mut buf = vec![0u8; 1024];
        assert_eq!(handle.read_at((2u64 << 30) - 10, &mut buf).unwrap(), 10);
        assert_eq!(&buf[..10], &[7u8; 10]);
        assert_eq!(reader.bytes_read_of(PATH), 10);
        assert_eq!(reader.reads_of(PATH), 1);
    }
}

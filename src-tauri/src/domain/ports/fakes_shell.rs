//! Dobles de `VolumeSpace`, `FolderPicker` y `Explorer` (solo tests).

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::domain::ports::{Explorer, FolderPicker, PortError, VolumeSpace, VolumeSpaceInfo};

/// Espacio del volumen fijo o con error.
pub(crate) struct FakeVolumeSpace {
    result: Result<VolumeSpaceInfo, PortError>,
}

impl FakeVolumeSpace {
    pub(crate) fn with_values(total_bytes: u64, free_bytes: u64) -> Self {
        Self {
            result: Ok(VolumeSpaceInfo {
                total_bytes,
                free_bytes,
            }),
        }
    }

    pub(crate) fn failing(error: PortError) -> Self {
        Self { result: Err(error) }
    }
}

impl VolumeSpace for FakeVolumeSpace {
    fn space(&self, _root: &Path) -> Result<VolumeSpaceInfo, PortError> {
        self.result.clone()
    }
}

/// Selector guionado: cada apertura consume una respuesta; sin guion restante, cancela.
pub(crate) struct FakeFolderPicker {
    script: Mutex<VecDeque<Result<Option<PathBuf>, PortError>>>,
    openings: Mutex<u32>,
}

impl FakeFolderPicker {
    pub(crate) fn scripted(script: Vec<Result<Option<PathBuf>, PortError>>) -> Self {
        Self {
            script: Mutex::new(script.into()),
            openings: Mutex::new(0),
        }
    }

    pub(crate) fn openings(&self) -> u32 {
        self.openings.lock().map(|n| *n).unwrap_or_default()
    }
}

impl FolderPicker for FakeFolderPicker {
    fn pick(&self) -> Result<Option<PathBuf>, PortError> {
        if let Ok(mut n) = self.openings.lock() {
            *n += 1;
        }
        self.script
            .lock()
            .ok()
            .and_then(|mut script| script.pop_front())
            .unwrap_or(Ok(None))
    }
}

/// Explorador que registra las rutas mostradas; no ejecuta nada.
pub(crate) struct FakeExplorer {
    error: Option<PortError>,
    revealed: Mutex<Vec<PathBuf>>,
}

impl FakeExplorer {
    pub(crate) fn new() -> Self {
        Self {
            error: None,
            revealed: Mutex::new(Vec::new()),
        }
    }

    pub(crate) fn failing(error: PortError) -> Self {
        Self {
            error: Some(error),
            revealed: Mutex::new(Vec::new()),
        }
    }

    pub(crate) fn revealed(&self) -> Vec<PathBuf> {
        self.revealed.lock().map(|v| v.clone()).unwrap_or_default()
    }
}

impl Explorer for FakeExplorer {
    fn reveal(&self, path: &Path) -> Result<(), PortError> {
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        if let Ok(mut revealed) = self.revealed.lock() {
            revealed.push(path.to_path_buf());
        }
        Ok(())
    }
}

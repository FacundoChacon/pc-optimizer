//! Orquestacion del analisis de espacio en disco (C-07, design D-01, D-03, D-06, D-15, D-16,
//! D-17): valida la raiz, recorre con el nucleo compartido, consolida y arma el resultado.
//!
//! Solo lee: lista carpetas y usa los metadatos de la enumeracion. Nunca abre un archivo, nunca
//! escribe ni borra. Un unico hilo.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::domain::disk_usage::aggregate::{FolderAggregator, FolderTree};
use crate::domain::disk_usage::classify::Classifier;
use crate::domain::disk_usage::model::{
    FolderCounters, FolderScanResults, MAX_STORED_FOLDERS, RootKind, StoredFolder, StoredLargeFile,
};
use crate::domain::disk_usage::root::validate_root;
pub use crate::domain::disk_usage::root::{FolderScanError, RootChoice};
use crate::domain::disk_usage::select::select_folders;
use crate::domain::disk_usage::top_files::nearest_kept;
use crate::domain::junk::walk::{WalkEnd, WalkVisitor, walk_directory};
use crate::domain::ports::{DirEntry, EntryKind, FileSystem, PortError, VolumeSpace};
use crate::domain::protection::context::{ContextError, ProtectionContext};
use crate::domain::protection::decision::{ProtectedReason, ResolveFailure};
use crate::domain::protection::path::{NormalizedPath, normalize_lexical};

/// Cada cuantos archivos vistos se avisa (D-17): por conteo, sin reloj.
pub const PROGRESS_EVERY_FILES: u64 = 4_096;
/// Cada cuantas carpetas listadas se avisa; lo primero que ocurra.
pub const PROGRESS_EVERY_DIRS: u64 = 512;

/// Receptor del avance. Solo conteos: ningun evento lleva rutas.
pub trait FolderScanObserver {
    /// Archivos, carpetas y bytes vistos hasta ahora.
    fn walking(&mut self, files_seen: u64, dirs_seen: u64, bytes_seen: u64);
    /// El recorrido termino y empieza el guardado (una sola vez).
    fn saving(&mut self);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FolderScanOutcome {
    Completed(FolderScanResults),
    Cancelled,
}

/// Analiza `root` y devuelve el resultado, o `Cancelled` sin resultado.
///
/// `protection` es el contexto ya restringido por privilegios (sin usuario en over-the-shoulder).
pub fn scan_folders<F: FileSystem, V: VolumeSpace, O: FolderScanObserver>(
    root: RootChoice,
    protection: Result<ProtectionContext, ContextError>,
    fs: &F,
    volume: &V,
    cancel: &AtomicBool,
    observer: &mut O,
) -> Result<FolderScanOutcome, FolderScanError> {
    let root = validate_root(fs, &protection, &root)?;
    let ctx = protection
        .map_err(|_| FolderScanError::RootRejected(ProtectedReason::ContextUnavailable))?;
    let kind = root.kind();
    let root_path = root.path();
    let root_buf = root.to_path_buf();

    let classifier = Classifier::new(&ctx);
    let mut tracked = Tracked {
        aggregator: FolderAggregator::new(&classifier, cancel),
        observer,
        files: 0,
        dirs: 0,
        bytes: 0,
        files_since: 0,
        dirs_since: 0,
    };
    let report = walk_directory(fs, &root_buf, |_| false, &mut tracked);
    if report.end == WalkEnd::Cancelled {
        return Ok(FolderScanOutcome::Cancelled);
    }
    if !report.present {
        return Err(FolderScanError::RootRejected(
            ProtectedReason::Unresolvable(ResolveFailure::NotFound),
        ));
    }
    tracked.emit();
    let Tracked {
        aggregator,
        observer,
        ..
    } = tracked;
    if cancel.load(Ordering::Relaxed) {
        return Ok(FolderScanOutcome::Cancelled);
    }
    observer.saving();

    let tree = aggregator.finish();
    let volume_info = volume.space(&root_buf).ok();
    Ok(FolderScanOutcome::Completed(build_results(
        &tree,
        &classifier,
        kind,
        root_path,
        volume_info,
    )))
}

/// Envuelve al agregador para avisar el progreso por conteo.
struct Tracked<'a, 'o, O: FolderScanObserver> {
    aggregator: FolderAggregator<'a>,
    observer: &'o mut O,
    files: u64,
    dirs: u64,
    bytes: u64,
    files_since: u64,
    dirs_since: u64,
}

impl<O: FolderScanObserver> Tracked<'_, '_, O> {
    fn emit(&mut self) {
        self.files_since = 0;
        self.dirs_since = 0;
        self.observer.walking(self.files, self.dirs, self.bytes);
    }

    fn emit_if_due(&mut self) {
        if self.files_since >= PROGRESS_EVERY_FILES || self.dirs_since >= PROGRESS_EVERY_DIRS {
            self.emit();
        }
    }
}

impl<O: FolderScanObserver> WalkVisitor for Tracked<'_, '_, O> {
    fn is_cancelled(&self) -> bool {
        self.aggregator.is_cancelled()
    }

    fn file_seen(&mut self) {
        self.aggregator.file_seen();
        self.files += 1;
        self.files_since += 1;
        self.emit_if_due();
    }

    fn candidate(&mut self, entry: DirEntry) {
        if entry.meta.kind == EntryKind::File && !entry.meta.cloud_placeholder {
            self.bytes += entry.meta.size_bytes;
        }
        self.aggregator.candidate(entry);
    }

    fn reparse_point(&mut self, path: PathBuf) {
        self.aggregator.reparse_point(path);
    }

    fn dir_listed(&mut self, path: &Path) {
        self.aggregator.dir_listed(path);
        self.dirs += 1;
        self.dirs_since += 1;
        self.emit_if_due();
    }

    fn dir_inaccessible(&mut self, path: &Path, error: &PortError) {
        self.aggregator.dir_inaccessible(path, error);
        self.dirs += 1;
        self.dirs_since += 1;
        self.emit_if_due();
    }
}

fn build_results(
    tree: &FolderTree,
    classifier: &Classifier<'_>,
    kind: RootKind,
    root_path: &NormalizedPath,
    volume: Option<crate::domain::ports::VolumeSpaceInfo>,
) -> FolderScanResults {
    let selection = select_folders(tree, MAX_STORED_FOLDERS);
    let mut kept_mask = vec![false; tree.nodes.len()];
    let mut new_index = vec![usize::MAX; tree.nodes.len()];
    for (position, old) in selection.kept.iter().enumerate() {
        kept_mask[*old] = true;
        new_index[*old] = position;
    }
    let folders: Vec<StoredFolder> = selection
        .kept
        .iter()
        .map(|old| {
            let node = &tree.nodes[*old];
            let path = tree.path_of(*old);
            let normalized = normalize_lexical(&path).ok();
            StoredFolder {
                parent: node.parent.map(|parent| new_index[parent]),
                depth: node.depth,
                size_bytes: node.size_bytes,
                direct_bytes: node.direct_bytes,
                file_count: node.file_count,
                dir_count: node.dir_count,
                access: node.access,
                opaque: node.opaque,
                system: normalized
                    .as_ref()
                    .is_some_and(|path| classifier.is_system(path)),
                junk_hint: normalized
                    .as_ref()
                    .is_some_and(|path| classifier.junk_hint(path)),
                path,
            }
        })
        .collect();
    let large_files = tree
        .top
        .iter()
        .map(|file| StoredLargeFile {
            folder: new_index[nearest_kept(tree, file.node, &kept_mask)],
            path: file.path.clone(),
            size_bytes: file.size_bytes,
            modified_at: file.modified.filter(|modified| modified.0 > 0),
        })
        .collect();
    let root = &tree.nodes[0];
    FolderScanResults {
        root_kind: kind,
        root_path: root_path.to_string(),
        volume,
        counters: FolderCounters {
            total_bytes: root.size_bytes,
            file_count: root.file_count,
            dir_count: root.dir_count,
            inaccessible_dirs: tree.counters.inaccessible_dirs,
            reparse_skipped: tree.counters.reparse_skipped,
            cloud_files: tree.counters.cloud_files,
            opaque_dirs: tree.counters.opaque_dirs,
            folders_stored: folders.len() as u64,
        },
        folders_truncated: selection.truncated,
        folders,
        large_files,
        issues: tree.issues.clone(),
    }
}

#[cfg(test)]
#[path = "scanner_tests.rs"]
mod tests;

//! Orquestacion del escaneo de duplicados (design D-04, D-05, D-07, D-20): raices, recorrido,
//! elegibilidad, tres etapas de hash, progreso por conteo y cancelacion.
//!
//! Solo lee: nunca mueve, renombra ni borra. Nunca abre un archivo que la proteccion no haya
//! declarado `allowed`. Un unico hilo, un archivo a la vez.

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::domain::duplicates::hash::{
    EDGE_SIZE, Expected, HashFailure, WHOLE_FILE_LIMIT, hash_full, hash_partial, open_verified,
    to_hex,
};
use crate::domain::duplicates::model::{
    DuplicateFile, DuplicateGroup, DuplicateScanOutcome, DuplicateScanRequest,
    DuplicateScanResults, RootResult, ScanCounters,
};
use crate::domain::duplicates::roots::{RootPlan, owning_root, resolve_roots};
use crate::domain::duplicates::walk::walk_content_root;
use crate::domain::junk::walk::{WalkEnd, WalkVisitor};
use crate::domain::model::EpochMillis;
use crate::domain::ports::{ContentReader, DirEntry, EntryKind, FileIdentity, FileSystem};
use crate::domain::protection::context::{ContentFolder, ContextError, ProtectionContext};
use crate::domain::protection::decision::{ProtectedReason, ProtectionDecision};
use crate::domain::protection::path::{NormalizedPath, normalize_lexical};
use crate::domain::protection::service::{CheckSession, PathProtection};

/// Cada cuantos archivos vistos se avisa el recorrido (D-20): por conteo, sin reloj.
pub const PROGRESS_EVERY_FILES: u64 = 256;
/// Cada cuantos candidatos con hash parcial se avisa.
pub const PROGRESS_EVERY_PARTIAL: u64 = 64;
/// Cada cuantos bytes del hash completo se avisa (16 MiB).
pub const PROGRESS_EVERY_BYTES: u64 = 16 << 20;

/// Receptor del avance de un escaneo de duplicados.
pub trait DuplicateScanObserver {
    /// Archivos vistos hasta ahora (cada 256 y una vez al terminar el recorrido).
    fn walking(&mut self, files_seen: u64);
    /// Etapa 2: candidatos procesados y total (al empezar, cada 64 y al terminar).
    fn partial(&mut self, done: u64, total: u64);
    /// Etapa 3: bytes hasheados y total (al empezar, cada 16 MiB y al terminar).
    fn full(&mut self, done_bytes: u64, total_bytes: u64);
}

/// Clave de agrupacion de las etapas 2 y 3: tamano y hash en hex.
type HashKey = (u64, String);
/// Candidato con su hash completo si ya se conoce (archivos chicos).
type Hashed = (Candidate, Option<blake3::Hash>);

/// Un archivo elegible, listo para hashear.
struct Candidate {
    path: NormalizedPath,
    root: ContentFolder,
    size_bytes: u64,
    modified: Option<EpochMillis>,
    identity: FileIdentity,
}

impl Candidate {
    fn expected(&self) -> Expected {
        Expected {
            identity: self.identity,
            size_bytes: self.size_bytes,
            modified: self.modified,
        }
    }

    fn fs_path(&self) -> PathBuf {
        self.path.to_path_buf()
    }

    fn sort_key(&self) -> String {
        self.path.to_string().to_uppercase()
    }
}

/// Escanea las carpetas de contenido pedidas y devuelve los grupos de duplicados.
///
/// `protection` es el contexto ya restringido por privilegios (o el error si no pudo
/// construirse: ninguna carpeta queda disponible y no se lee nada).
pub fn scan_duplicates<F: FileSystem, R: ContentReader, O: DuplicateScanObserver>(
    request: &DuplicateScanRequest,
    protection: Result<ProtectionContext, ContextError>,
    fs: &F,
    reader: &R,
    cancel: &AtomicBool,
    observer: &mut O,
) -> DuplicateScanOutcome {
    let plans = resolve_roots(fs, &protection, request.roots());
    let service = PathProtection::new(fs, protection);
    let session = service.session();
    let min_size = request.min_size().bytes();

    let mut collector = Collector {
        session: &session,
        plans: &plans,
        min_size,
        cancel,
        observer,
        counters: ScanCounters::default(),
        root_files: BTreeMap::new(),
        current_root: ContentFolder::Downloads,
        candidates: Vec::new(),
        identities: HashSet::new(),
    };
    for plan in plans.iter().filter(|plan| plan.is_walked()) {
        let Some(root) = &plan.path else { continue };
        collector.current_root = plan.folder;
        let report = walk_content_root(fs, &root.to_path_buf(), &mut collector);
        collector.counters.inaccessible_dirs += report.inaccessible;
        if report.end == WalkEnd::Cancelled {
            return DuplicateScanOutcome::Cancelled;
        }
    }
    collector.observer.walking(collector.counters.files_seen);
    let Collector {
        observer,
        mut counters,
        root_files,
        candidates,
        ..
    } = collector;
    if cancelled(cancel) {
        return DuplicateScanOutcome::Cancelled;
    }

    let Some(groups) = detect(candidates, reader, cancel, observer, &mut counters) else {
        return DuplicateScanOutcome::Cancelled;
    };
    let roots = plans
        .iter()
        .map(|plan| RootResult {
            folder: plan.folder,
            status: plan.status,
            files_seen: root_files.get(&plan.folder).copied().unwrap_or(0),
        })
        .collect();
    DuplicateScanOutcome::Completed(DuplicateScanResults {
        min_size: request.min_size(),
        roots,
        counters,
        groups,
    })
}

fn cancelled(cancel: &AtomicBool) -> bool {
    // Solo es una bandera: no ordena otros accesos a memoria.
    cancel.load(Ordering::Relaxed)
}

/// Fase de recorrido: cuenta, filtra y verifica cada archivo con la proteccion de rutas (D-04).
struct Collector<'a, F: FileSystem, O: DuplicateScanObserver> {
    session: &'a CheckSession<'a, 'a, F>,
    plans: &'a [RootPlan],
    min_size: u64,
    cancel: &'a AtomicBool,
    observer: &'a mut O,
    counters: ScanCounters,
    root_files: BTreeMap<ContentFolder, u64>,
    current_root: ContentFolder,
    candidates: Vec<Candidate>,
    /// Una identidad aparece una sola vez (no deberia repetirse sin hard links, pero es barato).
    identities: HashSet<FileIdentity>,
}

impl<F: FileSystem, O: DuplicateScanObserver> WalkVisitor for Collector<'_, F, O> {
    fn is_cancelled(&self) -> bool {
        cancelled(self.cancel)
    }

    fn file_seen(&mut self) {}

    fn candidate(&mut self, entry: DirEntry) {
        self.counters.files_seen += 1;
        let owner = normalize_lexical(&entry.path.to_string_lossy())
            .ok()
            .and_then(|path| owning_root(self.plans, &path))
            .unwrap_or(self.current_root);
        *self.root_files.entry(owner).or_insert(0) += 1;
        if self
            .counters
            .files_seen
            .is_multiple_of(PROGRESS_EVERY_FILES)
        {
            self.observer.walking(self.counters.files_seen);
        }
        // 1. El listado ya dice que es chico: ni se consulta la proteccion.
        if entry.meta.size_bytes < self.min_size {
            self.counters.below_min_size += 1;
            return;
        }
        match self.session.check(&entry.path) {
            ProtectionDecision::Allowed(checked) => {
                if checked.kind != EntryKind::File {
                    self.counters.protected_count += 1;
                } else if checked.size_bytes < self.min_size {
                    // 4. El tamano por handle manda: el listado puede ir atrasado.
                    self.counters.below_min_size += 1;
                } else if self.identities.insert(checked.identity) {
                    self.candidates.push(Candidate {
                        root: owning_root(self.plans, &checked.path).unwrap_or(owner),
                        path: checked.path,
                        size_bytes: checked.size_bytes,
                        modified: checked.modified,
                        identity: checked.identity,
                    });
                }
            }
            ProtectionDecision::SkippedCloud { .. } => self.counters.cloud_count += 1,
            ProtectionDecision::Protected(ProtectedReason::MultipleHardLinks) => {
                self.counters.hard_linked_count += 1;
            }
            ProtectionDecision::Protected(_) => self.counters.protected_count += 1,
        }
    }

    fn reparse_point(&mut self, _path: PathBuf) {}

    fn excluded_dir(&mut self, _path: PathBuf) {
        self.counters.excluded_dirs += 1;
    }
}

/// Anota un fallo de hash en los contadores. Devuelve `true` si fue la cancelacion.
fn count_failure(counters: &mut ScanCounters, failure: HashFailure) -> bool {
    match failure {
        HashFailure::Cancelled => return true,
        HashFailure::Cloud => counters.cloud_count += 1,
        HashFailure::IdentityMismatch | HashFailure::Changed => counters.changed_count += 1,
        HashFailure::InUse | HashFailure::Unreadable => counters.unreadable_count += 1,
    }
    false
}

/// Etapas 1 a 3. `None` si se cancelo.
fn detect<R: ContentReader, O: DuplicateScanObserver>(
    mut candidates: Vec<Candidate>,
    reader: &R,
    cancel: &AtomicBool,
    observer: &mut O,
    counters: &mut ScanCounters,
) -> Option<Vec<DuplicateGroup>> {
    // Etapa 1: tamanos unicos fuera, sin leer nada.
    candidates.sort_by_cached_key(Candidate::sort_key);
    let mut by_size: BTreeMap<u64, Vec<Candidate>> = BTreeMap::new();
    for candidate in candidates {
        by_size
            .entry(candidate.size_bytes)
            .or_default()
            .push(candidate);
    }
    let stage2: Vec<Candidate> = by_size
        .into_values()
        .filter(|same| same.len() >= 2)
        .flatten()
        .collect();

    // Etapa 2: hash parcial.
    let total = stage2.len() as u64;
    observer.partial(0, total);
    let mut by_partial: BTreeMap<HashKey, Vec<Hashed>> = BTreeMap::new();
    let mut last_reported = 0;
    for (index, candidate) in stage2.into_iter().enumerate() {
        if cancelled(cancel) {
            return None;
        }
        let outcome = open_verified(reader, &candidate.fs_path(), &candidate.expected())
            .and_then(|mut handle| hash_partial(handle.as_mut(), &candidate.expected(), cancel));
        match outcome {
            Ok(hashes) => {
                counters.hashed_bytes += if candidate.size_bytes <= WHOLE_FILE_LIMIT {
                    candidate.size_bytes
                } else {
                    2 * EDGE_SIZE as u64
                };
                by_partial
                    .entry((candidate.size_bytes, to_hex(&hashes.partial)))
                    .or_default()
                    .push((candidate, hashes.full));
            }
            Err(failure) => {
                if count_failure(counters, failure) {
                    return None;
                }
            }
        }
        let done = index as u64 + 1;
        if done.is_multiple_of(PROGRESS_EVERY_PARTIAL) {
            observer.partial(done, total);
            last_reported = done;
        }
    }
    if last_reported != total {
        observer.partial(total, total);
    }

    // Etapa 3: hash completo de lo que sigue empatando.
    let contenders: Vec<Vec<Hashed>> = by_partial
        .into_values()
        .filter(|same| same.len() >= 2)
        .collect();
    let total_bytes: u64 = contenders
        .iter()
        .flatten()
        .filter(|(_, full)| full.is_none())
        .map(|(candidate, _)| candidate.size_bytes)
        .sum();
    observer.full(0, total_bytes);
    let mut done_bytes = 0u64;
    let mut last_reported = 0u64;
    let mut by_full: BTreeMap<HashKey, Vec<Candidate>> = BTreeMap::new();
    for same in contenders {
        for (candidate, known) in same {
            let hash = match known {
                Some(hash) => hash,
                None => {
                    if cancelled(cancel) {
                        return None;
                    }
                    let outcome =
                        open_verified(reader, &candidate.fs_path(), &candidate.expected())
                            .and_then(|mut handle| {
                                hash_full(handle.as_mut(), &candidate.expected(), cancel, |bytes| {
                                    done_bytes += bytes;
                                    counters.hashed_bytes += bytes;
                                    if done_bytes - last_reported >= PROGRESS_EVERY_BYTES {
                                        observer.full(done_bytes, total_bytes);
                                        last_reported = done_bytes;
                                    }
                                })
                            });
                    match outcome {
                        Ok(hash) => hash,
                        Err(failure) => {
                            if count_failure(counters, failure) {
                                return None;
                            }
                            continue;
                        }
                    }
                }
            };
            by_full
                .entry((candidate.size_bytes, to_hex(&hash)))
                .or_default()
                .push(candidate);
        }
    }
    if last_reported != done_bytes {
        observer.full(done_bytes, total_bytes);
    }

    let mut groups: Vec<DuplicateGroup> = by_full
        .into_iter()
        .filter_map(|((size_bytes, blake3), mut same)| {
            same.sort_by_cached_key(Candidate::sort_key);
            let mut seen = HashSet::new();
            same.retain(|candidate| seen.insert(candidate.identity));
            (same.len() >= 2).then(|| DuplicateGroup {
                size_bytes,
                blake3,
                files: same
                    .into_iter()
                    .map(|candidate| DuplicateFile {
                        root: candidate.root,
                        path: candidate.path.to_string(),
                        modified: candidate.modified,
                        identity: candidate.identity,
                    })
                    .collect(),
            })
        })
        .collect();
    groups.sort_by(|a, b| {
        b.size_bytes
            .cmp(&a.size_bytes)
            .then_with(|| a.blake3.cmp(&b.blake3))
    });
    Some(groups)
}

#[cfg(test)]
#[path = "scanner_tests.rs"]
mod tests;

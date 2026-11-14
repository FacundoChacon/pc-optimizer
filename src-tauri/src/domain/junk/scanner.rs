//! Orquestacion del escaneo de basura (D-08, D-10, D-11, D-13, D-14): alcance por privilegios,
//! recorrido, verificacion de cada candidato, Papelera, resumen, progreso y cancelacion.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::domain::junk::category::{CategoryScope, JunkCategory};
use crate::domain::junk::model::{
    CategoryResult, CategoryStatus, FindingState, JunkFinding, JunkScanOutcome, JunkScanResults,
};
use crate::domain::junk::sources::{JunkSource, sources};
use crate::domain::junk::verify::{reparse_point_finding, verify_candidate};
use crate::domain::junk::walk::{WalkEnd, WalkVisitor, walk_source};
use crate::domain::model::EpochMillis;
use crate::domain::ports::{DirEntry, FileSystem, RecycleBinInfo};
use crate::domain::privileges::{
    MismatchReason, PrivilegeContext, ScopeAccess, UserScope, scope_access,
};
use crate::domain::protection::context::{ContextError, ProtectionContext};
use crate::domain::protection::service::{CheckSession, PathProtection};

/// Cada cuantos archivos vistos se avisa el progreso (D-13): por conteo, sin reloj.
pub const PROGRESS_EVERY_FILES: u64 = 256;

/// Receptor del avance de un escaneo.
pub trait ScanObserver {
    /// Empieza una categoria (se avisa por cada una, en el orden de `JunkCategory::ALL`).
    fn category(&mut self, category: JunkCategory);
    /// Avance dentro de una categoria: archivos vistos y bytes elegibles acumulados del escaneo.
    fn progress(&mut self, category: JunkCategory, files_seen: u64, eligible_bytes: u64);
}

/// Escanea todas las categorias. `privileges` decide el bloqueo por-usuario y `protection` es el
/// contexto ya restringido por privilegios (o el error si no pudo construirse).
///
/// `system_drive` es la unidad del sistema resuelta por quien llama (solo para MEDIR la
/// Papelera, que no depende del contexto de proteccion). Si es `None`, la Papelera queda
/// `Unavailable`: nunca se adivina una letra.
///
/// Solo lee: ni mueve, ni borra, ni vacia la Papelera. Devuelve `Cancelled` en cuanto `cancel`
/// se levanta (se consulta antes de cada categoria, carpeta y archivo).
#[expect(
    clippy::too_many_arguments,
    reason = "cada argumento es un puerto o una entrada independiente del escaneo; agruparlos solo agregaria una estructura sin significado"
)]
pub fn scan_junk<F: FileSystem, R: RecycleBinInfo, O: ScanObserver>(
    privileges: &PrivilegeContext,
    protection: Result<ProtectionContext, ContextError>,
    system_drive: Option<char>,
    fs: &F,
    recycle_bin: &R,
    now: EpochMillis,
    cancel: &AtomicBool,
    observer: &mut O,
) -> JunkScanOutcome {
    let locations = protection.as_ref().map(sources).unwrap_or_default();
    let usable_context = protection.is_ok();
    let service = PathProtection::new(fs, protection);
    let session = service.session();
    let mut run = Run {
        session: &session,
        fs,
        cancel,
        observer,
        findings: Vec::new(),
        known_paths: HashSet::new(),
        files_seen: 0,
        eligible_bytes: 0,
    };
    let blocked = match scope_access(privileges, UserScope::PerUser) {
        ScopeAccess::Allowed => None,
        ScopeAccess::Blocked(reason) => Some(reason),
    };
    let mut categories = Vec::with_capacity(JunkCategory::ALL.len());
    for category in JunkCategory::ALL {
        if is_cancelled(cancel) {
            return JunkScanOutcome::Cancelled;
        }
        run.observer.category(category);
        let result = if category == JunkCategory::RecycleBin {
            measure_recycle_bin(recycle_bin, system_drive, blocked)
        } else if usable_context {
            match run.scan_category(category, &locations, blocked, now) {
                Some(result) => result,
                None => return JunkScanOutcome::Cancelled,
            }
        } else {
            CategoryResult::empty(category, CategoryStatus::Unavailable, None)
        };
        categories.push(result);
    }
    JunkScanOutcome::Completed(JunkScanResults {
        findings: run.findings,
        categories,
    })
}

fn is_cancelled(cancel: &AtomicBool) -> bool {
    // Solo es una bandera: no ordena otros accesos a memoria.
    cancel.load(Ordering::Relaxed)
}

/// La Papelera se mide, nunca se lista: un fallo la deja `Unavailable`.
fn measure_recycle_bin<R: RecycleBinInfo>(
    recycle_bin: &R,
    system_drive: Option<char>,
    blocked: Option<MismatchReason>,
) -> CategoryResult {
    let category = JunkCategory::RecycleBin;
    if let Some(reason) = blocked {
        return CategoryResult::empty(category, CategoryStatus::Blocked, Some(reason));
    }
    let Some(drive) = system_drive else {
        return CategoryResult::empty(category, CategoryStatus::Unavailable, None);
    };
    let root = PathBuf::from(format!("{drive}:\\"));
    match recycle_bin.query(&root) {
        Ok(usage) => CategoryResult {
            item_count: usage.items,
            size_bytes: usage.bytes,
            ..CategoryResult::empty(category, CategoryStatus::Done, None)
        },
        Err(_) => CategoryResult::empty(category, CategoryStatus::Unavailable, None),
    }
}

/// Estado mutable de un escaneo en curso.
struct Run<'a, F: FileSystem, O: ScanObserver> {
    session: &'a CheckSession<'a, 'a, F>,
    fs: &'a F,
    cancel: &'a AtomicBool,
    observer: &'a mut O,
    findings: Vec<JunkFinding>,
    /// Rutas ya registradas (sin distinguir mayusculas): una ruta es unica por escaneo aunque
    /// dos categorias la alcancen (p. ej. `TEMP` redirigido a `C:\Windows\Temp`).
    known_paths: HashSet<String>,
    files_seen: u64,
    eligible_bytes: u64,
}

impl<F: FileSystem, O: ScanObserver> Run<'_, F, O> {
    /// `None` si el escaneo se cancelo durante la categoria.
    fn scan_category(
        &mut self,
        category: JunkCategory,
        locations: &[JunkSource],
        blocked: Option<MismatchReason>,
        now: EpochMillis,
    ) -> Option<CategoryResult> {
        let scope = category.scope();
        if let (CategoryScope::PerUser, Some(reason)) = (scope, blocked) {
            return Some(CategoryResult::empty(
                category,
                CategoryStatus::Blocked,
                Some(reason),
            ));
        }
        // Primera barrera: una ubicacion por-usuario bloqueada nunca se recorre, aunque el
        // contexto de proteccion todavia trajera al usuario.
        let usable: Vec<&JunkSource> = locations
            .iter()
            .filter(|source| source.category == category)
            .filter(|source| blocked.is_none() || source.scope == UserScope::Machine)
            .collect();
        let (status, block_reason) = match (usable.is_empty(), scope, blocked) {
            (true, _, _) => (CategoryStatus::Unavailable, None),
            (false, CategoryScope::Mixed, Some(reason)) => (CategoryStatus::Partial, Some(reason)),
            _ => (CategoryStatus::Done, None),
        };
        let mut totals = Totals::default();
        for source in usable {
            let fs = self.fs;
            let mut visitor = CategoryVisitor {
                run: &mut *self,
                category,
                totals: &mut totals,
            };
            let report = walk_source(fs, source, now, &mut visitor);
            totals.inaccessible += report.inaccessible;
            if report.end == WalkEnd::Cancelled {
                return None;
            }
        }
        Some(CategoryResult {
            category,
            scope,
            status,
            block_reason,
            item_count: totals.items,
            size_bytes: totals.bytes,
            skipped_count: totals.skipped,
            inaccessible_count: totals.inaccessible,
        })
    }

    fn record(&mut self, finding: JunkFinding, totals: &mut Totals) {
        if !self.known_paths.insert(finding.path.to_lowercase()) {
            return;
        }
        match finding.state {
            FindingState::Eligible(_) => {
                totals.items += 1;
                totals.bytes += finding.size_bytes;
                self.eligible_bytes += finding.size_bytes;
            }
            FindingState::Skipped(_) => totals.skipped += 1,
        }
        self.findings.push(finding);
    }
}

#[derive(Default)]
struct Totals {
    items: u64,
    bytes: u64,
    skipped: u64,
    inaccessible: u64,
}

struct CategoryVisitor<'r, 'a, F: FileSystem, O: ScanObserver> {
    run: &'r mut Run<'a, F, O>,
    category: JunkCategory,
    totals: &'r mut Totals,
}

impl<F: FileSystem, O: ScanObserver> WalkVisitor for CategoryVisitor<'_, '_, F, O> {
    fn is_cancelled(&self) -> bool {
        is_cancelled(self.run.cancel)
    }

    fn file_seen(&mut self) {
        self.run.files_seen += 1;
        if self.run.files_seen.is_multiple_of(PROGRESS_EVERY_FILES) {
            self.run
                .observer
                .progress(self.category, self.run.files_seen, self.run.eligible_bytes);
        }
    }

    fn candidate(&mut self, entry: DirEntry) {
        let finding = verify_candidate(self.run.session, self.category, &entry);
        self.run.record(finding, self.totals);
    }

    fn reparse_point(&mut self, path: PathBuf) {
        let finding = reparse_point_finding(self.category, &path);
        self.run.record(finding, self.totals);
    }
}

#[cfg(test)]
#[path = "scanner_tests.rs"]
mod tests;

//! Servicio de proteccion: reglas puras + resolucion contra el puerto `FileSystem`
//! (design D-01, D-02, D-03, D-08, D-09).
//!
//! El servicio responde con `ProtectionDecision`, nunca con `Result`: todo error (parseo,
//! contexto, puerto, identidad, discrepancia) termina en `Protected`, y el unico camino que
//! produce `Allowed` es el final de la resolucion completa ("lo que ves es lo que se borra").
//!
//! # Contrato para el consumidor del borrado (C-09)
//!
//! La re-verificacion anti-TOCTOU no es opcional. Para cada elemento a mover a la Papelera:
//!
//! 1. llamar a [`PathProtection::prepare_delete`] inmediatamente antes de CADA `move_to_trash`
//!    (nunca confiar en el resultado de un escaneo previo);
//! 2. mover `target.path()` (la ruta real verificada), jamas la ruta recibida del frontend ni la
//!    del escaneo;
//! 3. mantener vivo el [`DeleteTarget`] hasta que `move_to_trash` retorne y soltarlo despues:
//!    mientras vive, los ancestros estan fijados y nadie puede renombrarlos ni reemplazarlos;
//! 4. garantizar que el adaptador `Trash` no siga el ultimo componente (si el objetivo se
//!    reemplazara por un enlace en la ventana residual, se mueve el enlace y no su destino) y
//!    verificarlo con un test real;
//! 5. traducir con [`ProtectionDecision::skip_outcome`]: `Protected` -> `skipped_protected`,
//!    `SkippedCloud` -> `skipped_cloud`.
//!
//! Un `Allowed` sobre un directorio certifica solo esa entrada: cada descendiente necesita su
//! propia decision.

use std::cell::RefCell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::domain::ports::{AncestorPin, EntryKind, FileMeta, FileSystem, PortError};

use super::context::{ContextError, ProtectionContext};
use super::decision::{
    AllowRule, AnchorClass, CheckedPath, PathRejection, ProtectedReason, ProtectionDecision,
    ResolveFailure, RuleVerdict,
};
use super::path::{NormalizedPath, looks_short, normalize_lexical, same_shape};
use super::rules::evaluate_rules;

/// Por que se detuvo la resolucion antes de certificar la ruta.
#[derive(Debug)]
enum Stop {
    Cloud(NormalizedPath),
    Protected(ProtectedReason),
}

impl From<Stop> for ProtectionDecision {
    fn from(stop: Stop) -> Self {
        match stop {
            Stop::Cloud(path) => ProtectionDecision::SkippedCloud { path },
            Stop::Protected(reason) => ProtectionDecision::Protected(reason),
        }
    }
}

/// Para quien se resuelve la ruta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Un elemento a limpiar.
    Item,
    /// La raiz de una regla de limpieza: las anclas de contenido son validas y no se exigen
    /// hard links.
    RuleRoot,
}

type Memo = RefCell<HashSet<NormalizedPath>>;

fn deny(reason: ProtectedReason) -> Stop {
    Stop::Protected(reason)
}

fn unresolvable(error: &PortError) -> Stop {
    deny(ProtectedReason::Unresolvable(ResolveFailure::from(error)))
}

/// Servicio de proteccion de rutas sobre un puerto de archivos y un contexto resuelto.
pub struct PathProtection<'a, F: FileSystem> {
    fs: &'a F,
    ctx: Result<ProtectionContext, ContextError>,
}

/// Resultado de `prepare_delete`: la ruta real verificada mas los ancestros fijados.
///
/// Soltarlo libera la fijacion; debe vivir hasta que el movimiento a la Papelera retorne.
pub struct DeleteTarget {
    path: PathBuf,
    checked: CheckedPath,
    _pin: Box<dyn AncestorPin>,
}

impl DeleteTarget {
    /// Ruta real verificada: la unica que debe entregarse a `move_to_trash`.
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn checked(&self) -> &CheckedPath {
        &self.checked
    }
}

/// Memo de ancestros ya verificados para escaneos masivos; vive lo que dura un escaneo.
///
/// Solo `check` lo usa: `reverify` y `prepare_delete` nunca consultan memos.
pub struct CheckSession<'s, 'a, F: FileSystem> {
    service: &'s PathProtection<'a, F>,
    verified: Memo,
}

impl<F: FileSystem> CheckSession<'_, '_, F> {
    pub fn check(&self, raw: &Path) -> ProtectionDecision {
        into_decision(self.service.resolve(raw, Some(&self.verified), Mode::Item))
    }
}

fn into_decision(result: Result<CheckedPath, Stop>) -> ProtectionDecision {
    match result {
        Ok(checked) => ProtectionDecision::Allowed(checked),
        Err(stop) => stop.into(),
    }
}

impl<'a, F: FileSystem> PathProtection<'a, F> {
    /// `ctx` es `Err` si el contexto no pudo resolverse: todo queda protegido.
    pub fn new(fs: &'a F, ctx: Result<ProtectionContext, ContextError>) -> Self {
        Self { fs, ctx }
    }

    /// Decide si `raw` es elegible.
    pub fn check(&self, raw: &Path) -> ProtectionDecision {
        into_decision(self.resolve(raw, None, Mode::Item))
    }

    /// Sesion con memo de ancestros para escaneos.
    pub fn session(&self) -> CheckSession<'_, 'a, F> {
        CheckSession {
            service: self,
            verified: RefCell::new(HashSet::new()),
        }
    }

    /// Repite la verificacion completa (sin memo, sin fijar) y compara identidad y tipo con los
    /// del escaneo.
    pub fn reverify(&self, scanned: &CheckedPath) -> ProtectionDecision {
        match self.check(&scanned.path.to_path_buf()) {
            ProtectionDecision::Allowed(current) => {
                if current.identity != scanned.identity || current.kind != scanned.kind {
                    ProtectionDecision::Protected(ProtectedReason::IdentityChanged)
                } else {
                    ProtectionDecision::Allowed(current)
                }
            }
            other => other,
        }
    }

    /// Fija los ancestros, re-verifica y entrega la ruta real a mover.
    ///
    /// El `Err` nunca es `Allowed`; la fijacion se toma ANTES de re-verificar y se libera si
    /// la re-verificacion falla.
    pub fn prepare_delete(
        &self,
        scanned: &CheckedPath,
    ) -> Result<DeleteTarget, ProtectionDecision> {
        let pin = self
            .fs
            .pin_ancestors(&scanned.path.to_path_buf())
            .map_err(|_| {
                ProtectionDecision::Protected(ProtectedReason::Unresolvable(
                    ResolveFailure::PinFailed,
                ))
            })?;
        match self.reverify(scanned) {
            ProtectionDecision::Allowed(checked) => Ok(DeleteTarget {
                path: checked.path.to_path_buf(),
                checked,
                _pin: pin,
            }),
            other => Err(other),
        }
    }

    /// Valida la raiz de una regla de limpieza (D-09): la ruta debe existir, no atravesar ni ser
    /// un reparse point y no estar protegida salvo por ser un ancla de contenido (Descargas,
    /// temporal, raiz de excepcion). Cada elemento que la regla encuentre se decide aparte.
    pub fn check_rule_root(&self, raw: &Path) -> Result<NormalizedPath, ProtectedReason> {
        match self.resolve(raw, None, Mode::RuleRoot) {
            Ok(checked) => Ok(checked.path),
            // Una carpeta de nube puede ser raiz: sus elementos se omiten uno a uno.
            Err(Stop::Cloud(path)) => Ok(path),
            Err(Stop::Protected(reason)) => Err(reason),
        }
    }

    fn metadata(&self, path: &NormalizedPath) -> Result<FileMeta, Stop> {
        self.fs
            .metadata(&path.to_path_buf())
            .map_err(|error| unresolvable(&error))
    }

    /// Veredicto de las reglas; en `RuleRoot` las anclas de contenido no son fatales.
    fn rule_for(
        ctx: &ProtectionContext,
        path: &NormalizedPath,
        mode: Mode,
    ) -> Result<RuleVerdict, ProtectedReason> {
        match evaluate_rules(ctx, path) {
            RuleVerdict::Protected(ProtectedReason::Anchor(kind))
                if mode == Mode::RuleRoot && kind.class() == AnchorClass::Content =>
            {
                Ok(RuleVerdict::Allowed(AllowRule::Default))
            }
            RuleVerdict::Protected(reason) => Err(reason),
            allowed @ RuleVerdict::Allowed(_) => Ok(allowed),
        }
    }

    /// Algoritmo de D-02.
    fn resolve(&self, raw: &Path, memo: Option<&Memo>, mode: Mode) -> Result<CheckedPath, Stop> {
        let text = raw.to_str().ok_or(deny(ProtectedReason::Unparseable(
            PathRejection::NonUnicode,
        )))?;
        let lexical = normalize_lexical(text).map_err(|r| deny(ProtectedReason::Unparseable(r)))?;
        let ctx = self
            .ctx
            .as_ref()
            .map_err(|_| deny(ProtectedReason::ContextUnavailable))?;

        // Nivel lexico (sin I/O). Un alias 8.3 puede ocultar el perfil propio: en ese unico caso
        // la ruta real (paso 7) decide, porque el lexico seria un falso positivo.
        if let Err(reason) = Self::rule_for(ctx, &lexical, mode) {
            let may_be_own_profile_alias = reason == ProtectedReason::OtherUserProfile
                && lexical
                    .components()
                    .iter()
                    .any(|c| looks_short(c.original()));
            if !may_be_own_profile_alias {
                return Err(deny(reason));
            }
        }

        self.check_ancestors(&lexical, memo)?;
        let target = self.metadata(&lexical)?;
        let real = self.resolve_real(&lexical)?;
        let rule = match Self::rule_for(ctx, &real, mode).map_err(deny)? {
            RuleVerdict::Allowed(rule) => rule,
            RuleVerdict::Protected(reason) => return Err(deny(reason)),
        };

        if target.cloud_placeholder {
            return Err(Stop::Cloud(real));
        }
        if target.kind == EntryKind::ReparsePoint {
            return Err(deny(ProtectedReason::ReparsePoint));
        }
        if mode == Mode::Item && target.kind == EntryKind::File {
            match target.hard_links {
                Some(1) => {}
                Some(_) => return Err(deny(ProtectedReason::MultipleHardLinks)),
                None => {
                    return Err(deny(ProtectedReason::Unresolvable(
                        ResolveFailure::MissingIdentity,
                    )));
                }
            }
        }
        let identity = target.identity.ok_or(deny(ProtectedReason::Unresolvable(
            ResolveFailure::MissingIdentity,
        )))?;
        Ok(CheckedPath {
            path: real,
            kind: target.kind,
            size_bytes: target.size_bytes,
            identity,
            rule,
        })
    }

    /// Cada ancestro estricto (sin la raiz del volumen) debe ser un directorio sin reparse.
    fn check_ancestors(&self, lexical: &NormalizedPath, memo: Option<&Memo>) -> Result<(), Stop> {
        for depth in 1..lexical.len() {
            let ancestor = lexical.prefix(depth);
            if memo.is_some_and(|known| known.borrow().contains(&ancestor)) {
                continue;
            }
            let meta = self.metadata(&ancestor)?;
            if meta.cloud_placeholder {
                return Err(Stop::Cloud(lexical.clone()));
            }
            match meta.kind {
                EntryKind::Directory => {}
                EntryKind::ReparsePoint => return Err(deny(ProtectedReason::ReparsePointInPath)),
                EntryKind::File => {
                    return Err(deny(ProtectedReason::Unresolvable(
                        ResolveFailure::NotADirectory,
                    )));
                }
            }
            if let Some(known) = memo {
                known.borrow_mut().insert(ancestor);
            }
        }
        Ok(())
    }

    /// Ruta real con nombres largos; debe tener la misma forma que la lexica.
    fn resolve_real(&self, lexical: &NormalizedPath) -> Result<NormalizedPath, Stop> {
        let canonical = self
            .fs
            .canonicalize(&lexical.to_path_buf())
            .map_err(|error| unresolvable(&error))?;
        let mismatch = || deny(ProtectedReason::ResolvedPathMismatch);
        let real = canonical
            .to_str()
            .and_then(|text| normalize_lexical(text).ok())
            .ok_or_else(mismatch)?;
        if same_shape(lexical, &real) {
            Ok(real)
        } else {
            Err(mismatch())
        }
    }
}

#[cfg(test)]
#[path = "service_tests.rs"]
mod tests;

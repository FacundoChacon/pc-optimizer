//! Reglas puras de proteccion (design D-04): sin I/O, relativas a las anclas del contexto.
//!
//! Orden de evaluacion (la primera coincidencia gana dentro de cada nivel):
//! 1. denegaciones duras (ninguna excepcion las anula): volumen no soportado, raiz de volumen,
//!    entrada de sistema de la raiz, ancla, perfil de otro usuario;
//! 2. excepciones de la lista cerrada (subarboles estrictos);
//! 3. raices protegidas: Windows, Program Files, ProgramData, perfiles de navegador, app;
//! 4. el resto es elegible por defecto.
//!
//! Esta capa solo VETA: que se limpia lo decide la whitelist de C-08. Ampliar la lista de
//! excepciones exige un delta spec aprobado por el dueno (RN-PROT-06).

use super::context::ProtectionContext;
use super::decision::{AllowRule, AnchorKind, Browser, ExceptionId, ProtectedReason, RuleVerdict};
use super::path::{Component, NormalizedPath, fold_component};

/// Segmento de un patron relativo a una base: literal o "exactamente un componente cualquiera".
#[derive(Clone, Copy)]
enum Seg {
    Lit(&'static str),
    Any,
}

macro_rules! seg {
    (*) => {
        Seg::Any
    };
    ($lit:literal) => {
        Seg::Lit($lit)
    };
}

macro_rules! pattern {
    ($($segment:tt),+ $(,)?) => {
        &[$(seg!($segment)),+]
    };
}

/// Ancla del contexto respecto de la cual se escribe un patron.
#[derive(Clone, Copy)]
enum Base {
    Windows,
    ProgramData,
    LocalAppData,
    RoamingAppData,
}

/// Excepcion de la lista cerrada: `pattern` bajo `base`, valida solo si la unica raiz protegida
/// que cubre la ruta es `parent`.
struct ExceptionDef {
    id: ExceptionId,
    base: Base,
    pattern: &'static [Seg],
    parent: ProtectedReason,
}

macro_rules! chromium_cache {
    ($browser:expr, $vendor:literal, $product:literal, $dir:literal) => {
        ExceptionDef {
            id: ExceptionId::BrowserCache($browser),
            base: Base::LocalAppData,
            pattern: pattern![$vendor, $product, "User Data", *, $dir],
            parent: ProtectedReason::BrowserProfile($browser),
        }
    };
}

/// LISTA CERRADA de excepciones (RN-PROT-06). `*` = exactamente un componente.
const EXCEPTIONS: &[ExceptionDef] = &[
    ExceptionDef {
        id: ExceptionId::WindowsTemp,
        base: Base::Windows,
        pattern: pattern!["Temp"],
        parent: ProtectedReason::WindowsDir,
    },
    ExceptionDef {
        id: ExceptionId::WindowsUpdateDownload,
        base: Base::Windows,
        pattern: pattern!["SoftwareDistribution", "Download"],
        parent: ProtectedReason::WindowsDir,
    },
    ExceptionDef {
        id: ExceptionId::Minidump,
        base: Base::Windows,
        pattern: pattern!["Minidump"],
        parent: ProtectedReason::WindowsDir,
    },
    ExceptionDef {
        id: ExceptionId::WerReportArchive,
        base: Base::ProgramData,
        pattern: pattern!["Microsoft", "Windows", "WER", "ReportArchive"],
        parent: ProtectedReason::ProgramData,
    },
    ExceptionDef {
        id: ExceptionId::WerReportQueue,
        base: Base::ProgramData,
        pattern: pattern!["Microsoft", "Windows", "WER", "ReportQueue"],
        parent: ProtectedReason::ProgramData,
    },
    chromium_cache!(Browser::Chrome, "Google", "Chrome", "Cache"),
    chromium_cache!(Browser::Chrome, "Google", "Chrome", "Code Cache"),
    chromium_cache!(Browser::Chrome, "Google", "Chrome", "GPUCache"),
    chromium_cache!(Browser::Edge, "Microsoft", "Edge", "Cache"),
    chromium_cache!(Browser::Edge, "Microsoft", "Edge", "Code Cache"),
    chromium_cache!(Browser::Edge, "Microsoft", "Edge", "GPUCache"),
    chromium_cache!(Browser::Brave, "BraveSoftware", "Brave-Browser", "Cache"),
    chromium_cache!(
        Browser::Brave,
        "BraveSoftware",
        "Brave-Browser",
        "Code Cache"
    ),
    chromium_cache!(Browser::Brave, "BraveSoftware", "Brave-Browser", "GPUCache"),
    ExceptionDef {
        id: ExceptionId::BrowserCache(Browser::Firefox),
        base: Base::LocalAppData,
        pattern: pattern!["Mozilla", "Firefox", "Profiles", *, "cache2"],
        parent: ProtectedReason::BrowserProfile(Browser::Firefox),
    },
];

/// Raiz de perfil de navegador (todo su contenido esta protegido salvo excepciones).
struct BrowserRootDef {
    browser: Browser,
    base: Base,
    segments: &'static [&'static str],
}

const BROWSER_ROOTS: &[BrowserRootDef] = &[
    BrowserRootDef {
        browser: Browser::Chrome,
        base: Base::LocalAppData,
        segments: &["Google", "Chrome", "User Data"],
    },
    BrowserRootDef {
        browser: Browser::Edge,
        base: Base::LocalAppData,
        segments: &["Microsoft", "Edge", "User Data"],
    },
    BrowserRootDef {
        browser: Browser::Brave,
        base: Base::LocalAppData,
        segments: &["BraveSoftware", "Brave-Browser", "User Data"],
    },
    BrowserRootDef {
        browser: Browser::Firefox,
        base: Base::RoamingAppData,
        segments: &["Mozilla", "Firefox", "Profiles"],
    },
    BrowserRootDef {
        browser: Browser::Firefox,
        base: Base::LocalAppData,
        segments: &["Mozilla", "Firefox", "Profiles"],
    },
];

/// Entradas de la raiz de un volumen que son del sistema (ademas de todo lo que empiece con `$`).
const VOLUME_SYSTEM_ENTRIES: &[&str] = &[
    "System Volume Information",
    "pagefile.sys",
    "hiberfil.sys",
    "swapfile.sys",
    "DumpStack.log",
    "DumpStack.log.tmp",
    "Recovery",
    "Boot",
    "bootmgr",
    "BOOTNXT",
    "Config.Msi",
    "Documents and Settings",
];

/// Resultado de comparar una ruta con un patron bajo una base.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Overlap {
    /// La ruta es exactamente la raiz del patron.
    Exact,
    /// La ruta esta dentro de la raiz del patron.
    Below,
    /// La ruta es la base o un prefijo propio del patron (todavia no llego a la raiz).
    Partial,
}

fn base_path(ctx: &ProtectionContext, base: Base) -> Option<&NormalizedPath> {
    match base {
        Base::Windows => Some(ctx.windows_dir()),
        Base::ProgramData => Some(ctx.program_data()),
        Base::LocalAppData => ctx.user().map(|user| user.local_app_data()),
        Base::RoamingAppData => ctx.user().map(|user| user.roaming_app_data()),
    }
}

fn match_pattern(base: &NormalizedPath, pattern: &[Seg], path: &NormalizedPath) -> Option<Overlap> {
    let rest: &[Component] = path.strip_prefix(base)?;
    let compared = rest.len().min(pattern.len());
    for (component, segment) in rest.iter().zip(pattern).take(compared) {
        if let Seg::Lit(literal) = segment
            && component.folded() != fold_component(literal)
        {
            return None;
        }
    }
    Some(match rest.len().cmp(&pattern.len()) {
        std::cmp::Ordering::Equal => Overlap::Exact,
        std::cmp::Ordering::Greater => Overlap::Below,
        std::cmp::Ordering::Less => Overlap::Partial,
    })
}

fn overlap_with(
    ctx: &ProtectionContext,
    def: &ExceptionDef,
    path: &NormalizedPath,
) -> Option<Overlap> {
    match_pattern(base_path(ctx, def.base)?, def.pattern, path)
}

fn browser_root(ctx: &ProtectionContext, def: &BrowserRootDef) -> Option<NormalizedPath> {
    base_path(ctx, def.base).map(|base| base.joined(def.segments))
}

/// Anclas del contexto en orden de prioridad.
fn anchors(ctx: &ProtectionContext) -> Vec<(&NormalizedPath, AnchorKind)> {
    let mut list = vec![(ctx.users_root(), AnchorKind::UsersRoot)];
    if let Some(user) = ctx.user() {
        list.push((user.profile(), AnchorKind::UserProfile));
        list.push((user.local_app_data(), AnchorKind::LocalAppData));
        list.push((user.roaming_app_data(), AnchorKind::RoamingAppData));
        if let Some(temp) = user.temp() {
            list.push((temp, AnchorKind::Temp));
        }
        list.extend(
            user.content_folders()
                .iter()
                .map(|folder| (folder, AnchorKind::KnownFolder)),
        );
    }
    list.push((ctx.windows_dir(), AnchorKind::WindowsDir));
    list.extend(
        ctx.program_files()
            .iter()
            .map(|folder| (folder, AnchorKind::ProgramFiles)),
    );
    list.push((ctx.program_data(), AnchorKind::ProgramData));
    list.extend(ctx.app_dirs().iter().map(|dir| (dir, AnchorKind::AppDir)));
    list
}

fn is_volume_system_entry(path: &NormalizedPath) -> bool {
    path.components().first().is_some_and(|first| {
        first.original().starts_with('$')
            || VOLUME_SYSTEM_ENTRIES
                .iter()
                .any(|name| fold_component(name) == first.folded())
    })
}

/// La ruta es exactamente un ancla o la raiz de una excepcion.
fn exact_anchor(ctx: &ProtectionContext, path: &NormalizedPath) -> Option<AnchorKind> {
    if let Some((_, kind)) = anchors(ctx).into_iter().find(|(anchor, _)| *anchor == path) {
        return Some(kind);
    }
    EXCEPTIONS
        .iter()
        .any(|def| overlap_with(ctx, def, path) == Some(Overlap::Exact))
        .then_some(AnchorKind::ExceptionRoot)
}

/// Ancestro estricto de un ancla o de una raiz de perfil de navegador: borrarlo arrastraria
/// lo protegido que contiene.
fn is_ancestor_of_protected(ctx: &ProtectionContext, path: &NormalizedPath) -> bool {
    anchors(ctx)
        .iter()
        .any(|(anchor, _)| anchor.is_strictly_under(path))
        || BROWSER_ROOTS
            .iter()
            .any(|def| browser_root(ctx, def).is_some_and(|root| root.is_strictly_under(path)))
}

/// Dentro de la raiz de perfiles y fuera del perfil del usuario actual.
fn is_other_user_profile(ctx: &ProtectionContext, path: &NormalizedPath) -> bool {
    path.is_strictly_under(ctx.users_root())
        && !ctx
            .user()
            .is_some_and(|user| path.starts_with(user.profile()))
}

/// Motivos de las raices protegidas que cubren la ruta, en el orden documentado.
fn covering_roots(ctx: &ProtectionContext, path: &NormalizedPath) -> Vec<ProtectedReason> {
    let mut reasons = Vec::new();
    if path.starts_with(ctx.windows_dir()) {
        reasons.push(ProtectedReason::WindowsDir);
    }
    if ctx
        .program_files()
        .iter()
        .any(|root| path.starts_with(root))
    {
        reasons.push(ProtectedReason::ProgramFiles);
    }
    if path.starts_with(ctx.program_data()) {
        reasons.push(ProtectedReason::ProgramData);
    }
    for def in BROWSER_ROOTS {
        if browser_root(ctx, def).is_some_and(|root| path.starts_with(&root)) {
            reasons.push(ProtectedReason::BrowserProfile(def.browser));
        }
    }
    if ctx.app_dirs().iter().any(|dir| path.starts_with(dir)) {
        reasons.push(ProtectedReason::AppDir);
    }
    reasons
}

/// Evalua las reglas puras sobre una ruta ya normalizada.
pub fn evaluate_rules(ctx: &ProtectionContext, path: &NormalizedPath) -> RuleVerdict {
    use ProtectedReason as Reason;
    if !ctx.supported_volumes().contains(&path.drive()) {
        return RuleVerdict::Protected(Reason::UnsupportedVolume);
    }
    if path.is_volume_root() {
        return RuleVerdict::Protected(Reason::VolumeRoot);
    }
    if is_volume_system_entry(path) {
        return RuleVerdict::Protected(Reason::VolumeSystemEntry);
    }
    if let Some(kind) = exact_anchor(ctx, path) {
        return RuleVerdict::Protected(Reason::Anchor(kind));
    }
    if is_other_user_profile(ctx, path) {
        return RuleVerdict::Protected(Reason::OtherUserProfile);
    }
    // Extension fail-closed sobre D-04: va despues del perfil ajeno para no alterar ese motivo.
    if is_ancestor_of_protected(ctx, path) {
        return RuleVerdict::Protected(Reason::Anchor(AnchorKind::AncestorOfProtectedRoot));
    }
    let covering = covering_roots(ctx, path);
    let exception = EXCEPTIONS.iter().find(|def| {
        overlap_with(ctx, def, path) == Some(Overlap::Below)
            && covering.iter().all(|reason| *reason == def.parent)
    });
    if let Some(def) = exception {
        return RuleVerdict::Allowed(AllowRule::Exception(def.id));
    }
    match covering.first() {
        Some(reason) => RuleVerdict::Protected(*reason),
        None => RuleVerdict::Allowed(AllowRule::Default),
    }
}

/// Atajo: `true` si las reglas puras no declaran elegible la ruta.
pub fn is_protected(ctx: &ProtectionContext, path: &NormalizedPath) -> bool {
    !evaluate_rules(ctx, path).is_allowed()
}

#[cfg(test)]
#[path = "rules_tests.rs"]
mod tests;

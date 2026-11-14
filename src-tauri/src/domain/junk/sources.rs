//! Ubicaciones cerradas por categoria (D-05), derivadas del `ProtectionContext` ya validado:
//! nada de rutas fijas ni de variables de entorno. Si el contexto no tiene un ancla, la
//! ubicacion que depende de ella no existe (fail-closed).

use crate::domain::junk::category::JunkCategory;
use crate::domain::junk::filter::{Filter, NameFilter, TEMP_MIN_AGE_MS};
use crate::domain::privileges::UserScope;
use crate::domain::protection::context::ProtectionContext;
use crate::domain::protection::path::NormalizedPath;

/// Subcarpetas de cache de un perfil Chromium (Chrome, Edge, Brave). Nombres exactos.
const CHROMIUM_CACHES: &[&str] = &["Cache", "Code Cache", "GPUCache"];
/// Subcarpeta de cache de un perfil de Firefox (bajo `<Local>`, nunca `<Roaming>`).
const FIREFOX_CACHES: &[&str] = &["cache2"];

/// Como se recorre una ubicacion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Depth {
    Recursive,
    /// Solo los archivos directos de la raiz; no se desciende a subcarpetas.
    FirstLevel,
}

/// Forma de la ubicacion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// La raiz se recorre tal cual.
    Direct,
    /// La raiz contiene carpetas de perfil (un solo nivel); de cada una se recorren solo las
    /// subcarpetas exactas indicadas.
    Profiles { subdirs: &'static [&'static str] },
}

/// Una ubicacion a recorrer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JunkSource {
    pub category: JunkCategory,
    pub root: NormalizedPath,
    pub scope: UserScope,
    pub filter: Filter,
    pub depth: Depth,
    pub layout: Layout,
}

impl JunkSource {
    /// Ubicacion recorrida en profundidad, tal cual (`Layout::Direct`).
    fn tree(
        category: JunkCategory,
        root: NormalizedPath,
        scope: UserScope,
        filter: Filter,
    ) -> Self {
        Self {
            category,
            root,
            scope,
            filter,
            depth: Depth::Recursive,
            layout: Layout::Direct,
        }
    }
}

/// Ubicaciones de todas las categorias de archivos para `ctx`, en el orden de
/// `JunkCategory::ALL`. `old_logs` (D-06) y `recycle_bin` (consulta agregada) no tienen.
pub fn sources(ctx: &ProtectionContext) -> Vec<JunkSource> {
    let windows = ctx.windows_dir();
    let program_data = ctx.program_data();
    let user = ctx.user();
    let temp_filter = Filter {
        min_age_ms: Some(TEMP_MIN_AGE_MS),
        name: NameFilter::Any,
    };
    let mut found = Vec::new();

    // Las categorias se emiten en el orden de `JunkCategory::ALL`.
    if let Some(temp) = user.and_then(|u| u.temp()) {
        found.push(JunkSource::tree(
            JunkCategory::TempUser,
            temp.clone(),
            UserScope::PerUser,
            temp_filter,
        ));
    }
    found.push(JunkSource::tree(
        JunkCategory::TempWindows,
        windows.joined(&["Temp"]),
        UserScope::Machine,
        temp_filter,
    ));
    if let Some(user) = user {
        let local = user.local_app_data();
        let browsers: [(JunkCategory, &[&str], &'static [&'static str]); 4] = [
            (
                JunkCategory::BrowserCacheChrome,
                &["Google", "Chrome", "User Data"],
                CHROMIUM_CACHES,
            ),
            (
                JunkCategory::BrowserCacheEdge,
                &["Microsoft", "Edge", "User Data"],
                CHROMIUM_CACHES,
            ),
            (
                JunkCategory::BrowserCacheFirefox,
                &["Mozilla", "Firefox", "Profiles"],
                FIREFOX_CACHES,
            ),
            (
                JunkCategory::BrowserCacheBrave,
                &["BraveSoftware", "Brave-Browser", "User Data"],
                CHROMIUM_CACHES,
            ),
        ];
        for (category, path, subdirs) in browsers {
            found.push(JunkSource {
                layout: Layout::Profiles { subdirs },
                ..JunkSource::tree(
                    category,
                    local.joined(path),
                    UserScope::PerUser,
                    Filter::NONE,
                )
            });
        }
    }
    found.push(JunkSource::tree(
        JunkCategory::WuCache,
        windows.joined(&["SoftwareDistribution", "Download"]),
        UserScope::Machine,
        Filter::NONE,
    ));
    if let Some(user) = user {
        found.push(JunkSource {
            depth: Depth::FirstLevel,
            ..JunkSource::tree(
                JunkCategory::ThumbnailCache,
                user.local_app_data()
                    .joined(&["Microsoft", "Windows", "Explorer"]),
                UserScope::PerUser,
                Filter {
                    min_age_ms: None,
                    name: NameFilter::ThumbCache,
                },
            )
        });
        found.push(JunkSource::tree(
            JunkCategory::CrashDumps,
            user.local_app_data().joined(&["CrashDumps"]),
            UserScope::PerUser,
            Filter::NONE,
        ));
    }
    let machine_dumps = [
        windows.joined(&["Minidump"]),
        program_data.joined(&["Microsoft", "Windows", "WER", "ReportArchive"]),
        program_data.joined(&["Microsoft", "Windows", "WER", "ReportQueue"]),
    ];
    for root in machine_dumps {
        found.push(JunkSource::tree(
            JunkCategory::CrashDumps,
            root,
            UserScope::Machine,
            Filter::NONE,
        ));
    }
    found
}

#[cfg(test)]
#[path = "sources_tests.rs"]
mod tests;

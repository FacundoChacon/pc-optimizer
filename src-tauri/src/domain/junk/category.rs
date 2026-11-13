//! Categorias de la whitelist de limpieza (D-05, KB 11). Lista cerrada: no hay forma de agregar
//! una en tiempo de ejecucion.

/// Categoria de basura detectable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JunkCategory {
    TempUser,
    TempWindows,
    BrowserCacheChrome,
    BrowserCacheEdge,
    BrowserCacheFirefox,
    BrowserCacheBrave,
    OldLogs,
    WuCache,
    ThumbnailCache,
    CrashDumps,
    RecycleBin,
}

/// Alcance de una categoria (el de cada ubicacion se declara aparte: `CrashDumps` es mixta).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CategoryScope {
    Machine,
    PerUser,
    Mixed,
}

impl CategoryScope {
    /// Cadena persistida (coincide con el `CHECK` de la migracion 002).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Machine => "machine",
            Self::PerUser => "per_user",
            Self::Mixed => "mixed",
        }
    }

    /// Inverso de `as_str`.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "machine" => Some(Self::Machine),
            "per_user" => Some(Self::PerUser),
            "mixed" => Some(Self::Mixed),
            _ => None,
        }
    }
}

impl JunkCategory {
    /// Las 11 categorias en el orden estable de la UI.
    pub const ALL: [JunkCategory; 11] = [
        Self::TempUser,
        Self::TempWindows,
        Self::BrowserCacheChrome,
        Self::BrowserCacheEdge,
        Self::BrowserCacheFirefox,
        Self::BrowserCacheBrave,
        Self::OldLogs,
        Self::WuCache,
        Self::ThumbnailCache,
        Self::CrashDumps,
        Self::RecycleBin,
    ];

    /// Cadena persistida (coincide con el `CHECK` de la migracion 002).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TempUser => "temp_user",
            Self::TempWindows => "temp_windows",
            Self::BrowserCacheChrome => "browser_cache_chrome",
            Self::BrowserCacheEdge => "browser_cache_edge",
            Self::BrowserCacheFirefox => "browser_cache_firefox",
            Self::BrowserCacheBrave => "browser_cache_brave",
            Self::OldLogs => "old_logs",
            Self::WuCache => "wu_cache",
            Self::ThumbnailCache => "thumbnail_cache",
            Self::CrashDumps => "crash_dumps",
            Self::RecycleBin => "recycle_bin",
        }
    }

    /// Alcance de la categoria (D-11). `CrashDumps` mezcla un origen por-usuario con tres de
    /// maquina. `OldLogs` (sin ubicaciones, D-06) se declara de maquina: sus logs candidatos
    /// viven en `C:\Windows`, y asi nunca se informa `blocked` por una cuenta distinta.
    pub fn scope(self) -> CategoryScope {
        match self {
            Self::TempWindows | Self::WuCache | Self::OldLogs => CategoryScope::Machine,
            Self::CrashDumps => CategoryScope::Mixed,
            Self::TempUser
            | Self::BrowserCacheChrome
            | Self::BrowserCacheEdge
            | Self::BrowserCacheFirefox
            | Self::BrowserCacheBrave
            | Self::ThumbnailCache
            | Self::RecycleBin => CategoryScope::PerUser,
        }
    }

    /// Inverso de `as_str`; distingue mayusculas y no recorta espacios. `custom_rule` (v0.3) y
    /// cualquier otra cadena dan `None`.
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|category| category.as_str() == raw)
    }
}

#[cfg(test)]
#[path = "category_tests.rs"]
mod tests;

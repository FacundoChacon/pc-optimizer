//! Modelo del analisis de espacio en disco (C-07, design D-01, D-04, D-07, D-08, D-11).

use crate::domain::duplicates::model::RequestError;
use crate::domain::model::EpochMillis;
use crate::domain::ports::{PortError, VolumeSpaceInfo};

/// Carpetas que se guardan como maximo por analisis (D-07).
pub const MAX_STORED_FOLDERS: usize = 20_000;
/// Archivos grandes que se guardan por analisis (D-08).
pub const MAX_LARGE_FILES: usize = 500;
/// Carpetas sin acceso que se detallan (D-11).
pub const MAX_ISSUES: usize = 50;

/// Mayor entero que JavaScript representa sin perdida.
const MAX_SAFE_INTEGER: i64 = (1 << 53) - 1;

/// Identificador de la carpeta elegida con el selector (D-02): la interfaz solo conoce este
/// numero, nunca la ruta que el backend guardo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PickId(u64);

impl PickId {
    /// `None` si es negativo o no cabe en un entero seguro de JavaScript.
    pub fn new(raw: i64) -> Option<Self> {
        u64::try_from(raw)
            .ok()
            .filter(|_| raw <= MAX_SAFE_INTEGER)
            .map(Self)
    }

    pub fn get(self) -> u64 {
        self.0
    }
}

/// Que analizar (D-01): el volumen del sistema o la carpeta elegida.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootRequest {
    SystemDrive,
    Picked(PickId),
}

impl RootRequest {
    /// Interpreta `{ kind, pickId? }` del IPC. Un tipo desconocido, un `pickId` ausente, negativo o
    /// fuera de rango, o un `pickId` con `system_drive` son argumentos invalidos.
    pub fn parse(kind: &str, pick_id: Option<i64>) -> Result<Self, RequestError> {
        match (kind, pick_id) {
            ("system_drive", None) => Ok(Self::SystemDrive),
            ("system_drive", Some(_)) => Err(RequestError::InvalidArgument(
                "system_drive does not take a pick id",
            )),
            ("picked", Some(raw)) => PickId::new(raw)
                .map(Self::Picked)
                .ok_or(RequestError::InvalidArgument("pick id out of range")),
            ("picked", None) => Err(RequestError::InvalidArgument("picked needs a pick id")),
            _ => Err(RequestError::InvalidArgument("unknown root kind")),
        }
    }
}

macro_rules! coded_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => $code:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            pub const ALL: &'static [$name] = &[$(Self::$variant),+];

            /// Codigo estable (se persiste y viaja por IPC).
            pub fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $code),+
                }
            }

            /// Inverso de `as_str`; distingue mayusculas.
            pub fn parse(raw: &str) -> Option<Self> {
                Self::ALL.iter().copied().find(|value| value.as_str() == raw)
            }
        }
    };
}

coded_enum! {
    /// Tipo de raiz analizada.
    RootKind { SystemDrive => "system_drive", Picked => "picked" }
}

coded_enum! {
    /// Cuanto se pudo leer de una carpeta (D-07).
    Access {
        Complete => "complete",
        Incomplete => "incomplete",
        Inaccessible => "inaccessible",
    }
}

coded_enum! {
    /// Por que una carpeta es opaca (D-04): se mide, pero ninguna ruta interior se guarda.
    OpaqueReason {
        OtherUserProfile => "other_user_profile",
        RecycleBin => "recycle_bin",
        SystemVolumeInformation => "system_volume_information",
    }
}

coded_enum! {
    /// Motivo por el que una carpeta no se pudo listar (D-11).
    IssueReason {
        AccessDenied => "access_denied",
        NotFound => "not_found",
        IoError => "io_error",
    }
}

impl IssueReason {
    pub fn from_port_error(error: &PortError) -> Self {
        match error {
            PortError::PermissionDenied(_) => Self::AccessDenied,
            PortError::NotFound(_) => Self::NotFound,
            PortError::InUse(_) | PortError::Unsupported(_) | PortError::Io(_) => Self::IoError,
        }
    }
}

/// Una carpeta que no se pudo listar (muestra de D-11). Nunca de una carpeta opaca.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderIssue {
    pub path: String,
    pub reason: IssueReason,
}

/// Una carpeta guardada. `parent` es el indice (en `FolderScanResults::folders`) de su padre; el
/// padre siempre va antes que la hija.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredFolder {
    pub parent: Option<usize>,
    pub path: String,
    pub depth: u32,
    pub size_bytes: u64,
    pub direct_bytes: u64,
    pub file_count: u64,
    pub dir_count: u64,
    pub access: Access,
    pub opaque: Option<OpaqueReason>,
    pub system: bool,
    pub junk_hint: bool,
}

/// Un archivo grande guardado; `folder` es el indice de la carpeta guardada mas cercana.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredLargeFile {
    pub folder: usize,
    pub path: String,
    pub size_bytes: u64,
    pub modified_at: Option<EpochMillis>,
}

/// Contadores de un analisis terminado.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FolderCounters {
    pub total_bytes: u64,
    pub file_count: u64,
    pub dir_count: u64,
    pub inaccessible_dirs: u64,
    pub reparse_skipped: u64,
    pub cloud_files: u64,
    pub opaque_dirs: u64,
    pub folders_stored: u64,
}

/// Resultado completo de un analisis, listo para guardar. Solo `scan_folders` lo arma (fuera de
/// los tests): el almacen no recibe estructuras armadas a mano.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderScanResults {
    pub root_kind: RootKind,
    pub root_path: String,
    pub volume: Option<VolumeSpaceInfo>,
    pub counters: FolderCounters,
    pub folders_truncated: bool,
    /// Por profundidad creciente (el padre antes que la hija); la raiz es la primera.
    pub folders: Vec<StoredFolder>,
    pub large_files: Vec<StoredLargeFile>,
    pub issues: Vec<FolderIssue>,
}

/// Espacio usado del volumen que el analisis no contabilizo (D-06).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unaccounted {
    /// `None` si no aplica (raiz elegida o sin datos del volumen).
    pub bytes: Option<u64>,
    /// Lo analizado supera lo usado (hard links, compresion): se informa 0.
    pub over_counted: bool,
}

/// "No contabilizado" = usado del volumen menos lo analizado, solo para `system_drive`.
pub fn unaccounted(
    root_kind: RootKind,
    volume: Option<VolumeSpaceInfo>,
    analyzed_bytes: u64,
) -> Unaccounted {
    match (root_kind, volume) {
        (RootKind::SystemDrive, Some(volume)) => {
            let used = volume.total_bytes.saturating_sub(volume.free_bytes);
            Unaccounted {
                bytes: Some(used.saturating_sub(analyzed_bytes)),
                over_counted: analyzed_bytes > used,
            }
        }
        _ => Unaccounted {
            bytes: None,
            over_counted: false,
        },
    }
}

/// Orden de los hijos de una carpeta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderSort {
    Size,
    Name,
}

impl FolderSort {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "size" => Some(Self::Size),
            "name" => Some(Self::Name),
            _ => None,
        }
    }
}

/// Orden del top de archivos grandes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LargeFileSort {
    Size,
    Newest,
    Oldest,
}

impl LargeFileSort {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "size" => Some(Self::Size),
            "newest" => Some(Self::Newest),
            "oldest" => Some(Self::Oldest),
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;

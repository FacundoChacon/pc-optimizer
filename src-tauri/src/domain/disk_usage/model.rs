//! Modelo del analisis de espacio en disco (C-07, design D-01, D-04, D-07, D-08, D-11).

use crate::domain::duplicates::model::RequestError;
use crate::domain::model::{EpochMillis, Scan};
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

    /// Identificador a partir de un contador del backend, acotado al mayor entero seguro.
    pub fn from_counter(counter: u64) -> Self {
        Self(counter.min(MAX_SAFE_INTEGER.unsigned_abs()))
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

coded_enum! {
    /// Por que no se puede mostrar un elemento en el Explorador (D-13).
    RevealReason {
        TargetMissing => "target_missing",
        Opaque => "opaque",
        DifferentUser => "different_user",
        Undetermined => "undetermined",
        ShellError => "shell_error",
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

impl FolderScanResults {
    /// Invariantes que toda implementacion de `FolderStore` exige antes de guardar: una sola raiz
    /// (la primera), padres antes que hijas con la profundidad coherente, rutas unicas, archivos
    /// que apuntan a una carpeta guardada, a lo sumo `MAX_ISSUES` incidencias y contadores que
    /// caben en un entero con signo de 64 bits.
    pub fn validate(&self) -> Result<(), &'static str> {
        let fits = |value: u64| i64::try_from(value).is_ok();
        let c = &self.counters;
        let counters = [
            c.total_bytes,
            c.file_count,
            c.dir_count,
            c.inaccessible_dirs,
            c.reparse_skipped,
            c.cloud_files,
            c.opaque_dirs,
            c.folders_stored,
        ];
        if !counters.into_iter().all(fits) {
            return Err("counter exceeds i64::MAX");
        }
        if self.root_path.is_empty() {
            return Err("empty root path");
        }
        if let Some(volume) = &self.volume
            && (volume.total_bytes == 0 || !fits(volume.total_bytes) || !fits(volume.free_bytes))
        {
            return Err("invalid volume space");
        }
        if self.folders.is_empty() || self.folders.len() > MAX_STORED_FOLDERS {
            return Err("invalid folder count");
        }
        if c.folders_stored != self.folders.len() as u64 {
            return Err("folders_stored does not match the folders");
        }
        let mut paths = std::collections::HashSet::new();
        for (index, folder) in self.folders.iter().enumerate() {
            let expected_depth = match (index, folder.parent) {
                (0, None) => 0,
                (0, Some(_)) | (_, None) => return Err("only the first folder is the root"),
                (_, Some(parent)) if parent < index => self.folders[parent].depth + 1,
                _ => return Err("parent must come before its child"),
            };
            if folder.depth != expected_depth {
                return Err("depth does not follow the parent");
            }
            if folder.path.is_empty() || !paths.insert(folder.path.as_str()) {
                return Err("empty or repeated folder path");
            }
            if folder.direct_bytes > folder.size_bytes
                || !fits(folder.size_bytes)
                || !fits(folder.file_count)
                || !fits(folder.dir_count)
            {
                return Err("invalid folder sizes");
            }
        }
        if self.large_files.len() > MAX_LARGE_FILES {
            return Err("too many large files");
        }
        let mut file_paths = std::collections::HashSet::new();
        for file in &self.large_files {
            if file.folder >= self.folders.len()
                || file.size_bytes == 0
                || !fits(file.size_bytes)
                || file.path.is_empty()
                || !file_paths.insert(file.path.as_str())
                || file.modified_at.is_some_and(|at| at.0 <= 0)
            {
                return Err("invalid large file");
            }
        }
        if self.issues.len() > MAX_ISSUES {
            return Err("too many issues");
        }
        let mut issue_paths = std::collections::HashSet::new();
        if self
            .issues
            .iter()
            .any(|issue| issue.path.is_empty() || !issue_paths.insert(issue.path.as_str()))
        {
            return Err("invalid issue");
        }
        Ok(())
    }
}

/// Una carpeta leida de la base, con su id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderRow {
    pub id: i64,
    pub parent_id: Option<i64>,
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
    /// Tiene carpetas hijas guardadas.
    pub has_children: bool,
}

impl FolderRow {
    /// Nombre para mostrar: el ultimo componente (la raiz del analisis, su ruta completa).
    pub fn name(&self) -> String {
        if self.depth == 0 {
            return self.path.clone();
        }
        self.path
            .rsplit('\\')
            .next()
            .unwrap_or(&self.path)
            .to_string()
    }
}

/// Un paso de las migas de pan, de la raiz a la carpeta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderCrumb {
    pub id: i64,
    pub name: String,
}

/// Pagina de los hijos de una carpeta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderPage {
    pub folder: FolderRow,
    pub breadcrumb: Vec<FolderCrumb>,
    /// Hijos guardados de la carpeta (todos, no solo los de la pagina).
    pub total: u64,
    pub items: Vec<FolderRow>,
    /// Tamano de la carpeta menos lo directo menos la suma de todos sus hijos guardados.
    pub remainder_bytes: u64,
}

/// Un archivo grande leido de la base.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LargeFileRow {
    pub id: i64,
    pub folder_id: i64,
    pub path: String,
    pub size_bytes: u64,
    pub modified_at: Option<EpochMillis>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LargeFilePage {
    pub total: u64,
    pub items: Vec<LargeFileRow>,
}

/// Resumen de un analisis terminado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderScanSummary {
    pub scan: Scan,
    pub root_kind: RootKind,
    pub root_path: String,
    pub volume: Option<VolumeSpaceInfo>,
    pub counters: FolderCounters,
    pub folders_truncated: bool,
    /// `None` si las carpetas ya se purgaron (hay un analisis mas nuevo).
    pub root_folder_id: Option<i64>,
    pub issues: Vec<FolderIssue>,
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

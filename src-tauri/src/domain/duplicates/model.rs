//! Modelo de la busqueda de duplicados (C-11, design D-01, D-03, D-08).

use crate::domain::model::{EpochMillis, Scan};
use crate::domain::ports::FileIdentity;
use crate::domain::protection::context::ContentFolder;

/// Tamano minimo de un archivo candidato (D-03): lista cerrada, por escaneo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MinSize(u64);

impl MinSize {
    pub const KIB_100: MinSize = MinSize(102_400);
    pub const MIB_1: MinSize = MinSize(1_048_576);
    pub const MIB_10: MinSize = MinSize(10_485_760);
    pub const MIB_100: MinSize = MinSize(104_857_600);
    /// Valor por defecto (SU-06).
    pub const DEFAULT: MinSize = Self::MIB_1;
    pub const ALL: [MinSize; 4] = [Self::KIB_100, Self::MIB_1, Self::MIB_10, Self::MIB_100];

    /// `None` si `bytes` no es una de las cuatro opciones.
    pub fn new(bytes: u64) -> Option<Self> {
        Self::ALL.into_iter().find(|size| size.0 == bytes)
    }

    pub fn bytes(self) -> u64 {
        self.0
    }
}

impl Default for MinSize {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Peticion de escaneo invalida.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RequestError {
    #[error("invalid argument: {0}")]
    InvalidArgument(&'static str),
}

/// Que escanear: carpetas de contenido (por clave) y tamano minimo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateScanRequest {
    roots: Vec<ContentFolder>,
    min_size: MinSize,
}

impl DuplicateScanRequest {
    /// `roots = None` => las seis carpetas. Una lista vacia, repetida o con una clave
    /// desconocida es un error; `min_size = None` => 1 MiB.
    pub fn new(roots: Option<Vec<String>>, min_size: Option<u64>) -> Result<Self, RequestError> {
        let min_size = match min_size {
            None => MinSize::DEFAULT,
            Some(bytes) => MinSize::new(bytes).ok_or(RequestError::InvalidArgument(
                "min size is not an allowed option",
            ))?,
        };
        let roots = match roots {
            None => ContentFolder::ALL.to_vec(),
            Some(keys) => {
                if keys.is_empty() {
                    return Err(RequestError::InvalidArgument("empty folder list"));
                }
                let mut parsed = Vec::with_capacity(keys.len());
                for key in &keys {
                    let folder = ContentFolder::parse(key)
                        .ok_or(RequestError::InvalidArgument("unknown folder key"))?;
                    if parsed.contains(&folder) {
                        return Err(RequestError::InvalidArgument("repeated folder key"));
                    }
                    parsed.push(folder);
                }
                // Orden estable de la UI, sin importar como se pidio.
                ContentFolder::ALL
                    .into_iter()
                    .filter(|folder| parsed.contains(folder))
                    .collect()
            }
        };
        Ok(Self { roots, min_size })
    }

    /// Carpetas seleccionadas, en el orden de `ContentFolder::ALL`.
    pub fn roots(&self) -> &[ContentFolder] {
        &self.roots
    }

    pub fn min_size(&self) -> MinSize {
        self.min_size
    }
}

/// Por que una carpeta seleccionada no esta disponible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootReason {
    NotFound,
    NotADirectory,
    ReparsePoint,
    Cloud,
    UnsupportedVolume,
    ContextUnavailable,
    /// No se pudo leer su metadato (permiso u otro error de E/S).
    Inaccessible,
}

impl RootReason {
    pub const ALL: [RootReason; 7] = [
        Self::NotFound,
        Self::NotADirectory,
        Self::ReparsePoint,
        Self::Cloud,
        Self::UnsupportedVolume,
        Self::ContextUnavailable,
        Self::Inaccessible,
    ];

    /// Codigo estable (se persiste y viaja por IPC).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotFound => "not_found",
            Self::NotADirectory => "not_a_directory",
            Self::ReparsePoint => "reparse_point",
            Self::Cloud => "cloud",
            Self::UnsupportedVolume => "unsupported_volume",
            Self::ContextUnavailable => "context_unavailable",
            Self::Inaccessible => "inaccessible",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|reason| reason.as_str() == raw)
    }
}

/// Estado de una carpeta de contenido en un escaneo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootStatus {
    /// Se recorrio.
    Done,
    NotSelected,
    /// Esta dentro de otra carpeta seleccionada: se recorre una sola vez, desde la de arriba.
    Nested,
    Unavailable(RootReason),
}

impl RootStatus {
    /// Codigo del estado (coincide con el `CHECK` de la migracion 003).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Done => "done",
            Self::NotSelected => "not_selected",
            Self::Nested => "nested",
            Self::Unavailable(_) => "unavailable",
        }
    }

    pub fn reason(self) -> Option<RootReason> {
        match self {
            Self::Unavailable(reason) => Some(reason),
            _ => None,
        }
    }

    /// Inverso de `as_str` + `reason`; `None` si la combinacion no es valida.
    pub fn parse(status: &str, reason: Option<&str>) -> Option<Self> {
        match (status, reason) {
            ("done", None) => Some(Self::Done),
            ("not_selected", None) => Some(Self::NotSelected),
            ("nested", None) => Some(Self::Nested),
            ("unavailable", Some(reason)) => RootReason::parse(reason).map(Self::Unavailable),
            _ => None,
        }
    }
}

/// Resultado por carpeta de contenido.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RootResult {
    pub folder: ContentFolder,
    pub status: RootStatus,
    /// Archivos vistos (atribuidos a la carpeta mas profunda que los contiene).
    pub files_seen: u64,
}

/// Contadores del escaneo (todos `>= 0`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScanCounters {
    pub files_seen: u64,
    pub below_min_size: u64,
    pub protected_count: u64,
    pub cloud_count: u64,
    pub hard_linked_count: u64,
    pub changed_count: u64,
    pub unreadable_count: u64,
    pub inaccessible_dirs: u64,
    pub excluded_dirs: u64,
    pub hashed_bytes: u64,
}

/// Una copia dentro de un grupo, tal como la encontro el escaneo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateFile {
    /// Carpeta de contenido mas profunda que la contiene.
    pub root: ContentFolder,
    /// Ruta real verificada por la proteccion.
    pub path: String,
    /// Ultima escritura leida del archivo abierto.
    pub modified: Option<EpochMillis>,
    pub identity: FileIdentity,
}

/// Grupo de 2 o mas copias con igual tamano y hash completo (RN-DUP-02).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateGroup {
    pub size_bytes: u64,
    /// blake3 completo en hex minuscula de 64 caracteres.
    pub blake3: String,
    /// Ordenadas por ruta; al menos 2, con identidades distintas.
    pub files: Vec<DuplicateFile>,
}

impl DuplicateGroup {
    /// Bytes que se liberarian conservando una sola copia.
    pub fn reclaimable_bytes(&self) -> u64 {
        self.size_bytes
            .saturating_mul(self.files.len().saturating_sub(1) as u64)
    }
}

/// Resultado de un escaneo completo (sin cancelar).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateScanResults {
    pub min_size: MinSize,
    /// Una entrada por carpeta de contenido, en el orden de `ContentFolder::ALL`.
    pub roots: Vec<RootResult>,
    pub counters: ScanCounters,
    pub groups: Vec<DuplicateGroup>,
}

/// Como termina un escaneo: solo `Completed` puede persistirse como `done`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DuplicateScanOutcome {
    Completed(DuplicateScanResults),
    /// Cancelado por el usuario: nada se guarda.
    Cancelled,
}

/// Copia ya guardada, con su identificador de fila.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredDuplicateFile {
    pub id: i64,
    pub file: DuplicateFile,
    /// `true` solo si un grupo resuelto la conservo (D-10).
    pub kept: bool,
}

/// Grupo ya guardado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredDuplicateGroup {
    pub id: i64,
    pub size_bytes: u64,
    pub blake3: String,
    pub resolved_at: Option<EpochMillis>,
    /// Ordenadas por ruta.
    pub files: Vec<StoredDuplicateFile>,
}

impl StoredDuplicateGroup {
    pub fn reclaimable_bytes(&self) -> u64 {
        self.size_bytes
            .saturating_mul(self.files.len().saturating_sub(1) as u64)
    }
}

/// Pagina de grupos (con filtros ya aplicados al total).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateGroupPage {
    pub total: u64,
    pub groups: Vec<StoredDuplicateGroup>,
}

/// Resumen del ultimo escaneo de duplicados terminado (`done`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateScanSummary {
    pub scan: Scan,
    pub min_size: MinSize,
    pub roots: Vec<RootResult>,
    pub counters: ScanCounters,
    pub group_count: u64,
    pub duplicate_file_count: u64,
    pub reclaimable_bytes: u64,
    pub open_group_count: u64,
}

/// Resolucion de un grupo tras ejecutar su limpieza (D-17): las copias que el usuario conservo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupResolution {
    pub group_id: i64,
    pub kept_file_ids: Vec<i64>,
}

fn is_lower_hex_64(raw: &str) -> bool {
    raw.len() == 64
        && raw
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

impl DuplicateScanResults {
    /// Invariantes que toda implementacion de `DuplicateStore` exige antes de guardar: grupos de 2
    /// o mas copias con identidades y rutas distintas, hash de 64 hex minuscula, tamano > 0, un
    /// grupo por (tamano, hash) y contadores que caben en un entero con signo de 64 bits.
    pub fn validate(&self) -> Result<(), &'static str> {
        let c = &self.counters;
        let counters = [
            c.files_seen,
            c.below_min_size,
            c.protected_count,
            c.cloud_count,
            c.hard_linked_count,
            c.changed_count,
            c.unreadable_count,
            c.inaccessible_dirs,
            c.excluded_dirs,
            c.hashed_bytes,
        ];
        if counters.iter().any(|value| i64::try_from(*value).is_err()) {
            return Err("counter exceeds i64::MAX");
        }
        let mut roots = std::collections::BTreeSet::new();
        for root in &self.roots {
            if !roots.insert(root.folder) || i64::try_from(root.files_seen).is_err() {
                return Err("invalid root result");
            }
        }
        let mut keys = std::collections::BTreeSet::new();
        for group in &self.groups {
            if group.size_bytes == 0
                || i64::try_from(group.size_bytes).is_err()
                || !is_lower_hex_64(&group.blake3)
            {
                return Err("invalid group");
            }
            if group.files.len() < 2 {
                return Err("group with fewer than 2 copies");
            }
            if !keys.insert((group.size_bytes, group.blake3.as_str())) {
                return Err("duplicate group");
            }
            let mut identities = std::collections::BTreeSet::new();
            let mut paths = std::collections::BTreeSet::new();
            for file in &group.files {
                if file.path.is_empty() || !paths.insert(file.path.as_str()) {
                    return Err("invalid copy path");
                }
                if !identities.insert((file.identity.volume_serial, file.identity.file_id)) {
                    return Err("two copies share an identity");
                }
            }
        }
        Ok(())
    }

    /// Cantidad de copias (todas las de todos los grupos).
    pub fn duplicate_file_count(&self) -> u64 {
        self.groups.iter().map(|g| g.files.len() as u64).sum()
    }

    pub fn reclaimable_bytes(&self) -> u64 {
        self.groups
            .iter()
            .map(DuplicateGroup::reclaimable_bytes)
            .sum()
    }
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;

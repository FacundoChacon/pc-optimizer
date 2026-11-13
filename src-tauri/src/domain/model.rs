//! Modelo persistido del dominio (escaneos, bitacora de limpieza y ajustes por defecto).

/// Instante UTC en milisegundos desde la epoca Unix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EpochMillis(pub i64);

/// Identificador de un escaneo persistido.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScanId(pub i64);

/// Cantidad de escaneos finalizados que se conservan.
pub const SCAN_RETENTION: usize = 50;

/// Ajustes sembrados por la migracion 001 (clave, JSON).
pub const DEFAULT_SETTINGS: &[(&str, &str)] = &[
    ("ui_mode", "\"simple\""),
    ("autostart_tray", "false"),
    ("language", "\"es\""),
];

/// Tipo de escaneo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanKind {
    Full,
    Junk,
    Duplicates,
    Folders,
}

impl ScanKind {
    /// Cadena persistida (coincide con el `CHECK` de la migracion 001).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Junk => "junk",
            Self::Duplicates => "duplicates",
            Self::Folders => "folders",
        }
    }

    /// Inverso de `as_str`; distingue mayusculas y no recorta espacios.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "full" => Some(Self::Full),
            "junk" => Some(Self::Junk),
            "duplicates" => Some(Self::Duplicates),
            "folders" => Some(Self::Folders),
            _ => None,
        }
    }
}

/// Origen del escaneo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanTrigger {
    Manual,
    Scheduled,
}

impl ScanTrigger {
    /// Cadena persistida (coincide con el `CHECK` de la migracion 001).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Scheduled => "scheduled",
        }
    }

    /// Inverso de `as_str`; distingue mayusculas y no recorta espacios.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "manual" => Some(Self::Manual),
            "scheduled" => Some(Self::Scheduled),
            _ => None,
        }
    }
}

/// Estado de un escaneo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanStatus {
    Running,
    Done,
    Cancelled,
    Error,
}

impl ScanStatus {
    /// Cadena persistida (coincide con el `CHECK` de la migracion 001).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
            Self::Error => "error",
        }
    }

    /// Inverso de `as_str`; distingue mayusculas y no recorta espacios.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "running" => Some(Self::Running),
            "done" => Some(Self::Done),
            "cancelled" => Some(Self::Cancelled),
            "error" => Some(Self::Error),
            _ => None,
        }
    }

    /// `true` para todo estado distinto de `Running`.
    pub fn is_terminal(self) -> bool {
        self != Self::Running
    }
}

/// Resultado de procesar un elemento en una limpieza.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupOutcome {
    MovedToTrash,
    SkippedLocked,
    SkippedProtected,
    SkippedCloud,
    Error,
    RecycleBinEmptied,
}

impl CleanupOutcome {
    /// Cadena persistida (coincide con el `CHECK` de la migracion 001).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MovedToTrash => "moved_to_trash",
            Self::SkippedLocked => "skipped_locked",
            Self::SkippedProtected => "skipped_protected",
            Self::SkippedCloud => "skipped_cloud",
            Self::Error => "error",
            Self::RecycleBinEmptied => "recycle_bin_emptied",
        }
    }

    /// Inverso de `as_str`; distingue mayusculas y no recorta espacios.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "moved_to_trash" => Some(Self::MovedToTrash),
            "skipped_locked" => Some(Self::SkippedLocked),
            "skipped_protected" => Some(Self::SkippedProtected),
            "skipped_cloud" => Some(Self::SkippedCloud),
            "error" => Some(Self::Error),
            "recycle_bin_emptied" => Some(Self::RecycleBinEmptied),
            _ => None,
        }
    }
}

/// Escaneo a crear.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewScan {
    pub kind: ScanKind,
    pub trigger: ScanTrigger,
    pub started_at: EpochMillis,
}

/// Escaneo persistido.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scan {
    pub id: ScanId,
    pub kind: ScanKind,
    pub trigger: ScanTrigger,
    pub status: ScanStatus,
    pub started_at: EpochMillis,
    pub finished_at: Option<EpochMillis>,
}

/// Entrada de bitacora a agregar (append-only).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewCleanupLogEntry {
    pub ts: EpochMillis,
    pub category: String,
    pub path: String,
    pub size_bytes: u64,
    pub outcome: CleanupOutcome,
    pub scan_id: Option<ScanId>,
    pub error_detail: Option<String>,
}

/// Entrada de bitacora persistida.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanupLogEntry {
    pub id: i64,
    pub ts: EpochMillis,
    pub category: String,
    pub path: String,
    pub size_bytes: u64,
    pub outcome: CleanupOutcome,
    pub scan_id: Option<ScanId>,
    pub error_detail: Option<String>,
}

/// Escaneos a podar para conservar `keep` finalizados.
///
/// Se conservan los `keep` mas recientes (por `started_at`, desempate por `id`) y todo
/// escaneo `Running`; se devuelven los finalizados sobrantes, del mas antiguo al mas nuevo.
pub fn scans_to_prune(scans: &[Scan], keep: usize) -> Vec<ScanId> {
    let mut ordered: Vec<&Scan> = scans.iter().collect();
    ordered.sort_by_key(|s| (std::cmp::Reverse(s.started_at), std::cmp::Reverse(s.id)));
    let mut pruned: Vec<&Scan> = ordered
        .into_iter()
        .skip(keep)
        .filter(|s| s.status.is_terminal())
        .collect();
    pruned.reverse();
    pruned.into_iter().map(|s| s.id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enums_should_round_trip_through_exact_migration_strings() {
        for (v, s) in [
            (ScanKind::Full, "full"),
            (ScanKind::Junk, "junk"),
            (ScanKind::Duplicates, "duplicates"),
            (ScanKind::Folders, "folders"),
        ] {
            assert_eq!(v.as_str(), s);
            assert_eq!(ScanKind::parse(s), Some(v));
        }
        for (v, s) in [
            (ScanTrigger::Manual, "manual"),
            (ScanTrigger::Scheduled, "scheduled"),
        ] {
            assert_eq!(v.as_str(), s);
            assert_eq!(ScanTrigger::parse(s), Some(v));
        }
        for (v, s) in [
            (ScanStatus::Running, "running"),
            (ScanStatus::Done, "done"),
            (ScanStatus::Cancelled, "cancelled"),
            (ScanStatus::Error, "error"),
        ] {
            assert_eq!(v.as_str(), s);
            assert_eq!(ScanStatus::parse(s), Some(v));
        }
        for (v, s) in [
            (CleanupOutcome::MovedToTrash, "moved_to_trash"),
            (CleanupOutcome::SkippedLocked, "skipped_locked"),
            (CleanupOutcome::SkippedProtected, "skipped_protected"),
            (CleanupOutcome::SkippedCloud, "skipped_cloud"),
            (CleanupOutcome::Error, "error"),
            (CleanupOutcome::RecycleBinEmptied, "recycle_bin_emptied"),
        ] {
            assert_eq!(v.as_str(), s);
            assert_eq!(CleanupOutcome::parse(s), Some(v));
        }
    }

    #[test]
    fn parse_should_reject_unknown_empty_or_wrong_case() {
        for raw in ["quick", "", "DONE", "Full", " full"] {
            assert_eq!(ScanKind::parse(raw), None, "{raw:?}");
            assert_eq!(ScanStatus::parse(raw), None, "{raw:?}");
        }
        assert_eq!(ScanTrigger::parse("auto"), None);
        assert_eq!(CleanupOutcome::parse("deleted"), None);
        assert_eq!(CleanupOutcome::parse(""), None);
    }

    #[test]
    fn is_terminal_should_be_false_only_for_running() {
        assert!(!ScanStatus::Running.is_terminal());
        for s in [ScanStatus::Done, ScanStatus::Cancelled, ScanStatus::Error] {
            assert!(s.is_terminal(), "{s:?}");
        }
    }

    fn scan(id: i64, started: i64, status: ScanStatus) -> Scan {
        Scan {
            id: ScanId(id),
            kind: ScanKind::Junk,
            trigger: ScanTrigger::Manual,
            status,
            started_at: EpochMillis(started),
            finished_at: (status != ScanStatus::Running).then_some(EpochMillis(started + 1)),
        }
    }

    #[test]
    fn prune_should_return_oldest_finished_when_over_keep() {
        let scans: Vec<Scan> = (1..=51)
            .map(|i| scan(i, 1000 + i, ScanStatus::Done))
            .collect();
        assert_eq!(scans_to_prune(&scans, SCAN_RETENTION), vec![ScanId(1)]);
    }

    #[test]
    fn prune_should_break_started_at_ties_by_lowest_id() {
        let scans = vec![
            scan(5, 100, ScanStatus::Done),
            scan(3, 100, ScanStatus::Done),
            scan(9, 200, ScanStatus::Done),
        ];
        assert_eq!(scans_to_prune(&scans, 2), vec![ScanId(3)]);
    }

    #[test]
    fn prune_should_never_return_running_and_take_next_finished() {
        let scans = vec![
            scan(1, 100, ScanStatus::Running),
            scan(2, 200, ScanStatus::Done),
            scan(3, 300, ScanStatus::Done),
        ];
        assert_eq!(scans_to_prune(&scans, 1), vec![ScanId(2)]);
    }

    #[test]
    fn prune_should_be_empty_when_keep_exceeds_count() {
        let scans = vec![
            scan(1, 100, ScanStatus::Done),
            scan(2, 200, ScanStatus::Done),
        ];
        assert!(scans_to_prune(&scans, 5).is_empty());
        assert!(scans_to_prune(&[], 3).is_empty());
    }
}

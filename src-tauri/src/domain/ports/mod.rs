//! Puertos del dominio: contratos hacia el SO y la persistencia.
//!
//! El puerto `Registry` (snapshot/undo de mejoras) lo define C-16; C-17 lo extiende.

pub mod clock;
pub mod content;
pub mod error;
pub mod fs;
pub mod recycle_bin;
pub mod recycle_bin_empty;
pub mod recycle_bin_policy;
pub mod sensors;
pub mod session;
pub mod store;
pub mod trash;

#[cfg(test)]
pub(crate) mod content_contract;
#[cfg(test)]
pub(crate) mod fakes;
#[cfg(test)]
pub(crate) mod fakes_content;
#[cfg(test)]
pub(crate) mod fs_contract;
#[cfg(test)]
pub(crate) mod store_contract;

pub use clock::Clock;
pub use content::{ContentHandle, ContentReader};
pub use error::PortError;
pub use fs::{AncestorPin, DirEntry, EntryKind, FileIdentity, FileMeta, FileSystem};
pub use recycle_bin::{RecycleBinInfo, RecycleBinUsage};
pub use recycle_bin_empty::{EmptyError, RecycleBinEmptier};
pub use recycle_bin_policy::{
    BinAvailability, DisabledReason, RawBinPolicy, RecycleBinPolicy, check_for_move, check_move,
    evaluate_policy,
};
pub use sensors::{SensorSample, Sensors};
pub use session::SessionIdentity;
pub use store::{CleanupLogStore, JunkFindingStore, ScanStore, SettingsStore, Store, StoreError};
pub use trash::{Trash, TrashError};

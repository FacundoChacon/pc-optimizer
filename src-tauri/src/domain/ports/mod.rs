//! Puertos del dominio: contratos hacia el SO y la persistencia.
//!
//! El puerto `Registry` (snapshot/undo de mejoras) lo define C-16; C-17 lo extiende.

pub mod clock;
pub mod error;
pub mod fs;
pub mod sensors;
pub mod store;
pub mod trash;

#[cfg(test)]
pub(crate) mod fakes;
#[cfg(test)]
pub(crate) mod fs_contract;
#[cfg(test)]
pub(crate) mod store_contract;

pub use clock::Clock;
pub use error::PortError;
pub use fs::{AncestorPin, DirEntry, EntryKind, FileIdentity, FileMeta, FileSystem};
pub use sensors::{SensorSample, Sensors};
pub use store::{CleanupLogStore, ScanStore, SettingsStore, Store, StoreError};
pub use trash::Trash;

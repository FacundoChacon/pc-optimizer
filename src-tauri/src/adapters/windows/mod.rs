//! Adaptadores de Windows: sistema de archivos real y carpetas conocidas (design D-05, D-10).
//!
//! Unico lugar de C-03 con `unsafe`; cada bloque es minimo y lleva su `// SAFETY:`.

pub mod content;
#[cfg(test)]
mod disk_usage;
pub mod explorer;
pub mod folder_picker;
pub mod fs;
pub mod known_folders;
pub mod privileges;
pub mod recycle_bin;
pub mod recycle_bin_empty;
pub mod recycle_bin_policy;
pub mod trash;
pub(crate) mod trash_error;
pub mod volume;

pub use content::WindowsContentReader;
pub use explorer::WindowsExplorer;
pub use folder_picker::WindowsFolderPicker;
pub use fs::WindowsFileSystem;
pub use known_folders::{UserFolderSource, resolve_protection_context};
pub use privileges::SystemSessionIdentity;
pub use recycle_bin::WindowsRecycleBin;
pub use volume::WindowsVolumeSpace;

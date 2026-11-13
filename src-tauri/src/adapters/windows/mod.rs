//! Adaptadores de Windows: sistema de archivos real y carpetas conocidas (design D-05, D-10).
//!
//! Unico lugar de C-03 con `unsafe`; cada bloque es minimo y lleva su `// SAFETY:`.

pub mod fs;
pub mod known_folders;
pub mod privileges;

pub use fs::WindowsFileSystem;
pub use known_folders::{UserFolderSource, resolve_protection_context};
pub use privileges::SystemSessionIdentity;

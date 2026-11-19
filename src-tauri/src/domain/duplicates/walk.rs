//! Recorrido de una carpeta de contenido (design D-02): el nucleo de `junk::walk` con la lista
//! cerrada de carpetas de herramientas excluidas.

use std::path::Path;

use crate::domain::junk::walk::{WalkReport, WalkVisitor, walk_directory};
use crate::domain::ports::FileSystem;
use crate::domain::protection::path::fold_component;

/// Carpetas de herramientas cuyo contenido son duplicados "legitimos" (D-02). Cerrada.
pub const EXCLUDED_DIRS: [&str; 4] = [".git", ".hg", ".svn", "node_modules"];

/// `true` si `name` (comparado como NTFS) es una carpeta de herramientas excluida.
pub fn is_excluded_dir(name: &str) -> bool {
    let folded = fold_component(name);
    EXCLUDED_DIRS
        .iter()
        .any(|excluded| fold_component(excluded) == folded)
}

/// Recorre `root` sin seguir reparse points ni descender a las carpetas excluidas.
pub fn walk_content_root<F: FileSystem, V: WalkVisitor>(
    fs: &F,
    root: &Path,
    visitor: &mut V,
) -> WalkReport {
    walk_directory(fs, root, is_excluded_dir, visitor)
}

#[cfg(test)]
#[path = "walk_tests.rs"]
mod tests;

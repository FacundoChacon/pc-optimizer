//! Recorrido de una ubicacion (D-09) sobre el puerto `FileSystem`: iterativo, determinista y sin
//! seguir puntos de reanalisis. Solo lista carpetas y lee metadatos; nunca modifica nada.

use std::path::{Path, PathBuf};

use crate::domain::junk::filter::Filter;
use crate::domain::junk::sources::{Depth, JunkSource, Layout};
use crate::domain::model::EpochMillis;
use crate::domain::ports::{DirEntry, EntryKind, FileSystem, PortError};
use crate::domain::protection::path::fold_component;

/// Quien recibe lo que encuentra el recorrido. Todas las llamadas son de lectura del resultado.
pub trait WalkVisitor {
    /// Se consulta antes de cada listado de carpeta y de cada entrada; `true` detiene el recorrido.
    fn is_cancelled(&self) -> bool;
    /// Un archivo fue listado (antes de filtrar por nombre o edad).
    fn file_seen(&mut self);
    /// Archivo que pasa los filtros de la ubicacion.
    fn candidate(&mut self, entry: DirEntry);
    /// Punto de reanalisis encontrado; su destino no se recorre.
    fn reparse_point(&mut self, path: PathBuf);
}

/// Como termino el recorrido.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalkEnd {
    Completed,
    Cancelled,
}

/// Resumen del recorrido de una ubicacion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalkReport {
    pub end: WalkEnd,
    /// Carpetas que no se pudieron listar (incluida la raiz si no se pudo usar).
    pub inaccessible: u64,
    /// `false` si la raiz de la ubicacion no existe (sin error).
    pub present: bool,
}

/// Recorre `source` entregando al `visitor` candidatos y puntos de reanalisis.
pub fn walk_source<F: FileSystem, V: WalkVisitor>(
    fs: &F,
    source: &JunkSource,
    now: EpochMillis,
    visitor: &mut V,
) -> WalkReport {
    let mut walker = Walker {
        fs,
        filter: source.filter,
        depth: source.depth,
        now,
        visitor,
        inaccessible: 0,
        cancelled: false,
    };
    let root = source.root.to_path_buf();
    let present = match source.layout {
        Layout::Direct => walker.walk_root(&root),
        Layout::Profiles { subdirs } => walker.walk_profiles(&root, subdirs),
    };
    WalkReport {
        end: if walker.cancelled {
            WalkEnd::Cancelled
        } else {
            WalkEnd::Completed
        },
        inaccessible: walker.inaccessible,
        present,
    }
}

struct Walker<'a, F: FileSystem, V: WalkVisitor> {
    fs: &'a F,
    filter: Filter,
    depth: Depth,
    now: EpochMillis,
    visitor: &'a mut V,
    inaccessible: u64,
    cancelled: bool,
}

impl<F: FileSystem, V: WalkVisitor> Walker<'_, F, V> {
    /// Consulta la cancelacion; una vez detectada se mantiene.
    fn stop(&mut self) -> bool {
        if self.visitor.is_cancelled() {
            self.cancelled = true;
        }
        self.cancelled
    }

    /// Valida la raiz de una carpeta a recorrer y la recorre. `false` si no existe (sin error);
    /// una raiz que es reparse point, archivo o ilegible cuenta como inaccesible y no se recorre.
    fn walk_root(&mut self, root: &Path) -> bool {
        if self.stop() {
            return true;
        }
        match self.fs.metadata(root) {
            Err(PortError::NotFound(_)) => return false,
            Err(_) => self.inaccessible += 1,
            Ok(meta) if meta.kind != EntryKind::Directory => self.inaccessible += 1,
            Ok(_) => self.walk_tree(root),
        }
        true
    }

    /// Recorrido iterativo (pila explicita): archivos de cada carpeta en orden de nombre y luego
    /// sus subcarpetas, tambien en orden.
    fn walk_tree(&mut self, root: &Path) {
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            if self.stop() {
                return;
            }
            let Some(entries) = self.list(&dir) else {
                continue;
            };
            let mut subdirs = Vec::new();
            for entry in entries {
                if self.stop() {
                    return;
                }
                self.visit(entry, &mut subdirs);
            }
            stack.extend(subdirs.into_iter().rev());
        }
    }

    /// Hijos ordenados por nombre plegado; un fallo cuenta como carpeta inaccesible.
    fn list(&mut self, dir: &Path) -> Option<Vec<DirEntry>> {
        match self.fs.read_dir(dir) {
            Ok(mut entries) => {
                entries.sort_by_cached_key(|entry| fold_component(&file_name(&entry.path)));
                Some(entries)
            }
            Err(_) => {
                self.inaccessible += 1;
                None
            }
        }
    }

    fn visit(&mut self, entry: DirEntry, subdirs: &mut Vec<PathBuf>) {
        let name = file_name(&entry.path);
        match (entry.meta.kind, entry.meta.cloud_placeholder) {
            (EntryKind::Directory, _) => {
                if self.depth == Depth::Recursive {
                    subdirs.push(entry.path);
                }
            }
            // Un reparse de nube se trata como archivo: la proteccion lo decide como `SkippedCloud`.
            (EntryKind::File, _) | (EntryKind::ReparsePoint, true) => {
                self.visitor.file_seen();
                if self.filter.accepts(&name, entry.meta.modified, self.now) {
                    self.visitor.candidate(entry);
                }
            }
            (EntryKind::ReparsePoint, false) => {
                if self.filter.accepts_name(&name) {
                    self.visitor.reparse_point(entry.path);
                }
            }
        }
    }

    /// Raiz de perfiles (`User Data`): de cada subcarpeta-perfil solo se prueban las subcarpetas
    /// de cache exactas; ningun otro hijo se abre ni se informa.
    fn walk_profiles(&mut self, root: &Path, subdirs: &[&str]) -> bool {
        if self.stop() {
            return true;
        }
        match self.fs.metadata(root) {
            Err(PortError::NotFound(_)) => return false,
            Err(_) => {
                self.inaccessible += 1;
                return true;
            }
            Ok(meta) if meta.kind != EntryKind::Directory => {
                self.inaccessible += 1;
                return true;
            }
            Ok(_) => {}
        }
        let Some(children) = self.list(root) else {
            return true;
        };
        for profile in children
            .iter()
            .filter(|child| child.meta.kind == EntryKind::Directory)
        {
            for sub in subdirs {
                self.walk_root(&profile.path.join(sub));
                if self.cancelled {
                    return true;
                }
            }
        }
        true
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "walk_tests.rs"]
mod tests;

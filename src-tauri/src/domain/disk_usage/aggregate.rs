//! Agregador del analisis (C-07, design D-04, D-05, D-06, D-11): recibe lo que encuentra el
//! recorrido compartido (`walk_directory`) y arma el arbol de carpetas con sus sumas.
//!
//! Solo cuenta: nunca abre ni modifica nada. Las carpetas opacas se suman en su propio nodo y no
//! tienen nodos hijos ni rutas interiores.

use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::domain::disk_usage::classify::Classifier;
use crate::domain::disk_usage::model::{
    Access, FolderIssue, IssueReason, MAX_ISSUES, MAX_LARGE_FILES, OpaqueReason,
};
use crate::domain::disk_usage::top_files::{TopFile, TopFiles};
use crate::domain::junk::walk::WalkVisitor;
use crate::domain::ports::{DirEntry, EntryKind, PortError};
use crate::domain::protection::path::normalize_lexical;

/// Solo las carpetas a esta profundidad absoluta (o menos) pueden ser opacas: `C:\$Recycle.Bin`,
/// `C:\System Volume Information` y `C:\Users\<perfil>`.
const MAX_OPAQUE_DEPTH: usize = 2;

/// Una carpeta del arbol del analisis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeNode {
    pub parent: Option<usize>,
    /// Profundidad respecto de la raiz del analisis (la raiz es 0).
    pub depth: u32,
    /// Ultimo componente de la ruta; la raiz lleva su ruta completa.
    pub name: String,
    /// Recursivo.
    pub size_bytes: u64,
    /// Archivos directamente dentro.
    pub direct_bytes: u64,
    /// Recursivo.
    pub file_count: u64,
    /// Carpetas descendientes (recursivo, sin contar la propia).
    pub dir_count: u64,
    pub access: Access,
    pub opaque: Option<OpaqueReason>,
}

/// Contadores del recorrido que no salen de las sumas por carpeta.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TreeCounters {
    pub reparse_skipped: u64,
    pub cloud_files: u64,
    /// Todas las carpetas que no se pudieron listar, tambien las de dentro de opacas.
    pub inaccessible_dirs: u64,
    pub opaque_dirs: u64,
}

/// Arbol terminado: los padres se crean antes que sus hijos.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FolderTree {
    pub nodes: Vec<TreeNode>,
    pub counters: TreeCounters,
    /// Hasta `MAX_ISSUES`, en orden de recorrido y nunca de una carpeta opaca.
    pub issues: Vec<FolderIssue>,
    /// Los archivos mas grandes (fuera de carpetas opacas), tamano descendente.
    pub top: Vec<TopFile>,
}

impl FolderTree {
    /// Ruta completa del nodo `index`.
    pub fn path_of(&self, index: usize) -> String {
        let mut names = Vec::new();
        let mut current = Some(index);
        while let Some(i) = current {
            let node = &self.nodes[i];
            names.push(node.name.as_str());
            current = node.parent;
        }
        let mut path = String::new();
        for name in names.into_iter().rev() {
            if path.is_empty() {
                path.push_str(name);
                continue;
            }
            if !path.ends_with('\\') {
                path.push('\\');
            }
            path.push_str(name);
        }
        path
    }
}

/// Un nivel de la cadena de ancestros de la carpeta que se esta recorriendo.
#[derive(Debug, Clone, Copy)]
struct Frame {
    node: usize,
    /// La carpeta esta DENTRO de una opaca: no tiene nodo propio, aporta a `node`.
    inside: bool,
}

pub struct FolderAggregator<'a> {
    classifier: &'a Classifier<'a>,
    cancel: &'a AtomicBool,
    nodes: Vec<TreeNode>,
    chain: Vec<Frame>,
    root_components: Option<usize>,
    counters: TreeCounters,
    issues: Vec<FolderIssue>,
    top: TopFiles,
}

impl<'a> FolderAggregator<'a> {
    pub fn new(classifier: &'a Classifier<'a>, cancel: &'a AtomicBool) -> Self {
        Self {
            classifier,
            cancel,
            nodes: Vec::new(),
            chain: Vec::new(),
            root_components: None,
            counters: TreeCounters::default(),
            issues: Vec::new(),
            top: TopFiles::new(MAX_LARGE_FILES),
        }
    }

    /// Consolida las sumas de abajo hacia arriba y entrega el arbol.
    pub fn finish(mut self) -> FolderTree {
        for index in (1..self.nodes.len()).rev() {
            let (size, files, dirs, access, parent) = {
                let child = &self.nodes[index];
                (
                    child.size_bytes,
                    child.file_count,
                    child.dir_count,
                    child.access,
                    child.parent,
                )
            };
            let Some(parent) = parent else { continue };
            let parent = &mut self.nodes[parent];
            parent.size_bytes += size;
            parent.file_count += files;
            parent.dir_count += dirs + 1;
            if access != Access::Complete && parent.access == Access::Complete {
                parent.access = Access::Incomplete;
            }
        }
        FolderTree {
            nodes: self.nodes,
            counters: self.counters,
            issues: self.issues,
            top: self.top.into_sorted(),
        }
    }

    /// Posiciona la cadena de ancestros para una carpeta nueva y devuelve su profundidad y su
    /// ancestro (el ultimo de la cadena).
    fn enter(&mut self, path: &Path) -> (usize, usize, Option<Frame>) {
        let components = path
            .components()
            .filter(|c| matches!(c, Component::Normal(_)))
            .count();
        let root_components = *self.root_components.get_or_insert(components);
        let depth = components.saturating_sub(root_components);
        self.chain.truncate(depth);
        (depth, components, self.chain.last().copied())
    }

    fn new_node(
        &mut self,
        path: &Path,
        depth: usize,
        absolute: usize,
        parent: Option<Frame>,
    ) -> usize {
        let name = if depth == 0 {
            path.display().to_string()
        } else {
            path.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        };
        let opaque = (depth > 0 && absolute <= MAX_OPAQUE_DEPTH)
            .then(|| path.to_str().and_then(|text| normalize_lexical(text).ok()))
            .flatten()
            .and_then(|normalized| self.classifier.opaque(&normalized));
        if opaque.is_some() {
            self.counters.opaque_dirs += 1;
        }
        self.nodes.push(TreeNode {
            parent: parent.map(|frame| frame.node),
            depth: u32::try_from(depth).unwrap_or(u32::MAX),
            name,
            size_bytes: 0,
            direct_bytes: 0,
            file_count: 0,
            dir_count: 0,
            access: Access::Complete,
            opaque,
        });
        self.nodes.len() - 1
    }

    /// El ancestro es una carpeta opaca (o ya esta dentro de una): no se crean nodos.
    fn inside_opaque(&self, parent: Option<Frame>) -> Option<usize> {
        parent
            .filter(|frame| frame.inside || self.nodes[frame.node].opaque.is_some())
            .map(|frame| frame.node)
    }
}

impl WalkVisitor for FolderAggregator<'_> {
    fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    fn file_seen(&mut self) {}

    fn candidate(&mut self, entry: DirEntry) {
        let Some(frame) = self.chain.last().copied() else {
            return;
        };
        // Un marcador de nube no se descarga ni suma bytes (D-06).
        if entry.meta.cloud_placeholder {
            self.counters.cloud_files += 1;
            return;
        }
        if entry.meta.kind != EntryKind::File {
            return;
        }
        let node = &mut self.nodes[frame.node];
        if !frame.inside && node.opaque.is_none() {
            let (path, modified) = (&entry.path, entry.meta.modified);
            self.top.offer(entry.meta.size_bytes, || {
                (path.display().to_string(), modified, frame.node)
            });
        }
        let node = &mut self.nodes[frame.node];
        node.size_bytes += entry.meta.size_bytes;
        node.file_count += 1;
        if !frame.inside {
            node.direct_bytes += entry.meta.size_bytes;
        }
    }

    fn reparse_point(&mut self, _path: PathBuf) {
        self.counters.reparse_skipped += 1;
    }

    fn dir_listed(&mut self, path: &Path) {
        let (depth, absolute, parent) = self.enter(path);
        if let Some(opaque) = self.inside_opaque(parent) {
            self.nodes[opaque].dir_count += 1;
            self.chain.push(Frame {
                node: opaque,
                inside: true,
            });
            return;
        }
        let node = self.new_node(path, depth, absolute, parent);
        self.chain.push(Frame {
            node,
            inside: false,
        });
    }

    fn dir_inaccessible(&mut self, path: &Path, error: &PortError) {
        self.counters.inaccessible_dirs += 1;
        let (depth, absolute, parent) = self.enter(path);
        if let Some(opaque) = self.inside_opaque(parent) {
            // Solo suma al contador: la ruta de una carpeta opaca nunca se guarda.
            let node = &mut self.nodes[opaque];
            node.dir_count += 1;
            if node.access == Access::Complete {
                node.access = Access::Incomplete;
            }
            return;
        }
        let node = self.new_node(path, depth, absolute, parent);
        self.nodes[node].access = Access::Inaccessible;
        if self.issues.len() < MAX_ISSUES {
            self.issues.push(FolderIssue {
                path: path.display().to_string(),
                reason: IssueReason::from_port_error(error),
            });
        }
    }
}

#[cfg(test)]
#[path = "aggregate_tests.rs"]
mod tests;

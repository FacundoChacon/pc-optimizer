//! Top de archivos grandes (C-07, design D-08): montículo acotado durante el recorrido.

use std::cmp::{Ordering, Reverse};
use std::collections::BinaryHeap;

use crate::domain::disk_usage::aggregate::FolderTree;
use crate::domain::model::EpochMillis;
use crate::domain::protection::path::fold_component;

/// Un archivo del top, con el nodo de la carpeta que lo contiene.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopFile {
    pub size_bytes: u64,
    pub path: String,
    pub modified: Option<EpochMillis>,
    /// Indice del nodo de su carpeta en el arbol (sin asociar a una carpeta guardada).
    pub node: usize,
}

/// `Ord`: mayor = mejor candidato (mas grande; a igual tamano, ruta plegada menor).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    file: TopFile,
    folded: String,
}

impl Ord for Entry {
    fn cmp(&self, other: &Self) -> Ordering {
        self.file
            .size_bytes
            .cmp(&other.file.size_bytes)
            .then_with(|| other.folded.cmp(&self.folded))
    }
}

impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Los `max` archivos mas grandes vistos.
#[derive(Debug)]
pub struct TopFiles {
    max: usize,
    heap: BinaryHeap<Reverse<Entry>>,
}

impl TopFiles {
    pub fn new(max: usize) -> Self {
        Self {
            max,
            heap: BinaryHeap::new(),
        }
    }

    /// Propone un archivo; `make` (ruta, fecha, nodo) solo se llama si puede entrar. Los
    /// archivos vacios no entran.
    pub fn offer(
        &mut self,
        size: u64,
        make: impl FnOnce() -> (String, Option<EpochMillis>, usize),
    ) {
        if size == 0 || self.max == 0 {
            return;
        }
        let full = self.heap.len() >= self.max;
        if full
            && self
                .heap
                .peek()
                .is_some_and(|worst| size < worst.0.file.size_bytes)
        {
            return;
        }
        let (path, modified, node) = make();
        let entry = Entry {
            folded: fold_component(&path),
            file: TopFile {
                size_bytes: size,
                path,
                modified,
                node,
            },
        };
        if !full {
            self.heap.push(Reverse(entry));
        } else if self.heap.peek().is_some_and(|worst| entry > worst.0) {
            self.heap.pop();
            self.heap.push(Reverse(entry));
        }
    }

    /// Tamano descendente; a igual tamano, ruta plegada ascendente.
    pub fn into_sorted(self) -> Vec<TopFile> {
        let mut entries: Vec<Entry> = self.heap.into_iter().map(|reverse| reverse.0).collect();
        entries.sort_by(|a, b| b.cmp(a));
        entries.into_iter().map(|entry| entry.file).collect()
    }
}

/// La carpeta guardada mas cercana a `node`: ella misma o su ancestro guardado mas proximo (la
/// raiz siempre esta guardada).
pub fn nearest_kept(tree: &FolderTree, node: usize, kept: &[bool]) -> usize {
    let mut current = node;
    while !kept.get(current).copied().unwrap_or(false) {
        match tree.nodes[current].parent {
            Some(parent) => current = parent,
            None => return current,
        }
    }
    current
}

#[cfg(test)]
#[path = "top_files_tests.rs"]
mod tests;

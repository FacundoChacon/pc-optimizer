//! Seleccion de las carpetas que se guardan (C-07, design D-07).
//!
//! Se guardan todas si caben; si no, las de mayor tamano con desempate por menor profundidad y
//! luego por ruta. Como una carpeta nunca es mas chica que una hija y, a igual tamano, el padre
//! tiene menor profundidad, el conjunto elegido es cerrado por ancestros.

use std::cmp::Ordering;

use crate::domain::disk_usage::aggregate::FolderTree;
use crate::domain::protection::path::fold_component;

/// Carpetas elegidas, por indice de nodo en orden de creacion (el padre antes que la hija).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    pub kept: Vec<usize>,
    pub truncated: bool,
}

/// Elige hasta `max` carpetas del arbol.
pub fn select_folders(tree: &FolderTree, max: usize) -> Selection {
    if tree.nodes.len() <= max {
        return Selection {
            kept: (0..tree.nodes.len()).collect(),
            truncated: false,
        };
    }
    let mut ranked: Vec<usize> = (0..tree.nodes.len()).collect();
    ranked.sort_by(|a, b| compare(tree, *a, *b));
    ranked.truncate(max);
    ranked.sort_unstable();
    Selection {
        kept: ranked,
        truncated: true,
    }
}

fn compare(tree: &FolderTree, a: usize, b: usize) -> Ordering {
    let (left, right) = (&tree.nodes[a], &tree.nodes[b]);
    right
        .size_bytes
        .cmp(&left.size_bytes)
        .then(left.depth.cmp(&right.depth))
        .then_with(|| fold_component(&tree.path_of(a)).cmp(&fold_component(&tree.path_of(b))))
}

/// Lo que no cubren los hijos guardados: total menos directo menos la suma de los hijos.
pub fn remainder_bytes(
    size: u64,
    direct: u64,
    stored_children: impl IntoIterator<Item = u64>,
) -> u64 {
    let children: u64 = stored_children.into_iter().sum();
    size.saturating_sub(direct).saturating_sub(children)
}

#[cfg(test)]
#[path = "select_tests.rs"]
mod tests;

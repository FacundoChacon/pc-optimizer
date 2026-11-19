use std::collections::HashSet;

use super::*;
use crate::domain::disk_usage::aggregate::{FolderTree, TreeNode};
use crate::domain::disk_usage::model::{Access, MAX_STORED_FOLDERS};

/// Generador determinista (LCG) para los arboles de las pruebas de propiedad.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self, bound: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) % bound
    }
}

fn node(parent: Option<usize>, depth: u32, name: &str, direct: u64) -> TreeNode {
    TreeNode {
        parent,
        depth,
        name: name.to_string(),
        size_bytes: direct,
        direct_bytes: direct,
        file_count: 0,
        dir_count: 0,
        access: Access::Complete,
        opaque: None,
    }
}

/// Arbol aleatorio con `count` carpetas: `direct` toma pocos valores distintos para forzar
/// empates entre padres, hijas y hermanas; las sumas se consolidan de abajo hacia arriba.
fn random_tree(count: usize, seed: u64, distinct_sizes: u64) -> FolderTree {
    let mut rng = Lcg(seed);
    let mut nodes = vec![node(None, 0, r"C:\r", rng.next(distinct_sizes))];
    for index in 1..count {
        let parent = usize::try_from(rng.next(index as u64)).unwrap_or(0);
        let depth = nodes[parent].depth + 1;
        let direct = rng.next(distinct_sizes) * 1024;
        nodes.push(node(Some(parent), depth, &format!("d{index}"), direct));
    }
    for index in (1..count).rev() {
        let (size, parent) = (nodes[index].size_bytes, nodes[index].parent);
        if let Some(parent) = parent {
            nodes[parent].size_bytes += size;
        }
    }
    FolderTree {
        nodes,
        ..FolderTree::default()
    }
}

fn assert_closed_by_ancestors(tree: &FolderTree, selection: &Selection) {
    let kept: HashSet<usize> = selection.kept.iter().copied().collect();
    assert!(kept.contains(&0), "la raiz siempre se guarda");
    for index in &selection.kept {
        if let Some(parent) = tree.nodes[*index].parent {
            assert!(
                kept.contains(&parent),
                "la carpeta {index} se guardo sin su padre {parent}"
            );
        }
    }
}

#[test]
fn select_should_keep_every_folder_when_they_fit() {
    let tree = random_tree(121, 7, 5);
    let selection = select_folders(&tree, MAX_STORED_FOLDERS);
    assert_eq!(selection.kept.len(), 121);
    assert!(!selection.truncated);
    assert_eq!(selection.kept, (0..121).collect::<Vec<_>>());
}

#[test]
fn select_should_keep_exactly_the_limit_and_report_truncation() {
    let tree = random_tree(30_000, 42, 50);
    let selection = select_folders(&tree, 1_000);
    assert_eq!(selection.kept.len(), 1_000);
    assert!(selection.truncated);
    assert_closed_by_ancestors(&tree, &selection);
}

#[test]
fn select_should_be_closed_by_ancestors_across_trees_with_many_size_ties() {
    for (seed, count, distinct) in [
        (1, 3_000, 1),
        (2, 3_000, 2),
        (3, 5_000, 3),
        (4, 2_000, 1_000),
    ] {
        let tree = random_tree(count, seed, distinct);
        for max in [1, 2, 10, 333, 1_999] {
            let selection = select_folders(&tree, max);
            assert_eq!(selection.kept.len(), max, "seed {seed} max {max}");
            assert_closed_by_ancestors(&tree, &selection);
        }
    }
}

#[test]
fn select_should_prefer_bigger_folders_then_shallower_then_path_order() {
    // Raiz 0 con hijas de distintos tamanos; `big` (30) > `mid` (20) > `small` (10).
    let mut nodes = vec![node(None, 0, r"C:\r", 0)];
    nodes.push(node(Some(0), 1, "small", 10));
    nodes.push(node(Some(0), 1, "big", 30));
    nodes.push(node(Some(0), 1, "mid", 20));
    // Empate de tamano y profundidad: gana la ruta menor (b antes que c).
    nodes.push(node(Some(0), 1, "c", 5));
    nodes.push(node(Some(0), 1, "b", 5));
    nodes[0].size_bytes = 70;
    let tree = FolderTree {
        nodes,
        ..FolderTree::default()
    };
    let names = |max: usize| -> Vec<String> {
        select_folders(&tree, max)
            .kept
            .iter()
            .map(|i| tree.nodes[*i].name.clone())
            .collect()
    };
    assert_eq!(names(3), vec![r"C:\r", "big", "mid"]);
    assert_eq!(names(5), vec![r"C:\r", "small", "big", "mid", "b"]);
}

#[test]
fn select_should_keep_a_parent_before_its_equal_sized_child() {
    // Cadena r -> a -> b, los tres de 100 bytes (solo b tiene archivos): el padre gana el empate.
    let mut nodes = vec![node(None, 0, r"C:\r", 0)];
    nodes.push(node(Some(0), 1, "a", 0));
    nodes.push(node(Some(1), 2, "b", 100));
    nodes[0].size_bytes = 100;
    nodes[1].size_bytes = 100;
    let tree = FolderTree {
        nodes,
        ..FolderTree::default()
    };
    assert_eq!(select_folders(&tree, 2).kept, vec![0, 1]);
}

#[test]
fn remainder_should_be_total_minus_direct_minus_stored_children() {
    assert_eq!(remainder_bytes(100, 10, [30, 20]), 40);
    assert_eq!(remainder_bytes(100, 10, []), 90);
    assert_eq!(remainder_bytes(100, 100, []), 0);
    assert_eq!(remainder_bytes(50, 10, [40]), 0);
}

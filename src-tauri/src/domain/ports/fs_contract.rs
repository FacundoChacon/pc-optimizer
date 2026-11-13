//! Bateria de contrato del puerto `FileSystem` (solo tests).
//!
//! Se ejecuta contra `FakeFileSystem` y contra el adaptador real de Windows sobre un `TempDir`,
//! para que el doble y la realidad no diverjan. Cada `contract_*` recibe un `Scenario` ya armado.

use std::path::PathBuf;

use crate::domain::ports::{EntryKind, FileSystem, PortError};

/// Disposicion estandar que cada implementacion debe construir bajo `root`:
///
/// - `file` (5 bytes), `dir` (con `dir_child`), `target` (con `target_child`),
/// - `junction` -> `target` (reparse point),
/// - `hard_a` y `hard_b` (dos nombres del mismo archivo),
/// - `missing` (no existe).
pub(crate) struct Scenario<F> {
    pub fs: F,
    pub root: PathBuf,
    pub file: PathBuf,
    pub dir: PathBuf,
    pub dir_child: PathBuf,
    pub target: PathBuf,
    pub target_child: PathBuf,
    pub junction: PathBuf,
    pub hard_a: PathBuf,
    pub hard_b: PathBuf,
    pub missing: PathBuf,
}

pub(crate) fn contract_metadata_of_a_file<F: FileSystem>(s: &Scenario<F>) {
    let meta = s.fs.metadata(&s.file).unwrap();
    assert_eq!(meta.kind, EntryKind::File);
    assert_eq!(meta.size_bytes, 5);
    assert!(!meta.cloud_placeholder);
    assert!(meta.identity.is_some());
    assert_eq!(meta.hard_links, Some(1));
}

pub(crate) fn contract_metadata_of_a_directory<F: FileSystem>(s: &Scenario<F>) {
    let meta = s.fs.metadata(&s.dir).unwrap();
    assert_eq!(meta.kind, EntryKind::Directory);
    assert!(meta.identity.is_some());
}

pub(crate) fn contract_metadata_does_not_follow_a_junction<F: FileSystem>(s: &Scenario<F>) {
    let link = s.fs.metadata(&s.junction).unwrap();
    let target = s.fs.metadata(&s.target).unwrap();
    assert_eq!(link.kind, EntryKind::ReparsePoint);
    assert!(link.identity.is_some());
    assert_ne!(link.identity, target.identity);
}

pub(crate) fn contract_identity_is_stable_and_distinct<F: FileSystem>(s: &Scenario<F>) {
    let first = s.fs.metadata(&s.file).unwrap().identity;
    let second = s.fs.metadata(&s.file).unwrap().identity;
    let other = s.fs.metadata(&s.dir).unwrap().identity;
    assert_eq!(first, second);
    assert_ne!(first, other);
}

pub(crate) fn contract_hard_links_share_identity_and_count_two<F: FileSystem>(s: &Scenario<F>) {
    let a = s.fs.metadata(&s.hard_a).unwrap();
    let b = s.fs.metadata(&s.hard_b).unwrap();
    assert_eq!(a.hard_links, Some(2));
    assert_eq!(b.hard_links, Some(2));
    assert_eq!(a.identity, b.identity);
}

pub(crate) fn contract_missing_path_is_not_found<F: FileSystem>(s: &Scenario<F>) {
    assert!(matches!(
        s.fs.metadata(&s.missing),
        Err(PortError::NotFound(_))
    ));
    assert!(matches!(
        s.fs.canonicalize(&s.missing),
        Err(PortError::NotFound(_))
    ));
}

pub(crate) fn contract_read_dir_lists_direct_children_without_following_junctions<F: FileSystem>(
    s: &Scenario<F>,
) {
    let children = s.fs.read_dir(&s.root).unwrap();
    let paths: Vec<&PathBuf> = children.iter().map(|c| &c.path).collect();
    for expected in [&s.file, &s.dir, &s.target, &s.junction, &s.hard_a] {
        assert!(paths.contains(&expected), "falta {expected:?} en {paths:?}");
    }
    assert!(!paths.contains(&&s.dir_child), "no es hijo directo");
    assert!(!paths.contains(&&s.target_child), "no es hijo directo");
    let junction = children.iter().find(|c| c.path == s.junction).unwrap();
    assert_eq!(junction.meta.kind, EntryKind::ReparsePoint);
    assert!(children.iter().all(|c| c.meta.identity.is_none()));
    // El contenido de la junction no se expone: el destino se lista por su propia ruta.
    assert!(s.fs.read_dir(&s.junction).unwrap().is_empty());
    assert_eq!(s.fs.read_dir(&s.target).unwrap().len(), 1);
}

pub(crate) fn contract_canonicalize_does_not_follow_the_last_component<F: FileSystem>(
    s: &Scenario<F>,
) {
    assert_eq!(s.fs.canonicalize(&s.junction).unwrap(), s.junction);
    assert_eq!(s.fs.canonicalize(&s.file).unwrap(), s.file);
}

pub(crate) fn contract_canonicalize_resolves_intermediate_junctions<F: FileSystem>(
    s: &Scenario<F>,
) {
    let through = s.junction.join("secret.txt");
    assert_eq!(s.fs.canonicalize(&through).unwrap(), s.target_child);
}

/// Genera un `#[test]` por caso del contrato, con el escenario que arma `$factory`.
macro_rules! fs_contract_tests {
    ($factory:expr) => {
        $crate::domain::ports::fs_contract::fs_contract_tests!(@one $factory, contract_metadata_of_a_file);
        $crate::domain::ports::fs_contract::fs_contract_tests!(@one $factory, contract_metadata_of_a_directory);
        $crate::domain::ports::fs_contract::fs_contract_tests!(@one $factory, contract_metadata_does_not_follow_a_junction);
        $crate::domain::ports::fs_contract::fs_contract_tests!(@one $factory, contract_identity_is_stable_and_distinct);
        $crate::domain::ports::fs_contract::fs_contract_tests!(@one $factory, contract_hard_links_share_identity_and_count_two);
        $crate::domain::ports::fs_contract::fs_contract_tests!(@one $factory, contract_missing_path_is_not_found);
        $crate::domain::ports::fs_contract::fs_contract_tests!(
            @one $factory,
            contract_read_dir_lists_direct_children_without_following_junctions
        );
        $crate::domain::ports::fs_contract::fs_contract_tests!(
            @one $factory,
            contract_canonicalize_does_not_follow_the_last_component
        );
        $crate::domain::ports::fs_contract::fs_contract_tests!(
            @one $factory,
            contract_canonicalize_resolves_intermediate_junctions
        );
    };
    (@one $factory:expr, $case:ident) => {
        #[test]
        fn $case() {
            let scenario = $factory;
            $crate::domain::ports::fs_contract::$case(&scenario);
        }
    };
}
pub(crate) use fs_contract_tests;

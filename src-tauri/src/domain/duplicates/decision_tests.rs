use super::*;
use crate::domain::duplicates::model::DuplicateFile;
use crate::domain::model::EpochMillis;
use crate::domain::ports::FileIdentity;
use crate::domain::protection::context::ContentFolder;

fn file(id: i64, path: &str) -> StoredDuplicateFile {
    StoredDuplicateFile {
        id,
        file: DuplicateFile {
            root: ContentFolder::Downloads,
            path: path.to_string(),
            modified: Some(EpochMillis(1_000)),
            identity: FileIdentity {
                volume_serial: 1,
                file_id: id as u128,
            },
        },
        kept: false,
    }
}

fn group(id: i64, file_ids: &[i64]) -> StoredDuplicateGroup {
    StoredDuplicateGroup {
        id,
        size_bytes: 4 << 20,
        blake3: "ab".repeat(32),
        resolved_at: None,
        files: file_ids
            .iter()
            .map(|file_id| file(*file_id, &format!(r"C:\Users\ana\Downloads\c{file_id}.bin")))
            .collect(),
    }
}

fn decide(group_id: i64, keep: &[i64]) -> Decision {
    Decision {
        group_id,
        keep_file_ids: keep.to_vec(),
    }
}

fn groups() -> Vec<StoredDuplicateGroup> {
    vec![group(4, &[41, 42, 43]), group(9, &[91, 92])]
}

#[test]
fn validate_should_derive_the_copies_to_delete_from_the_ones_kept() {
    let groups = groups();
    let validated = validate_decisions(&[decide(4, &[42])], &groups).unwrap();
    assert_eq!(validated.len(), 1);
    assert_eq!(validated[0].group.id, 4);
    assert_eq!(
        validated[0].kept.iter().map(|f| f.id).collect::<Vec<_>>(),
        [42]
    );
    assert_eq!(
        validated[0].delete.iter().map(|f| f.id).collect::<Vec<_>>(),
        [41, 43]
    );
}

#[test]
fn validate_should_accept_keeping_two_of_three_and_leave_a_single_copy_to_delete() {
    let groups = groups();
    let validated = validate_decisions(&[decide(4, &[43, 41])], &groups).unwrap();
    assert_eq!(
        validated[0].kept.iter().map(|f| f.id).collect::<Vec<_>>(),
        [41, 43],
        "en el orden del grupo, no de la decision"
    );
    assert_eq!(
        validated[0].delete.iter().map(|f| f.id).collect::<Vec<_>>(),
        [42]
    );
}

#[test]
fn validate_should_keep_the_order_of_the_decisions() {
    let groups = groups();
    let validated = validate_decisions(&[decide(9, &[91]), decide(4, &[41])], &groups).unwrap();
    assert_eq!(
        validated.iter().map(|v| v.group.id).collect::<Vec<_>>(),
        [9, 4]
    );
}

#[test]
fn validate_should_require_at_least_one_kept_copy() {
    let groups = groups();
    assert_eq!(
        validate_decisions(&[decide(4, &[])], &groups).err(),
        Some(DecisionError::KeepRequired)
    );
    // Aunque otro grupo de la lista este bien.
    assert_eq!(
        validate_decisions(&[decide(9, &[91]), decide(4, &[])], &groups).err(),
        Some(DecisionError::KeepRequired)
    );
}

#[test]
fn validate_should_reject_keeping_every_copy() {
    let groups = groups();
    for keep in [vec![41, 42, 43], vec![43, 42, 41]] {
        assert!(
            matches!(
                validate_decisions(&[decide(4, &keep)], &groups),
                Err(DecisionError::InvalidArgument(_))
            ),
            "{keep:?}"
        );
    }
    assert!(matches!(
        validate_decisions(&[decide(9, &[91, 92])], &groups),
        Err(DecisionError::InvalidArgument(_))
    ));
}

#[test]
fn validate_should_reject_a_copy_of_another_group_or_a_repeated_copy() {
    let groups = groups();
    // La copia 91 es del grupo 9, no del 4.
    assert!(matches!(
        validate_decisions(&[decide(4, &[91])], &groups),
        Err(DecisionError::InvalidArgument(_))
    ));
    assert!(matches!(
        validate_decisions(&[decide(4, &[41, 41])], &groups),
        Err(DecisionError::InvalidArgument(_))
    ));
    assert!(matches!(
        validate_decisions(&[decide(4, &[999])], &groups),
        Err(DecisionError::InvalidArgument(_))
    ));
}

#[test]
fn validate_should_reject_empty_decisions_and_a_repeated_group() {
    let groups = groups();
    assert!(matches!(
        validate_decisions(&[], &groups),
        Err(DecisionError::InvalidArgument(_))
    ));
    assert!(matches!(
        validate_decisions(&[decide(4, &[41]), decide(4, &[42])], &groups),
        Err(DecisionError::InvalidArgument(_))
    ));
}

#[test]
fn validate_should_reject_ids_outside_the_js_safe_range() {
    let groups = groups();
    for bad in [-1, MAX_SAFE_ID + 1, i64::MAX, i64::MIN] {
        assert!(
            matches!(
                validate_decisions(&[decide(bad, &[41])], &groups),
                Err(DecisionError::InvalidArgument(_))
            ),
            "grupo {bad}"
        );
        assert!(
            matches!(
                validate_decisions(&[decide(4, &[bad])], &groups),
                Err(DecisionError::InvalidArgument(_))
            ),
            "copia {bad}"
        );
    }
    // El mayor id seguro es valido (y entonces no existe).
    assert_eq!(
        validate_decisions(&[decide(MAX_SAFE_ID, &[41])], &groups).err(),
        Some(DecisionError::GroupNotFound)
    );
}

#[test]
fn validate_should_report_an_unknown_group_and_a_resolved_one() {
    let mut groups = groups();
    assert_eq!(
        validate_decisions(&[decide(77, &[41])], &groups).err(),
        Some(DecisionError::GroupNotFound)
    );
    groups[1].resolved_at = Some(EpochMillis(5_000));
    assert_eq!(
        validate_decisions(&[decide(9, &[91])], &groups).err(),
        Some(DecisionError::GroupResolved)
    );
    // El resuelto se informa antes que cualquier otro defecto de la decision.
    assert_eq!(
        validate_decisions(&[decide(9, &[])], &groups).err(),
        Some(DecisionError::GroupResolved)
    );
}

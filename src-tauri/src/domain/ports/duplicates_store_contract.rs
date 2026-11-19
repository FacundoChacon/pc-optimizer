//! Bateria de contrato de `DuplicateStore` (solo tests): corre contra `InMemoryStore` y contra
//! `SqliteStore` sobre un archivo temporal.

use crate::domain::duplicates::model::{
    DuplicateFile, DuplicateGroup, DuplicateScanResults, GroupResolution, MinSize, RootReason,
    RootResult, RootStatus, ScanCounters,
};
use crate::domain::junk::category::JunkCategory;
use crate::domain::junk::model::{CategoryResult, CategoryStatus, JunkScanResults};
use crate::domain::model::{
    EpochMillis, NewScan, SCAN_RETENTION, ScanId, ScanKind, ScanStatus, ScanTrigger,
};
use crate::domain::ports::FileIdentity;
use crate::domain::ports::store::{Store, StoreError};
use crate::domain::protection::context::ContentFolder;

const MIB: u64 = 1 << 20;

pub(crate) fn dup_scan<S: Store>(store: &S, started: i64) -> ScanId {
    store
        .create_scan(&NewScan {
            kind: ScanKind::Duplicates,
            trigger: ScanTrigger::Manual,
            started_at: EpochMillis(started),
        })
        .unwrap()
}

fn hash(n: u8) -> String {
    format!("{n:02x}").repeat(32)
}

fn file(root: ContentFolder, path: &str, n: u128) -> DuplicateFile {
    DuplicateFile {
        root,
        path: path.to_string(),
        modified: Some(EpochMillis(1_700_000_000_000 + n as i64)),
        identity: FileIdentity {
            // Bit alto encendido: debe sobrevivir al viaje por una columna con signo.
            volume_serial: 0x8000_0000_dead_beef,
            file_id: 0xfedc_ba98_7654_3210_0123_4567_89ab_cdef ^ n,
        },
    }
}

/// Grupo de `copies` copias de `size` bytes en `Downloads`, con rutas `base_<i>`.
pub(crate) fn group(size: u64, tag: u8, base: &str, copies: usize, seed: u128) -> DuplicateGroup {
    DuplicateGroup {
        size_bytes: size,
        blake3: hash(tag),
        files: (0..copies)
            .map(|i| {
                file(
                    ContentFolder::Downloads,
                    &format!(r"C:\Users\ana\Downloads\{base}_{i}.bin"),
                    seed + i as u128,
                )
            })
            .collect(),
    }
}

pub(crate) fn roots() -> Vec<RootResult> {
    ContentFolder::ALL
        .into_iter()
        .map(|folder| RootResult {
            folder,
            status: match folder {
                ContentFolder::Downloads => RootStatus::Done,
                ContentFolder::Videos => RootStatus::Unavailable(RootReason::UnsupportedVolume),
                ContentFolder::Documents => RootStatus::Nested,
                _ => RootStatus::NotSelected,
            },
            files_seen: if folder == ContentFolder::Downloads {
                7
            } else {
                0
            },
        })
        .collect()
}

pub(crate) fn counters() -> ScanCounters {
    ScanCounters {
        files_seen: 40,
        below_min_size: 11,
        protected_count: 2,
        cloud_count: 3,
        hard_linked_count: 4,
        changed_count: 1,
        unreadable_count: 5,
        inaccessible_dirs: 6,
        excluded_dirs: 7,
        hashed_bytes: 123_456_789,
    }
}

pub(crate) fn results(groups: Vec<DuplicateGroup>) -> DuplicateScanResults {
    DuplicateScanResults {
        min_size: MinSize::MIB_10,
        roots: roots(),
        counters: counters(),
        groups,
    }
}

fn save<S: Store>(store: &S, scan: ScanId, groups: Vec<DuplicateGroup>) {
    store
        .complete_duplicate_scan(scan, &results(groups), EpochMillis(9_000))
        .unwrap();
}

fn ids<S: Store>(store: &S, scan: ScanId) -> Vec<i64> {
    store
        .list_duplicate_groups(scan, 0, 500, None, None)
        .unwrap()
        .groups
        .iter()
        .map(|g| g.id)
        .collect()
}

pub(crate) fn contract_round_trip<S: Store>(make: impl Fn() -> S) {
    let store = make();
    assert_eq!(store.last_done_duplicate_scan().unwrap(), None);
    let scan = dup_scan(&store, 1_000);
    save(
        &store,
        scan,
        vec![
            group(4 * MIB, 1, "a", 2, 100),
            group(10 * MIB, 2, "b", 3, 200),
        ],
    );
    let summary = store.last_done_duplicate_scan().unwrap().unwrap();
    assert_eq!(summary.scan.id, scan);
    assert_eq!(summary.scan.status, ScanStatus::Done);
    assert_eq!(summary.scan.finished_at, Some(EpochMillis(9_000)));
    assert_eq!(summary.min_size, MinSize::MIB_10);
    assert_eq!(summary.roots, roots());
    assert_eq!(summary.counters, counters());
    assert_eq!(summary.group_count, 2);
    assert_eq!(summary.duplicate_file_count, 5);
    assert_eq!(summary.reclaimable_bytes, 4 * MIB + 20 * MIB);
    assert_eq!(summary.open_group_count, 2);

    let page = store
        .list_duplicate_groups(scan, 0, 10, None, None)
        .unwrap();
    assert_eq!(page.total, 2);
    // 20 MiB recuperables antes que 4 MiB.
    assert_eq!(
        page.groups.iter().map(|g| g.size_bytes).collect::<Vec<_>>(),
        [10 * MIB, 4 * MIB]
    );
    let big = &page.groups[0];
    assert_eq!(big.blake3, hash(2));
    assert_eq!(big.resolved_at, None);
    assert_eq!(big.files.len(), 3);
    // Ningun grupo recien guardado trae una copia elegida (RN-DUP-03).
    assert!(page.groups.iter().flat_map(|g| &g.files).all(|f| !f.kept));
    // Las copias conservan ruta, fecha e identidad (incluidos los bits altos) y el orden.
    assert_eq!(
        big.files.iter().map(|f| f.file.clone()).collect::<Vec<_>>(),
        group(10 * MIB, 2, "b", 3, 200).files
    );
}

pub(crate) fn contract_failed_save_is_atomic<S: Store>(make: impl Fn() -> S) {
    let store = make();
    let first = dup_scan(&store, 1_000);
    save(&store, first, vec![group(4 * MIB, 1, "a", 2, 100)]);
    let before = ids(&store, first);

    let second = dup_scan(&store, 2_000);
    // Un grupo de una sola copia: no se guarda nada del escaneo nuevo.
    let single = DuplicateGroup {
        files: group(4 * MIB, 3, "x", 1, 300).files,
        ..group(4 * MIB, 3, "x", 1, 300)
    };
    let error = store
        .complete_duplicate_scan(
            second,
            &results(vec![group(5 * MIB, 4, "ok", 2, 400), single]),
            EpochMillis(9_000),
        )
        .unwrap_err();
    assert!(matches!(error, StoreError::Constraint(_)), "{error:?}");
    assert_eq!(
        store.get_scan(second).unwrap().unwrap().status,
        ScanStatus::Running
    );
    assert!(ids(&store, second).is_empty());
    // Los grupos del escaneo anterior siguen intactos.
    assert_eq!(ids(&store, first), before);
    assert_eq!(
        store.last_done_duplicate_scan().unwrap().unwrap().scan.id,
        first
    );
}

pub(crate) fn contract_invalid_results_are_rejected<S: Store>(make: impl Fn() -> S) {
    let store = make();
    let scan = dup_scan(&store, 1_000);
    // Dos copias con la misma identidad.
    let mut same_identity = group(4 * MIB, 1, "a", 2, 100);
    same_identity.files[1].identity = same_identity.files[0].identity;
    // Hash de 10 caracteres, tamano 0 y grupo repetido.
    let mut short_hash = group(4 * MIB, 1, "a", 2, 100);
    short_hash.blake3 = "abcdef0123".into();
    let mut upper_hash = group(4 * MIB, 1, "a", 2, 100);
    upper_hash.blake3 = "A".repeat(64);
    let zero_size = group(0, 1, "a", 2, 100);
    for (name, groups) in [
        ("identidad repetida", vec![same_identity]),
        ("hash corto", vec![short_hash]),
        ("hash en mayusculas", vec![upper_hash]),
        ("tamano cero", vec![zero_size]),
        (
            "grupo repetido",
            vec![
                group(4 * MIB, 1, "a", 2, 100),
                group(4 * MIB, 1, "b", 2, 200),
            ],
        ),
    ] {
        let error = store
            .complete_duplicate_scan(scan, &results(groups), EpochMillis(9_000))
            .unwrap_err();
        assert!(
            matches!(error, StoreError::Constraint(_)),
            "{name}: {error:?}"
        );
        assert_eq!(
            store.get_scan(scan).unwrap().unwrap().status,
            ScanStatus::Running,
            "{name}"
        );
    }
    // Y un valido sigue pudiendo guardarse despues de tantos rechazos.
    save(&store, scan, vec![group(4 * MIB, 1, "a", 2, 100)]);
}

pub(crate) fn contract_complete_requires_a_running_duplicates_scan<S: Store>(make: impl Fn() -> S) {
    let store = make();
    let valid = results(vec![group(4 * MIB, 1, "a", 2, 100)]);
    assert!(matches!(
        store.complete_duplicate_scan(ScanId(99), &valid, EpochMillis(1)),
        Err(StoreError::NotFound(_))
    ));
    let junk = store
        .create_scan(&NewScan {
            kind: ScanKind::Junk,
            trigger: ScanTrigger::Manual,
            started_at: EpochMillis(1_000),
        })
        .unwrap();
    assert!(matches!(
        store.complete_duplicate_scan(junk, &valid, EpochMillis(1_001)),
        Err(StoreError::InvalidState(_))
    ));
    let done = dup_scan(&store, 2_000);
    store
        .finish_scan(done, ScanStatus::Cancelled, EpochMillis(2_001))
        .unwrap();
    assert!(matches!(
        store.complete_duplicate_scan(done, &valid, EpochMillis(2_002)),
        Err(StoreError::InvalidState(_))
    ));
    assert_eq!(store.last_done_duplicate_scan().unwrap(), None);
}

pub(crate) fn contract_second_scan_purges_previous_groups_keeping_the_scan<S: Store>(
    make: impl Fn() -> S,
) {
    let store = make();
    let first = dup_scan(&store, 1_000);
    save(
        &store,
        first,
        (0..10)
            .map(|i| group(2 * MIB, i as u8 + 10, &format!("g{i}"), 2, 1_000 * (i + 1)))
            .collect(),
    );
    assert_eq!(ids(&store, first).len(), 10);

    let second = dup_scan(&store, 2_000);
    save(
        &store,
        second,
        (0..4)
            .map(|i| {
                group(
                    3 * MIB,
                    i as u8 + 50,
                    &format!("h{i}"),
                    2,
                    100_000 * (i + 1),
                )
            })
            .collect(),
    );
    assert_eq!(ids(&store, second).len(), 4);
    assert!(ids(&store, first).is_empty());
    // Ambos escaneos siguen listados y el ultimo terminado es el nuevo.
    assert_eq!(store.list_scans().unwrap().len(), 2);
    let summary = store.last_done_duplicate_scan().unwrap().unwrap();
    assert_eq!(summary.scan.id, second);
    assert_eq!(summary.group_count, 4);
}

pub(crate) fn contract_junk_findings_are_untouched_by_a_duplicates_scan<S: Store>(
    make: impl Fn() -> S,
) {
    let store = make();
    let junk = store
        .create_scan(&NewScan {
            kind: ScanKind::Junk,
            trigger: ScanTrigger::Manual,
            started_at: EpochMillis(500),
        })
        .unwrap();
    let junk_results = JunkScanResults {
        findings: vec![crate::domain::junk::model::JunkFinding {
            category: JunkCategory::TempUser,
            path: r"C:\t\a".into(),
            size_bytes: 5,
            modified_at: None,
            state: crate::domain::junk::model::FindingState::Eligible(FileIdentity {
                volume_serial: 1,
                file_id: 1,
            }),
        }],
        categories: JunkCategory::ALL
            .into_iter()
            .map(|category| CategoryResult::empty(category, CategoryStatus::Done, None))
            .collect(),
    };
    store
        .complete_junk_scan(junk, &junk_results, EpochMillis(501))
        .unwrap();
    let scan = dup_scan(&store, 1_000);
    save(&store, scan, vec![group(4 * MIB, 1, "a", 2, 100)]);
    let page = store
        .list_junk_findings(junk, JunkCategory::TempUser, 0, 10)
        .unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(store.last_done_junk_scan().unwrap().unwrap().scan.id, junk);
}

pub(crate) fn contract_last_done_ignores_unfinished_and_other_kinds<S: Store>(
    make: impl Fn() -> S,
) {
    let store = make();
    assert_eq!(store.last_done_duplicate_scan().unwrap(), None);
    // Un escaneo de basura terminado no cuenta.
    let junk = store
        .create_scan(&NewScan {
            kind: ScanKind::Junk,
            trigger: ScanTrigger::Manual,
            started_at: EpochMillis(100),
        })
        .unwrap();
    store
        .finish_scan(junk, ScanStatus::Done, EpochMillis(101))
        .unwrap();
    assert_eq!(store.last_done_duplicate_scan().unwrap(), None);
    // Cancelado y con error tampoco.
    let cancelled = dup_scan(&store, 200);
    store
        .finish_scan(cancelled, ScanStatus::Cancelled, EpochMillis(201))
        .unwrap();
    let errored = dup_scan(&store, 300);
    store
        .finish_scan(errored, ScanStatus::Error, EpochMillis(301))
        .unwrap();
    assert_eq!(store.last_done_duplicate_scan().unwrap(), None);
    let done = dup_scan(&store, 400);
    save(&store, done, vec![group(4 * MIB, 1, "a", 2, 100)]);
    // Uno posterior que falla no tapa al ultimo terminado.
    let later = dup_scan(&store, 500);
    store
        .finish_scan(later, ScanStatus::Error, EpochMillis(501))
        .unwrap();
    assert_eq!(
        store.last_done_duplicate_scan().unwrap().unwrap().scan.id,
        done
    );
}

pub(crate) fn contract_groups_order_by_reclaimable_bytes_then_id<S: Store>(make: impl Fn() -> S) {
    let store = make();
    let scan = dup_scan(&store, 1_000);
    save(
        &store,
        scan,
        vec![
            group(50 * MIB, 1, "fifty", 2, 100),
            group(20 * MIB, 2, "twenty", 4, 200),
            group(10 * MIB, 3, "tie_a", 2, 300),
            group(5 * MIB, 4, "tie_b", 3, 400),
        ],
    );
    let page = store
        .list_duplicate_groups(scan, 0, 50, None, None)
        .unwrap();
    // 60 MiB, 50 MiB y luego el empate de 10 MiB por id (el de 10 MiB se guardo antes).
    assert_eq!(
        page.groups.iter().map(|g| g.size_bytes).collect::<Vec<_>>(),
        [20 * MIB, 50 * MIB, 10 * MIB, 5 * MIB]
    );
    assert_eq!(
        page.groups
            .iter()
            .map(StoredGroupExt::reclaimable)
            .collect::<Vec<_>>(),
        [60 * MIB, 50 * MIB, 10 * MIB, 10 * MIB]
    );
}

trait StoredGroupExt {
    fn reclaimable(&self) -> u64;
}

impl StoredGroupExt for crate::domain::duplicates::model::StoredDuplicateGroup {
    fn reclaimable(&self) -> u64 {
        self.reclaimable_bytes()
    }
}

pub(crate) fn contract_filters_and_pagination<S: Store>(make: impl Fn() -> S) {
    let store = make();
    let scan = dup_scan(&store, 1_000);
    let mut pictures = group(2 * MIB, 1, "pic", 2, 100);
    pictures.files[1].root = ContentFolder::Pictures;
    let mut only_music = group(8 * MIB, 2, "mus", 2, 200);
    for f in &mut only_music.files {
        f.root = ContentFolder::Music;
    }
    save(
        &store,
        scan,
        vec![
            pictures,
            only_music,
            group(30 * MIB, 3, "dl", 2, 300),
            group(MIB, 4, "small", 2, 400),
        ],
    );
    let by_root = |root| {
        store
            .list_duplicate_groups(scan, 0, 50, Some(root), None)
            .unwrap()
    };
    // Un grupo cuenta si tiene AL MENOS una copia en la carpeta.
    assert_eq!(by_root(ContentFolder::Pictures).total, 1);
    assert_eq!(by_root(ContentFolder::Music).total, 1);
    assert_eq!(by_root(ContentFolder::Downloads).total, 3);
    assert_eq!(by_root(ContentFolder::Videos).total, 0);
    // El grupo filtrado conserva todas sus copias (tambien la de otra carpeta).
    assert_eq!(by_root(ContentFolder::Pictures).groups[0].files.len(), 2);
    let by_size = store
        .list_duplicate_groups(scan, 0, 50, None, Some(8 * MIB))
        .unwrap();
    assert_eq!(by_size.total, 2);
    assert_eq!(
        by_size
            .groups
            .iter()
            .map(|g| g.size_bytes)
            .collect::<Vec<_>>(),
        [30 * MIB, 8 * MIB]
    );
    let both = store
        .list_duplicate_groups(scan, 0, 50, Some(ContentFolder::Downloads), Some(30 * MIB))
        .unwrap();
    assert_eq!(both.total, 1);

    // Paginacion: el total es el filtrado y no el de la pagina.
    let first = store.list_duplicate_groups(scan, 0, 2, None, None).unwrap();
    let second = store.list_duplicate_groups(scan, 2, 2, None, None).unwrap();
    assert_eq!((first.total, first.groups.len()), (4, 2));
    assert_eq!((second.total, second.groups.len()), (4, 2));
    assert_ne!(first.groups[0].id, second.groups[0].id);
    // Fuera de rango y escaneo desconocido: vacio, con el total correcto.
    let beyond = store
        .list_duplicate_groups(scan, 500, 10, None, None)
        .unwrap();
    assert_eq!((beyond.total, beyond.groups.len()), (4, 0));
    let unknown = store
        .list_duplicate_groups(ScanId(404), 0, 10, None, None)
        .unwrap();
    assert_eq!((unknown.total, unknown.groups.len()), (0, 0));
}

pub(crate) fn contract_get_groups_only_returns_those_of_the_scan<S: Store>(make: impl Fn() -> S) {
    let store = make();
    let first = dup_scan(&store, 1_000);
    save(&store, first, vec![group(4 * MIB, 1, "a", 2, 100)]);
    let first_ids = ids(&store, first);
    let second = dup_scan(&store, 2_000);
    save(&store, second, vec![group(5 * MIB, 2, "b", 2, 200)]);
    let second_ids = ids(&store, second);
    // El grupo del primer escaneo ya no existe (se purgo): solo aparece el del segundo.
    assert!(
        store
            .get_duplicate_groups(first, &first_ids)
            .unwrap()
            .is_empty()
    );
    let mine = store
        .get_duplicate_groups(second, &[second_ids[0], 9_999, first_ids[0]])
        .unwrap();
    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0].id, second_ids[0]);
    assert_eq!(mine[0].files.len(), 2);
    assert!(store.get_duplicate_groups(second, &[]).unwrap().is_empty());
}

fn group_file_ids<S: Store>(store: &S, scan: ScanId, group_id: i64) -> Vec<i64> {
    store.get_duplicate_groups(scan, &[group_id]).unwrap()[0]
        .files
        .iter()
        .map(|f| f.id)
        .collect()
}

pub(crate) fn contract_resolve_marks_only_the_kept_copies<S: Store>(make: impl Fn() -> S) {
    let store = make();
    let scan = dup_scan(&store, 1_000);
    save(
        &store,
        scan,
        vec![
            group(4 * MIB, 1, "a", 2, 100),
            group(10 * MIB, 2, "b", 3, 200),
            group(6 * MIB, 3, "c", 2, 300),
        ],
    );
    let all = store
        .list_duplicate_groups(scan, 0, 10, None, None)
        .unwrap();
    let find = |size: u64| all.groups.iter().find(|g| g.size_bytes == size).unwrap();
    let (a, b, c) = (find(4 * MIB), find(10 * MIB), find(6 * MIB));
    store
        .resolve_duplicate_groups(
            scan,
            &[
                GroupResolution {
                    group_id: a.id,
                    kept_file_ids: vec![a.files[1].id],
                },
                GroupResolution {
                    group_id: b.id,
                    kept_file_ids: vec![b.files[0].id, b.files[2].id],
                },
            ],
            EpochMillis(5_000),
        )
        .unwrap();
    let after = store
        .get_duplicate_groups(scan, &[a.id, b.id, c.id])
        .unwrap();
    assert_eq!(after[0].resolved_at, Some(EpochMillis(5_000)));
    assert_eq!(
        after[0].files.iter().map(|f| f.kept).collect::<Vec<_>>(),
        [false, true]
    );
    assert_eq!(after[1].resolved_at, Some(EpochMillis(5_000)));
    assert_eq!(
        after[1].files.iter().map(|f| f.kept).collect::<Vec<_>>(),
        [true, false, true]
    );
    // El grupo no resuelto no cambia.
    assert_eq!(after[2].resolved_at, None);
    assert!(after[2].files.iter().all(|f| !f.kept));
    // El resumen cuenta solo lo que sigue abierto.
    let summary = store.last_done_duplicate_scan().unwrap().unwrap();
    assert_eq!(summary.group_count, 3);
    assert_eq!(summary.open_group_count, 1);
    assert_eq!(summary.reclaimable_bytes, 6 * MIB);
}

pub(crate) fn contract_resolve_rejects_invalid_resolutions_without_changes<S: Store>(
    make: impl Fn() -> S,
) {
    let store = make();
    let scan = dup_scan(&store, 1_000);
    save(
        &store,
        scan,
        vec![
            group(4 * MIB, 1, "a", 2, 100),
            group(10 * MIB, 2, "b", 3, 200),
        ],
    );
    let all = store
        .list_duplicate_groups(scan, 0, 10, None, None)
        .unwrap();
    let (big, small) = (&all.groups[0], &all.groups[1]);
    let valid_for_big = GroupResolution {
        group_id: big.id,
        kept_file_ids: vec![big.files[0].id],
    };
    let snapshot = |store: &S| {
        store
            .get_duplicate_groups(scan, &[big.id, small.id])
            .unwrap()
    };
    let before = snapshot(&store);

    let cases: Vec<(&str, Vec<GroupResolution>)> = vec![
        (
            "sin copias conservadas",
            vec![GroupResolution {
                group_id: small.id,
                kept_file_ids: vec![],
            }],
        ),
        (
            "copia de otro grupo (y el primero, valido, no se aplica)",
            vec![
                valid_for_big.clone(),
                GroupResolution {
                    group_id: small.id,
                    kept_file_ids: vec![big.files[1].id],
                },
            ],
        ),
        (
            "grupo inexistente",
            vec![
                valid_for_big.clone(),
                GroupResolution {
                    group_id: 99_999,
                    kept_file_ids: vec![1],
                },
            ],
        ),
        (
            "el mismo grupo dos veces",
            vec![valid_for_big.clone(), valid_for_big.clone()],
        ),
    ];
    for (name, resolutions) in cases {
        assert!(
            store
                .resolve_duplicate_groups(scan, &resolutions, EpochMillis(5_000))
                .is_err(),
            "{name}"
        );
        assert_eq!(snapshot(&store), before, "{name}");
    }
    // Un grupo de otro escaneo.
    let other = dup_scan(&store, 2_000);
    assert!(
        store
            .resolve_duplicate_groups(
                other,
                std::slice::from_ref(&valid_for_big),
                EpochMillis(5_000)
            )
            .is_err()
    );
    assert_eq!(snapshot(&store), before);
}

pub(crate) fn contract_resolve_twice_fails_keeping_the_first_resolution<S: Store>(
    make: impl Fn() -> S,
) {
    let store = make();
    let scan = dup_scan(&store, 1_000);
    save(&store, scan, vec![group(4 * MIB, 1, "a", 3, 100)]);
    let group_id = ids(&store, scan)[0];
    let files = group_file_ids(&store, scan, group_id);
    store
        .resolve_duplicate_groups(
            scan,
            &[GroupResolution {
                group_id,
                kept_file_ids: vec![files[0]],
            }],
            EpochMillis(5_000),
        )
        .unwrap();
    let error = store
        .resolve_duplicate_groups(
            scan,
            &[GroupResolution {
                group_id,
                kept_file_ids: vec![files[1]],
            }],
            EpochMillis(6_000),
        )
        .unwrap_err();
    assert!(matches!(error, StoreError::InvalidState(_)), "{error:?}");
    let after = store.get_duplicate_groups(scan, &[group_id]).unwrap();
    assert_eq!(after[0].resolved_at, Some(EpochMillis(5_000)));
    assert_eq!(
        after[0].files.iter().map(|f| f.kept).collect::<Vec<_>>(),
        [true, false, false]
    );
}

pub(crate) fn contract_groups_follow_scan_retention<S: Store>(make: impl Fn() -> S) {
    let store = make();
    let doomed = dup_scan(&store, 1_000);
    save(&store, doomed, vec![group(4 * MIB, 1, "a", 2, 100)]);
    let group_id = ids(&store, doomed)[0];
    for i in 1..=SCAN_RETENTION as i64 {
        let id = store
            .create_scan(&NewScan {
                kind: ScanKind::Full,
                trigger: ScanTrigger::Manual,
                started_at: EpochMillis(1_000 + i),
            })
            .unwrap();
        store
            .finish_scan(id, ScanStatus::Done, EpochMillis(1_000 + i + 1))
            .unwrap();
    }
    assert_eq!(store.get_scan(doomed).unwrap(), None);
    assert!(
        store
            .get_duplicate_groups(doomed, &[group_id])
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        store
            .list_duplicate_groups(doomed, 0, 10, None, None)
            .unwrap()
            .total,
        0
    );
    assert_eq!(store.last_done_duplicate_scan().unwrap(), None);
}

/// Las copias de un grupo salen ordenadas por ruta completa sin distinguir mayusculas y, a igual
/// ruta, por id; no importa en que orden las entrego el escaneo.
pub(crate) fn contract_copies_sort_by_path_ignoring_case_then_id<S: Store>(make: impl Fn() -> S) {
    fn copy(path: &str, n: u128) -> DuplicateFile {
        file(ContentFolder::Downloads, path, n)
    }
    fn paths_of<S: Store>(store: &S, scan: ScanId, group_id: i64) -> Vec<String> {
        store.get_duplicate_groups(scan, &[group_id]).unwrap()[0]
            .files
            .iter()
            .map(|f| f.file.path.clone())
            .collect()
    }
    let store = make();
    let scan = dup_scan(&store, 1_000);
    // Orden de recorrido del caso real: la subcarpeta antes que el archivo suelto.
    let mut walk_order = group(4 * MIB, 1, "unused", 0, 100);
    walk_order.files = vec![
        copy(r"C:\Users\ana\Downloads\b\B2", 1),
        copy(r"C:\Users\ana\Downloads\b\B3", 2),
        copy(r"C:\Users\ana\Downloads\B1", 3),
    ];
    // Mayusculas y minusculas mezcladas; "x.bin" y "X.bin" empatan y desempata el id.
    let mut mixed = group(5 * MIB, 2, "unused", 0, 200);
    mixed.files = vec![
        copy(r"C:\Users\ana\Downloads\zeta.bin", 4),
        copy(r"C:\Users\ana\Downloads\x.bin", 5),
        copy(r"C:\Users\ana\Downloads\Alfa.bin", 6),
        copy(r"C:\Users\ana\Downloads\X.bin", 7),
    ];
    save(&store, scan, vec![walk_order, mixed]);
    let page = store
        .list_duplicate_groups(scan, 0, 10, None, None)
        .unwrap();
    let by_hash = |tag: u8| {
        page.groups
            .iter()
            .find(|g| g.blake3 == hash(tag))
            .unwrap()
            .id
    };
    assert_eq!(
        paths_of(&store, scan, by_hash(1)),
        [
            r"C:\Users\ana\Downloads\B1",
            r"C:\Users\ana\Downloads\b\B2",
            r"C:\Users\ana\Downloads\b\B3",
        ]
    );
    assert_eq!(
        paths_of(&store, scan, by_hash(2)),
        [
            r"C:\Users\ana\Downloads\Alfa.bin",
            r"C:\Users\ana\Downloads\x.bin",
            r"C:\Users\ana\Downloads\X.bin",
            r"C:\Users\ana\Downloads\zeta.bin",
        ]
    );
    // El listado de la pantalla ve el mismo orden que la consulta puntual.
    let listed: Vec<String> = page
        .groups
        .iter()
        .find(|g| g.id == by_hash(1))
        .unwrap()
        .files
        .iter()
        .map(|f| f.file.path.clone())
        .collect();
    assert_eq!(listed, paths_of(&store, scan, by_hash(1)));
}

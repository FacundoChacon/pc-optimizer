use std::sync::Arc;

use super::*;
use crate::domain::ports::content_contract::sample_bytes;
use crate::domain::ports::fakes::FakeFileSystem;
use crate::domain::ports::fakes_content::FakeContentReader;

const PATH: &str = r"C:\Users\ana\Downloads\a.bin";
const MIB: usize = 1 << 20;

struct Setup {
    fs: Arc<FakeFileSystem>,
    reader: FakeContentReader,
    expected: Expected,
}

fn setup_with(bytes: Vec<u8>, build: impl FnOnce(FakeContentReader) -> FakeContentReader) -> Setup {
    let modified = Some(EpochMillis(1_000));
    let fs = Arc::new(FakeFileSystem::new().with_file(PATH, bytes.len() as u64, modified));
    let identity = fs.peek(Path::new(PATH)).and_then(|m| m.identity).unwrap();
    let reader = build(FakeContentReader::new(Arc::clone(&fs)).with_content(PATH, bytes.clone()));
    Setup {
        fs,
        reader,
        expected: Expected {
            identity,
            size_bytes: bytes.len() as u64,
            modified,
        },
    }
}

fn setup(bytes: Vec<u8>) -> Setup {
    setup_with(bytes, |reader| reader)
}

fn no_cancel() -> AtomicBool {
    AtomicBool::new(false)
}

fn handle(s: &Setup) -> Box<dyn ContentHandle> {
    open_verified(&s.reader, Path::new(PATH), &s.expected).unwrap()
}

#[test]
fn hash_partial_should_hash_the_first_and_last_64_kib_of_a_3_mib_file() {
    let bytes = sample_bytes(3 * MIB);
    let s = setup(bytes.clone());
    let mut h = handle(&s);
    let result = hash_partial(h.as_mut(), &s.expected, &no_cancel()).unwrap();
    let mut expected_input = bytes[..EDGE_SIZE].to_vec();
    expected_input.extend_from_slice(&bytes[bytes.len() - EDGE_SIZE..]);
    assert_eq!(result.partial, blake3::hash(&expected_input));
    assert_eq!(result.full, None);
    // Solo se leyeron los dos bordes, no el medio.
    assert_eq!(s.reader.bytes_read_of(PATH), 2 * EDGE_SIZE as u64);
}

#[test]
fn hash_partial_should_ignore_the_middle_of_the_file() {
    let a = sample_bytes(3 * MIB);
    let mut b = a.clone();
    b[MIB] ^= 0xFF;
    let (sa, sb) = (setup(a), setup(b));
    let (mut ha, mut hb) = (handle(&sa), handle(&sb));
    let pa = hash_partial(ha.as_mut(), &sa.expected, &no_cancel()).unwrap();
    let pb = hash_partial(hb.as_mut(), &sb.expected, &no_cancel()).unwrap();
    assert_eq!(pa.partial, pb.partial);
}

#[test]
fn hash_partial_should_use_the_whole_content_for_a_file_up_to_128_kib() {
    for len in [102_400usize, WHOLE_FILE_LIMIT as usize] {
        let bytes = sample_bytes(len);
        let s = setup(bytes.clone());
        let mut h = handle(&s);
        let result = hash_partial(h.as_mut(), &s.expected, &no_cancel()).unwrap();
        assert_eq!(result.partial, blake3::hash(&bytes), "{len}");
        assert_eq!(result.full, Some(blake3::hash(&bytes)), "{len}");
        assert_eq!(s.reader.bytes_read_of(PATH), len as u64, "{len}");
    }
}

#[test]
fn hash_partial_should_switch_to_edges_just_above_128_kib() {
    let bytes = sample_bytes(WHOLE_FILE_LIMIT as usize + 1);
    let s = setup(bytes);
    let mut h = handle(&s);
    let result = hash_partial(h.as_mut(), &s.expected, &no_cancel()).unwrap();
    assert_eq!(result.full, None);
}

#[test]
fn hash_full_should_equal_the_blake3_of_the_content_read_in_1_mib_blocks() {
    let bytes = sample_bytes(5 * MIB / 2);
    let s = setup(bytes.clone());
    let mut h = handle(&s);
    let mut reported = Vec::new();
    let hash = hash_full(h.as_mut(), &s.expected, &no_cancel(), |n| reported.push(n)).unwrap();
    assert_eq!(hash, blake3::hash(&bytes));
    assert_eq!(reported, [MIB as u64, MIB as u64, MIB as u64 / 2]);
    assert_eq!(s.reader.reads_of(PATH), 3);
}

#[test]
fn hash_full_should_differ_when_a_single_middle_byte_differs() {
    let a = sample_bytes(2 * MIB);
    let mut b = a.clone();
    b[MIB + 7] ^= 1;
    let (sa, sb) = (setup(a), setup(b));
    let (mut ha, mut hb) = (handle(&sa), handle(&sb));
    let fa = hash_full(ha.as_mut(), &sa.expected, &no_cancel(), |_| {}).unwrap();
    let fb = hash_full(hb.as_mut(), &sb.expected, &no_cancel(), |_| {}).unwrap();
    assert_ne!(fa, fb);
}

#[test]
fn open_verified_should_reject_another_identity_without_reading() {
    let s = setup(sample_bytes(1_000));
    let other = Expected {
        identity: FileIdentity {
            volume_serial: 9,
            file_id: 9,
        },
        ..s.expected
    };
    assert_eq!(
        open_verified(&s.reader, Path::new(PATH), &other).err(),
        Some(HashFailure::IdentityMismatch)
    );
    assert_eq!(s.reader.bytes_read(), 0);
}

#[test]
fn open_verified_should_reject_a_different_size_or_date() {
    let s = setup(sample_bytes(1_000));
    let bigger = Expected {
        size_bytes: 2_000,
        ..s.expected
    };
    let newer = Expected {
        modified: Some(EpochMillis(5)),
        ..s.expected
    };
    for expected in [bigger, newer] {
        assert_eq!(
            open_verified(&s.reader, Path::new(PATH), &expected).err(),
            Some(HashFailure::Changed)
        );
    }
}

#[test]
fn open_verified_should_reject_a_file_replaced_between_verification_and_open() {
    let s = setup(sample_bytes(1_000));
    let fs = Arc::clone(&s.fs);
    let reader = s.reader.on_open(PATH, move || {
        fs.recreate_file(
            Path::new(PATH),
            FileIdentity {
                volume_serial: 1,
                file_id: 999,
            },
        );
    });
    assert_eq!(
        open_verified(&reader, Path::new(PATH), &s.expected).err(),
        Some(HashFailure::IdentityMismatch)
    );
}

#[test]
fn open_verified_should_refuse_cloud_content_before_reading_any_byte() {
    let fs = Arc::new(FakeFileSystem::new().with_cloud_placeholder(PATH, 2 * MIB as u64));
    let identity = fs.peek(Path::new(PATH)).and_then(|m| m.identity).unwrap();
    let reader = FakeContentReader::new(Arc::clone(&fs)).with_content(PATH, sample_bytes(MIB));
    let expected = Expected {
        identity,
        size_bytes: 2 * MIB as u64,
        modified: None,
    };
    assert_eq!(
        open_verified(&reader, Path::new(PATH), &expected).err(),
        Some(HashFailure::Cloud)
    );
    assert_eq!(reader.bytes_read(), 0);
}

#[test]
fn open_verified_should_reject_a_directory_in_place_of_the_file() {
    let s = setup(sample_bytes(1_000));
    s.fs.replace_with_directory(
        Path::new(PATH),
        FileIdentity {
            volume_serial: 1,
            file_id: 5,
        },
    );
    assert_eq!(
        open_verified(&s.reader, Path::new(PATH), &s.expected).err(),
        Some(HashFailure::IdentityMismatch)
    );
}

#[test]
fn open_verified_should_map_port_errors() {
    let in_use = setup_with(sample_bytes(10), |r| {
        r.fail_open(PATH, PortError::InUse("x".into()))
    });
    assert_eq!(
        open_verified(&in_use.reader, Path::new(PATH), &in_use.expected).err(),
        Some(HashFailure::InUse)
    );
    let denied = setup_with(sample_bytes(10), |r| {
        r.fail_open(PATH, PortError::PermissionDenied("x".into()))
    });
    assert_eq!(
        open_verified(&denied.reader, Path::new(PATH), &denied.expected).err(),
        Some(HashFailure::Unreadable)
    );
    let gone = setup(sample_bytes(10));
    assert_eq!(
        open_verified(&gone.reader, Path::new(r"C:\no\esta.bin"), &gone.expected).err(),
        Some(HashFailure::Changed)
    );
}

#[test]
fn hashing_should_report_in_use_and_unreadable_read_errors_apart() {
    let in_use = setup_with(sample_bytes(3 * MIB), |r| {
        r.fail_read(PATH, PortError::InUse("x".into()))
    });
    let mut h = handle(&in_use);
    assert_eq!(
        hash_partial(h.as_mut(), &in_use.expected, &no_cancel()).err(),
        Some(HashFailure::InUse)
    );
    let broken = setup_with(sample_bytes(3 * MIB), |r| {
        r.fail_read(PATH, PortError::Io("sector".into()))
    });
    let mut h = handle(&broken);
    assert_eq!(
        hash_full(h.as_mut(), &broken.expected, &no_cancel(), |_| {}).err(),
        Some(HashFailure::Unreadable)
    );
}

#[test]
fn hash_full_should_report_changed_when_the_file_grows_while_reading() {
    let bytes = sample_bytes(3 * MIB);
    let s = setup(bytes);
    let fs = Arc::clone(&s.fs);
    let reader = s.reader.after_read(PATH, 1, move || {
        fs.set_size_and_time(Path::new(PATH), 4 * MIB as u64, Some(EpochMillis(1_000)));
    });
    let mut h = open_verified(&reader, Path::new(PATH), &s.expected).unwrap();
    assert_eq!(
        hash_full(h.as_mut(), &s.expected, &no_cancel(), |_| {}).err(),
        Some(HashFailure::Changed)
    );
}

#[test]
fn hash_full_should_report_changed_when_the_date_changes_while_reading() {
    let s = setup(sample_bytes(2 * MIB));
    let fs = Arc::clone(&s.fs);
    let reader = s.reader.after_read(PATH, 2, move || {
        fs.set_size_and_time(Path::new(PATH), 2 * MIB as u64, Some(EpochMillis(2_000)));
    });
    let mut h = open_verified(&reader, Path::new(PATH), &s.expected).unwrap();
    assert_eq!(
        hash_full(h.as_mut(), &s.expected, &no_cancel(), |_| {}).err(),
        Some(HashFailure::Changed)
    );
}

#[test]
fn hash_partial_should_report_changed_when_the_file_changes_while_reading() {
    let s = setup(sample_bytes(3 * MIB));
    let fs = Arc::clone(&s.fs);
    let reader = s.reader.after_read(PATH, 1, move || {
        fs.set_size_and_time(Path::new(PATH), 3 * MIB as u64, Some(EpochMillis(77)));
    });
    let mut h = open_verified(&reader, Path::new(PATH), &s.expected).unwrap();
    assert_eq!(
        hash_partial(h.as_mut(), &s.expected, &no_cancel()).err(),
        Some(HashFailure::Changed)
    );
}

#[test]
fn hash_full_should_report_changed_when_the_file_is_truncated_under_the_reader() {
    let s = setup(sample_bytes(2 * MIB));
    s.reader.replace_content(Path::new(PATH), sample_bytes(MIB));
    let mut h = handle(&s);
    assert_eq!(
        hash_full(h.as_mut(), &s.expected, &no_cancel(), |_| {}).err(),
        Some(HashFailure::Changed)
    );
}

#[test]
fn hash_full_should_stop_after_the_block_in_which_cancel_is_raised() {
    let size = 4 * MIB as u64;
    let fs = Arc::new(FakeFileSystem::new().with_file(PATH, size, Some(EpochMillis(1_000))));
    let identity = fs.peek(Path::new(PATH)).and_then(|m| m.identity).unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancel);
    let reader = FakeContentReader::new(Arc::clone(&fs))
        .with_filled(PATH, size, 3)
        .after_read(PATH, 1, move || flag.store(true, Ordering::Relaxed));
    let expected = Expected {
        identity,
        size_bytes: size,
        modified: Some(EpochMillis(1_000)),
    };
    let mut h = open_verified(&reader, Path::new(PATH), &expected).unwrap();
    assert_eq!(
        hash_full(h.as_mut(), &expected, &cancel, |_| {}).err(),
        Some(HashFailure::Cancelled)
    );
    // Se leyo un solo bloque: el resto no.
    assert_eq!(reader.bytes_read_of(PATH), MIB as u64);
}

#[test]
fn hash_should_not_read_when_cancelled_beforehand() {
    let s = setup(sample_bytes(3 * MIB));
    let mut h = handle(&s);
    let raised = AtomicBool::new(true);
    assert_eq!(
        hash_partial(h.as_mut(), &s.expected, &raised).err(),
        Some(HashFailure::Cancelled)
    );
    assert_eq!(
        hash_full(h.as_mut(), &s.expected, &raised, |_| {}).err(),
        Some(HashFailure::Cancelled)
    );
    assert_eq!(s.reader.bytes_read(), 0);
}

#[test]
fn to_hex_should_be_64_lowercase_hex_characters() {
    let hex = to_hex(&blake3::hash(b"abc"));
    assert_eq!(hex.len(), 64);
    assert!(
        hex.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
}

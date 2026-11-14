//! Bateria de contrato del puerto `ContentReader` (solo tests).
//!
//! Corre contra `FakeContentReader` y contra el adaptador real de Windows sobre un `TempDir`.
//! El archivo `file` contiene `sample_bytes(SAMPLE_LEN)`.

use std::path::PathBuf;

use crate::domain::ports::{ContentReader, EntryKind, PortError};

/// Tamano del archivo de muestra: mas de 128 KiB para distinguir cabeza, medio y cola.
pub(crate) const SAMPLE_LEN: usize = 300_000;

/// Bytes deterministas no periodicos en bloques cortos.
pub(crate) fn sample_bytes(len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| u8::try_from((i * 31 + i / 251) % 256).unwrap_or(0))
        .collect()
}

pub(crate) struct Scenario<R> {
    pub reader: R,
    /// Existe y contiene `sample_bytes(SAMPLE_LEN)`.
    pub file: PathBuf,
    /// No existe.
    pub missing: PathBuf,
}

pub(crate) fn contract_reads_exact_ranges<R: ContentReader>(s: &Scenario<R>) {
    let expected = sample_bytes(SAMPLE_LEN);
    let mut handle = s.reader.open(&s.file).unwrap();
    let mut head = vec![0u8; 65_536];
    assert_eq!(handle.read_at(0, &mut head).unwrap(), 65_536);
    assert_eq!(head, expected[..65_536]);
    let tail_start = SAMPLE_LEN - 65_536;
    let mut tail = vec![0u8; 65_536];
    assert_eq!(
        handle.read_at(tail_start as u64, &mut tail).unwrap(),
        65_536
    );
    assert_eq!(tail, expected[tail_start..]);
}

pub(crate) fn contract_read_past_the_end_returns_zero<R: ContentReader>(s: &Scenario<R>) {
    let mut handle = s.reader.open(&s.file).unwrap();
    let mut buf = [0u8; 128];
    assert_eq!(handle.read_at(SAMPLE_LEN as u64, &mut buf).unwrap(), 0);
    assert_eq!(
        handle
            .read_at(SAMPLE_LEN as u64 + 10_000, &mut buf)
            .unwrap(),
        0
    );
    // Un rango que cruza el final devuelve solo lo que queda.
    assert_eq!(handle.read_at(SAMPLE_LEN as u64 - 5, &mut buf).unwrap(), 5);
}

pub(crate) fn contract_meta_describes_the_open_file<R: ContentReader>(s: &Scenario<R>) {
    let first = s.reader.open(&s.file).unwrap().meta().unwrap();
    let second = s.reader.open(&s.file).unwrap().meta().unwrap();
    assert_eq!(first.kind, EntryKind::File);
    assert_eq!(first.size_bytes, SAMPLE_LEN as u64);
    assert!(!first.cloud_placeholder);
    assert!(first.identity.is_some());
    assert_eq!(first.identity, second.identity);
}

pub(crate) fn contract_missing_file_is_not_found<R: ContentReader>(s: &Scenario<R>) {
    assert!(matches!(
        s.reader.open(&s.missing),
        Err(PortError::NotFound(_))
    ));
}

/// Corre toda la bateria.
pub(crate) fn run_all<R: ContentReader>(s: &Scenario<R>) {
    contract_reads_exact_ranges(s);
    contract_read_past_the_end_returns_zero(s);
    contract_meta_describes_the_open_file(s);
    contract_missing_file_is_not_found(s);
}

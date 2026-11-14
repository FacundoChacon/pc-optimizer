//! Hash parcial y completo sobre `ContentReader` (design D-05, D-06).
//!
//! `blake3` es computo puro (sin E/S): el hash vive en el dominio para poder probar las etapas, el
//! falso positivo del parcial y el "cambio durante la lectura" con el doble en memoria.
//!
//! Defensa en profundidad contra la nube: el dominio nunca abre un candidato que no sea `allowed`
//! y ademas `open_verified` rechaza el handle cuyo `meta().cloud_placeholder` sea verdadero ANTES
//! de leer un solo byte.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::domain::model::EpochMillis;
use crate::domain::ports::{ContentHandle, ContentReader, EntryKind, FileIdentity, PortError};

/// Tamano de cada lectura del hash completo (1 MiB): la cancelacion se consulta entre bloques.
pub const BLOCK_SIZE: usize = 1 << 20;
/// Cada borde del hash parcial: los primeros y los ultimos 64 KiB (RN-DUP-01).
pub const EDGE_SIZE: usize = 64 << 10;
/// Hasta este tamano se lee el archivo completo una vez y vale como parcial y completo.
pub const WHOLE_FILE_LIMIT: u64 = 2 * EDGE_SIZE as u64;

/// Por que no se obtuvo un hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashFailure {
    /// El archivo abierto no es el verificado (identidad, tipo): otro archivo ocupa la ruta.
    IdentityMismatch,
    /// Tamano o ultima escritura distintos de los verificados, al abrir o durante la lectura, o
    /// el archivo desaparecio o se trunco.
    Changed,
    /// Otro proceso lo tiene abierto sin compartir lectura.
    InUse,
    /// No se pudo abrir o leer por otro motivo.
    Unreadable,
    /// El handle es contenido en la nube: no se lee (no provocar una descarga).
    Cloud,
    /// Se levanto la cancelacion.
    Cancelled,
}

/// Lo que se verifico del archivo antes de abrirlo; el handle debe coincidir.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Expected {
    pub identity: FileIdentity,
    pub size_bytes: u64,
    pub modified: Option<EpochMillis>,
}

/// Hash parcial y, si el archivo es chico, tambien el completo (no se relee).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartialHash {
    pub partial: blake3::Hash,
    pub full: Option<blake3::Hash>,
}

fn map_port(error: &PortError) -> HashFailure {
    match error {
        PortError::InUse(_) => HashFailure::InUse,
        PortError::NotFound(_) => HashFailure::Changed,
        _ => HashFailure::Unreadable,
    }
}

/// Abre `path` y verifica que el archivo abierto es el esperado: no nube, archivo comun, misma
/// identidad, tamano y ultima escritura.
pub fn open_verified<R: ContentReader + ?Sized>(
    reader: &R,
    path: &Path,
    expected: &Expected,
) -> Result<Box<dyn ContentHandle>, HashFailure> {
    let handle = reader.open(path).map_err(|e| map_port(&e))?;
    let meta = handle.meta().map_err(|e| map_port(&e))?;
    if meta.cloud_placeholder {
        return Err(HashFailure::Cloud);
    }
    if meta.kind != EntryKind::File || meta.identity != Some(expected.identity) {
        return Err(HashFailure::IdentityMismatch);
    }
    if meta.size_bytes != expected.size_bytes || meta.modified != expected.modified {
        return Err(HashFailure::Changed);
    }
    Ok(handle)
}

/// Tamano y fecha del archivo abierto siguen siendo los esperados (despues de leer).
fn verify_unchanged(handle: &dyn ContentHandle, expected: &Expected) -> Result<(), HashFailure> {
    let meta = handle.meta().map_err(|e| map_port(&e))?;
    if meta.size_bytes != expected.size_bytes || meta.modified != expected.modified {
        return Err(HashFailure::Changed);
    }
    Ok(())
}

/// Lee exactamente `buf.len()` bytes desde `offset`; un final de archivo antes es `Changed`
/// (el archivo se trunco despues de medirlo).
fn read_exact_at(
    handle: &mut dyn ContentHandle,
    mut offset: u64,
    buf: &mut [u8],
) -> Result<(), HashFailure> {
    let mut filled = 0;
    while filled < buf.len() {
        let read = handle
            .read_at(offset, &mut buf[filled..])
            .map_err(|e| map_port(&e))?;
        if read == 0 {
            return Err(HashFailure::Changed);
        }
        filled += read;
        offset += read as u64;
    }
    Ok(())
}

fn cancelled(cancel: &AtomicBool) -> bool {
    // Solo es una bandera: no ordena otros accesos a memoria.
    cancel.load(Ordering::Relaxed)
}

/// Hash parcial: `blake3(primeros 64 KiB || ultimos 64 KiB)`. Un archivo de hasta 128 KiB se lee
/// completo y ese hash vale tambien como completo.
pub fn hash_partial(
    handle: &mut dyn ContentHandle,
    expected: &Expected,
    cancel: &AtomicBool,
) -> Result<PartialHash, HashFailure> {
    if cancelled(cancel) {
        return Err(HashFailure::Cancelled);
    }
    let size = expected.size_bytes;
    let result = if size <= WHOLE_FILE_LIMIT {
        let mut content = vec![0u8; usize::try_from(size).map_err(|_| HashFailure::Unreadable)?];
        read_exact_at(handle, 0, &mut content)?;
        let hash = blake3::hash(&content);
        PartialHash {
            partial: hash,
            full: Some(hash),
        }
    } else {
        let mut head = vec![0u8; EDGE_SIZE];
        let mut tail = vec![0u8; EDGE_SIZE];
        read_exact_at(handle, 0, &mut head)?;
        read_exact_at(handle, size - EDGE_SIZE as u64, &mut tail)?;
        let mut hasher = blake3::Hasher::new();
        hasher.update(&head);
        hasher.update(&tail);
        PartialHash {
            partial: hasher.finalize(),
            full: None,
        }
    };
    verify_unchanged(handle, expected)?;
    Ok(result)
}

/// Hash blake3 completo en bloques de 1 MiB. La cancelacion se consulta antes de cada bloque;
/// `on_bytes` recibe cada bloque leido.
pub fn hash_full(
    handle: &mut dyn ContentHandle,
    expected: &Expected,
    cancel: &AtomicBool,
    mut on_bytes: impl FnMut(u64),
) -> Result<blake3::Hash, HashFailure> {
    let size = expected.size_bytes;
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; BLOCK_SIZE];
    let mut offset = 0u64;
    while offset < size {
        if cancelled(cancel) {
            return Err(HashFailure::Cancelled);
        }
        let want = usize::try_from(size - offset).map_or(BLOCK_SIZE, |r| r.min(BLOCK_SIZE));
        let read = handle
            .read_at(offset, &mut buf[..want])
            .map_err(|e| map_port(&e))?;
        if read == 0 {
            return Err(HashFailure::Changed);
        }
        hasher.update(&buf[..read]);
        offset += read as u64;
        on_bytes(read as u64);
    }
    verify_unchanged(handle, expected)?;
    Ok(hasher.finalize())
}

/// Hex minuscula de 64 caracteres.
pub fn to_hex(hash: &blake3::Hash) -> String {
    hash.to_hex().to_string()
}

#[cfg(test)]
#[path = "hash_tests.rs"]
mod tests;

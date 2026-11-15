//! Analisis de espacio en disco (C-07): que carpetas y archivos ocupan el volumen del sistema o
//! una carpeta elegida.
//!
//! # Contrato
//!
//! - El analisis es de SOLO LECTURA: lista carpetas y usa los metadatos de la enumeracion
//!   (puerto `FileSystem`). Nunca abre ni lee el contenido de un archivo, nunca mueve, renombra,
//!   modifica ni borra nada, y no sigue enlaces (junctions, symlinks, puntos de montaje).
//! - Una carpeta opaca (perfil de otro usuario, `$Recycle.Bin`, `System Volume Information`) se
//!   mide pero sus rutas interiores NUNCA salen del dominio: ni nodos, ni top de archivos, ni
//!   incidencias, ni eventos de progreso.
//! - Este modulo no ofrece ningun camino de borrado: revisar basura es ir a la pantalla Limpieza.
//! - Dominio puro: sin `std::fs`, `windows`, `rusqlite` ni `tauri` (lo verifica
//!   `tests/architecture.rs`).

pub mod aggregate;
pub mod classify;
pub mod model;
pub mod scanner;
pub mod select;
pub mod top_files;

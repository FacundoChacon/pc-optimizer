//! Proteccion de rutas: decide que puede borrarse alguna vez (fail-closed).
//!
//! Dos capas (design D-01): reglas puras sobre rutas normalizadas (`path`, `context`, `rules`,
//! `decision`) y un servicio (`service`) que combina las reglas con el puerto `FileSystem`.

pub mod context;
pub mod decision;
pub mod path;
pub mod rules;
pub mod service;

#[cfg(test)]
pub(crate) mod test_support;

#[cfg(test)]
mod scenarios_tests;

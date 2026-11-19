//! Busqueda de duplicados (C-11): archivos con contenido identico en las carpetas de contenido del
//! usuario actual.
//!
//! # Contrato
//!
//! - El escaneo es de SOLO LECTURA: lista carpetas, lee metadatos y lee contenido por el puerto
//!   `ContentReader`. Nunca mueve, renombra, modifica ni borra nada.
//! - Nunca abre un archivo que la proteccion de rutas no haya declarado `allowed`: los `protected`
//!   y los marcadores de nube se cuentan y no se leen.
//! - El borrado de las copias que el usuario decida solo ocurre por `cleanup::run_cleanup`
//!   (Papelera, re-verificacion y log), con una verificacion adicional del contenido.

pub mod decision;
pub mod guard;
pub mod hash;
pub mod model;
pub mod preview;
pub mod roots;
pub mod scanner;
pub mod walk;

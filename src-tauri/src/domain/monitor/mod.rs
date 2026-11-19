//! Monitor en vivo (C-06): CPU, memoria y disco del sistema muestreados en segundo plano.
//!
//! # Contrato
//!
//! - SOLO LECTURA: lee contadores del sistema por el puerto `Sensors`. No toca archivos del
//!   usuario, no escribe en disco ni en la base: nada se persiste (C-15 agregara el historico).
//! - Una metrica que no se pudo medir es AUSENTE con un motivo (`Availability`); nunca se
//!   informa como 0 ni se interpola. Los huecos (suspension, salto del reloj) tampoco se rellenan.
//! - Todo lo calculable (tasas, huecos, promedios, niveles) vive aqui y se prueba con relojes y
//!   sensores dobles, sin `sleep` ni reloj real.
//! - Dominio puro: sin `std::fs`, `windows`, `rusqlite` ni `tauri` (lo verifica
//!   `tests/architecture.rs`).

pub mod engine;
pub mod model;
pub mod ring;
pub mod runner;
pub mod schedule;
pub mod summary;
pub mod window;

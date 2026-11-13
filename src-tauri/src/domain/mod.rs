//! Dominio: logica pura, sin I/O. No importa `std::fs`, `windows`, `rusqlite` ni `tauri`.

pub mod model;
pub mod ports;
pub mod privileges;
pub mod protection;
pub mod settings;
pub mod store_status;

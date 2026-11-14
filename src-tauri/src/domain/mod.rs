//! Dominio: logica pura, sin I/O. No importa `std::fs`, `windows`, `rusqlite` ni `tauri`.

pub mod cleanup;
pub mod junk;
pub mod model;
pub mod ports;
pub mod privileges;
pub mod protection;
pub mod recycle_bin;
pub mod settings;
pub mod store_status;

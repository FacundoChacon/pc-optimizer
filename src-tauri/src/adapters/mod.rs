//! Adaptadores: acceso al SO (disco, Windows, SQLite) detras de traits definidos en el dominio.
//! Dominio: logica pura, sin I/O. No importa `std::fs`, `windows`, `rusqlite` ni `tauri`.

pub mod clock;
pub mod sqlite;

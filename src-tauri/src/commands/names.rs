/// Nombres de los comandos IPC propios (fuente unica).
///
/// Lo incluye `build.rs` con `include!` para generar los permisos `allow-<comando>`
/// y la lib lo expone como modulo para que los tests de baseline lo comparen
/// con las capabilities.
pub const APP_COMMANDS: &[&str] = &["get_app_info"];

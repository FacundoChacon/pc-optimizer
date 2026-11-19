//! Vaciado de la Papelera (C-10): la UNICA accion permanente del producto (RN-LIM-06).
//!
//! # Contrato
//!
//! - Solo la llaman los comandos de vaciado (`commands/recycle_bin.rs`): nunca los escaneos, el
//!   scheduler, las mejoras ni la bandeja. `tests/architecture.rs` impone la lista cerrada.
//! - La raiz es siempre el volumen del sistema ([`root::VolumeRoot`]): no existe "todas las
//!   Papeleras" ni una carpeta.
//! - Doble confirmacion aplicada por tipos: [`request::PreparedEmpty`] (medida) ->
//!   [`request::AcknowledgedEmpty`] (primera confirmacion) -> [`execute::run_empty_recycle_bin`]
//!   (segunda confirmacion con el token). Solo `acknowledge` produce un `AcknowledgedEmpty`.
//! - Bloqueado en `mismatch` (la Papelera es por-usuario) y con re-medicion estricta justo antes.
//! - El resultado se decide midiendo despues ([`outcome::classify_empty`]), no por el `Result`
//!   de la llamada al shell.

pub mod execute;
pub mod outcome;
pub mod request;
pub mod root;

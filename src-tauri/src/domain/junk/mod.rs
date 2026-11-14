//! Deteccion de basura (C-08): whitelist cerrada de categorias, recorrido de solo lectura y
//! verificacion de cada candidato con la proteccion de rutas.
//!
//! La deteccion NUNCA borra, mueve ni abre archivos para escribir: solo lista carpetas, lee
//! metadatos y mide la Papelera.
//!
//! # Contrato para la limpieza (C-09)
//!
//! - Un hallazgo es un archivo; las carpetas nunca lo son. Cada uno lleva su decision propia:
//!   `FindingState::Eligible` con la ruta REAL verificada y su identidad (volumen + id de
//!   archivo), o `FindingState::Skipped` con el codigo estable del motivo.
//! - Un hallazgo elegible no autoriza a borrar: `prepare_delete` debe volver a verificar justo
//!   antes de mover (ruta real, tipo e identidad persistida), y `scope_access` debe reevaluarse
//!   al confirmar, no solo al previsualizar.
//! - El tamano elegible es un maximo: la deteccion no prueba si un archivo esta en uso; la
//!   limpieza lo omite (`skipped_locked`) al intentar moverlo.
//! - La Papelera solo se mide (`recycle_bin`): no hay hallazgos de esa categoria.
//! - Solo el ultimo escaneo `done` conserva sus hallazgos.

pub mod category;
pub mod filter;
pub mod model;
pub mod scanner;
pub mod sources;
pub mod verify;
pub mod walk;

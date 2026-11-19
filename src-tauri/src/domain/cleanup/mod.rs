//! Limpieza (C-09): preview y ejecucion sobre los hallazgos del ultimo escaneo de basura.
//!
//! Todo es puro: la E/S llega por los puertos `FileSystem`, `Trash`, `CleanupLogStore` y `Clock`.
//!
//! # Contrato para quien consuma la limpieza (C-10, C-11, C-21)
//!
//! - **Preview**: [`preview::build_preview`] es la UNICA forma de obtener un
//!   [`model::CleanupPreview`] en produccion y no toca el disco. Quien necesite limpiar otras
//!   filas (copias duplicadas en C-11, el asistente en C-21) construye el preview con sus PROPIAS
//!   filas ([`model::CleanupItem`]: ruta real, categoria, tamano, fecha e identidad persistidos) y
//!   reutiliza [`execute::run_cleanup`]; no hay que reimplementar la re-verificacion.
//! - **Ejecucion**: `run_cleanup` consume el preview por valor (de un solo uso) y, por cada item y
//!   en este orden: reevalua el alcance por-usuario con el contexto de AHORA, repite
//!   `prepare_delete` (fija ancestros, vuelve a resolver sin memo, compara identidad y tipo),
//!   compara tamano y fecha leidos del archivo ABIERTO, aplica la guarda de 24 h, mueve con
//!   `Trash::move_to_trash(&DeleteTarget)`, post-verifica y recien entonces suelta la fijacion y
//!   registra el resultado en `cleanup_log`.
//! - **Nunca borrado permanente**: la unica via es `Trash::move_to_trash`, que solo acepta un
//!   `DeleteTarget` re-verificado; no hay vaciado de la Papelera en este flujo (eso es C-10, con su
//!   propia doble confirmacion).
//! - **Resultado por item** (D-06): uno de los 5 outcomes de `cleanup_log` mas un codigo de
//!   detalle estable ([`detail`]); un fallo por item nunca detiene el resto, salvo los abortos
//!   (`log_unavailable`, `recycle_bin_unavailable`, `trash_unavailable`).
//! - **Ciclo de vida**: [`lifecycle::PreviewSlot`] guarda un unico preview activo, de un solo uso,
//!   que vence a los 10 minutos.

pub mod detail;
pub mod execute;
pub mod lifecycle;
pub mod model;
pub mod preview;
mod summary;

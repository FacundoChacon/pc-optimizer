//! Ciclo de vida del preview (design D-01): uno activo, de un solo uso, vence a los 10 minutos.

use crate::domain::model::EpochMillis;

use super::model::{CleanupPreview, PREVIEW_TTL_MS, PreviewId};

/// Por que no se pudo tomar un preview para confirmarlo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SlotError {
    /// Identificador desconocido, reemplazado o ya consumido.
    #[error("preview not found")]
    PreviewNotFound,
    /// Pasaron mas de 10 minutos desde que se creo (queda consumido).
    #[error("preview expired")]
    PreviewExpired,
}

/// El unico preview activo. Crear otro reemplaza al anterior; confirmarlo lo consume.
#[derive(Debug, Default)]
pub struct PreviewSlot {
    current: Option<CleanupPreview>,
}

impl PreviewSlot {
    pub fn new() -> Self {
        Self::default()
    }

    /// Guarda `preview` e invalida el anterior (su id deja de valer).
    pub fn store(&mut self, preview: CleanupPreview) {
        self.current = Some(preview);
    }

    /// Consume el preview `id`. Un id que no es el activo falla sin tocar el activo; uno vencido
    /// (`now - created_at > 10 min`) tambien queda consumido.
    pub fn take(&mut self, id: PreviewId, now: EpochMillis) -> Result<CleanupPreview, SlotError> {
        if self.current.as_ref().map(CleanupPreview::id) != Some(id) {
            return Err(SlotError::PreviewNotFound);
        }
        let preview = self.current.take().ok_or(SlotError::PreviewNotFound)?;
        if now.0.saturating_sub(preview.created_at().0) > PREVIEW_TTL_MS {
            return Err(SlotError::PreviewExpired);
        }
        Ok(preview)
    }
}

#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod tests;

//! Ciclo de vida del preview (design D-01): uno activo, de un solo uso, vence a los 10 minutos.

use crate::domain::duplicates::preview::DuplicateCleanupPreview;
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

/// El preview activo: de basura o de duplicados (C-11). Un solo preview de cualquier tipo.
#[derive(Debug)]
pub enum ActivePreview {
    Junk(CleanupPreview),
    Duplicates(DuplicateCleanupPreview),
}

impl ActivePreview {
    fn core(&self) -> &CleanupPreview {
        match self {
            Self::Junk(preview) => preview,
            Self::Duplicates(preview) => preview.preview(),
        }
    }
}

/// El unico preview activo. Crear otro (de cualquier tipo) reemplaza al anterior; confirmarlo lo
/// consume. Confirmar con el tipo equivocado es `PreviewNotFound` y no consume el activo.
#[derive(Debug, Default)]
pub struct PreviewSlot {
    current: Option<ActivePreview>,
}

impl PreviewSlot {
    pub fn new() -> Self {
        Self::default()
    }

    /// Guarda un preview de basura e invalida el anterior (su id deja de valer).
    pub fn store(&mut self, preview: CleanupPreview) {
        self.current = Some(ActivePreview::Junk(preview));
    }

    /// Guarda un preview de duplicados e invalida el anterior.
    pub fn store_duplicates(&mut self, preview: DuplicateCleanupPreview) {
        self.current = Some(ActivePreview::Duplicates(preview));
    }

    /// Consume el preview de basura `id`. Un id que no es el activo (o un preview de otro tipo)
    /// falla sin tocar el activo; uno vencido (`now - created_at > 10 min`) tambien queda
    /// consumido.
    pub fn take(&mut self, id: PreviewId, now: EpochMillis) -> Result<CleanupPreview, SlotError> {
        match self.take_active(id, now, |active| matches!(active, ActivePreview::Junk(_)))? {
            ActivePreview::Junk(preview) => Ok(preview),
            ActivePreview::Duplicates(_) => Err(SlotError::PreviewNotFound),
        }
    }

    /// Como [`PreviewSlot::take`], para un preview de duplicados.
    pub fn take_duplicates(
        &mut self,
        id: PreviewId,
        now: EpochMillis,
    ) -> Result<DuplicateCleanupPreview, SlotError> {
        match self.take_active(id, now, |active| {
            matches!(active, ActivePreview::Duplicates(_))
        })? {
            ActivePreview::Duplicates(preview) => Ok(preview),
            ActivePreview::Junk(_) => Err(SlotError::PreviewNotFound),
        }
    }

    fn take_active(
        &mut self,
        id: PreviewId,
        now: EpochMillis,
        wanted: impl Fn(&ActivePreview) -> bool,
    ) -> Result<ActivePreview, SlotError> {
        let matches = self
            .current
            .as_ref()
            .is_some_and(|active| active.core().id() == id && wanted(active));
        if !matches {
            return Err(SlotError::PreviewNotFound);
        }
        let active = self.current.take().ok_or(SlotError::PreviewNotFound)?;
        if now.0.saturating_sub(active.core().created_at().0) > PREVIEW_TTL_MS {
            return Err(SlotError::PreviewExpired);
        }
        Ok(active)
    }
}

#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod tests;

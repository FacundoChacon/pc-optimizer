//! Solicitud de vaciado en dos pasos (design D-01, D-02), con type-state.
//!
//! `PreparedEmpty` solo lo crea `prepare_empty`; `acknowledge` lo consume y produce un
//! `AcknowledgedEmpty`, el UNICO valor que acepta `run_empty_recycle_bin`. Los constructores son
//! privados al modulo `recycle_bin`: no hay forma de saltarse un paso desde fuera.

use crate::domain::model::EpochMillis;
use crate::domain::ports::RecycleBinUsage;

use super::root::VolumeRoot;

/// Vida de una solicitud desde su preparacion (D-02).
pub const EMPTY_REQUEST_TTL_MS: i64 = 2 * 60 * 1000;

/// Por que una solicitud no sirve para el paso pedido.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum EmptyRequestError {
    /// Identificador o token desconocido, reemplazado o ya consumido.
    #[error("empty request not found")]
    NotFound,
    /// Pasaron mas de 2 minutos desde la preparacion.
    #[error("empty request expired")]
    Expired,
    /// Falta la primera confirmacion.
    #[error("empty request not acknowledged")]
    NotAcknowledged,
}

/// Solicitud medida, todavia sin la primera confirmacion.
#[derive(Debug)]
pub struct PreparedEmpty {
    id: u64,
    root: VolumeRoot,
    usage: RecycleBinUsage,
    created_at: EpochMillis,
}

impl PreparedEmpty {
    pub(super) fn new(
        id: u64,
        root: VolumeRoot,
        usage: RecycleBinUsage,
        created_at: EpochMillis,
    ) -> Self {
        Self {
            id,
            root,
            usage,
            created_at,
        }
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn root(&self) -> VolumeRoot {
        self.root
    }

    /// Lo que se midio y se le mostro al usuario.
    pub fn usage(&self) -> RecycleBinUsage {
        self.usage
    }

    pub fn created_at(&self) -> EpochMillis {
        self.created_at
    }

    pub fn expires_at(&self) -> EpochMillis {
        EpochMillis(self.created_at.0.saturating_add(EMPTY_REQUEST_TTL_MS))
    }

    fn is_expired(&self, now: EpochMillis) -> bool {
        now.0.saturating_sub(self.created_at.0) > EMPTY_REQUEST_TTL_MS
    }

    /// Primera confirmacion: consume la solicitud. `token` prueba que el segundo paso ocurre
    /// despues del primero (no es un secreto; D-01).
    pub fn acknowledge(
        self,
        now: EpochMillis,
        token: u64,
    ) -> Result<AcknowledgedEmpty, EmptyRequestError> {
        if self.is_expired(now) {
            return Err(EmptyRequestError::Expired);
        }
        Ok(AcknowledgedEmpty {
            prepared: self,
            token,
        })
    }
}

/// Solicitud con la primera confirmacion hecha: lista para la segunda.
#[derive(Debug)]
pub struct AcknowledgedEmpty {
    prepared: PreparedEmpty,
    token: u64,
}

impl AcknowledgedEmpty {
    pub fn id(&self) -> u64 {
        self.prepared.id
    }

    pub fn token(&self) -> u64 {
        self.token
    }

    pub fn root(&self) -> VolumeRoot {
        self.prepared.root
    }

    pub fn usage(&self) -> RecycleBinUsage {
        self.prepared.usage
    }

    /// Segunda confirmacion: el token debe ser el entregado al reconocer y la solicitud seguir
    /// vigente. El token se evalua primero (un token ajeno nunca revela si vencio).
    pub fn check(&self, token: u64, now: EpochMillis) -> Result<(), EmptyRequestError> {
        if token != self.token {
            return Err(EmptyRequestError::NotFound);
        }
        if self.prepared.is_expired(now) {
            return Err(EmptyRequestError::Expired);
        }
        Ok(())
    }
}

/// Comprobante de la primera confirmacion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AckReceipt {
    pub request_id: u64,
    pub token: u64,
    pub usage: RecycleBinUsage,
    pub expires_at: EpochMillis,
}

#[derive(Debug)]
enum Stage {
    Prepared(PreparedEmpty),
    Acknowledged(AcknowledgedEmpty),
}

/// La unica solicitud vigente: preparar reemplaza a la anterior (en cualquier etapa).
#[derive(Debug, Default)]
pub struct EmptySlot {
    stage: Option<Stage>,
}

impl EmptySlot {
    /// Guarda `request` e invalida la anterior.
    pub fn store(&mut self, request: PreparedEmpty) {
        self.stage = Some(Stage::Prepared(request));
    }

    /// Primer paso: exige la solicitud `Prepared` con ese id. Un id ajeno o una solicitud ya
    /// reconocida fallan con `NotFound` sin tocar el slot; una vencida queda consumida.
    pub fn acknowledge(
        &mut self,
        id: u64,
        now: EpochMillis,
        token: u64,
    ) -> Result<AckReceipt, EmptyRequestError> {
        match &self.stage {
            Some(Stage::Prepared(prepared)) if prepared.id == id => {}
            _ => return Err(EmptyRequestError::NotFound),
        }
        let Some(Stage::Prepared(prepared)) = self.stage.take() else {
            return Err(EmptyRequestError::NotFound);
        };
        let acknowledged = prepared.acknowledge(now, token)?;
        let receipt = AckReceipt {
            request_id: acknowledged.id(),
            token: acknowledged.token(),
            usage: acknowledged.usage(),
            expires_at: acknowledged.prepared.expires_at(),
        };
        self.stage = Some(Stage::Acknowledged(acknowledged));
        Ok(receipt)
    }

    /// Segundo paso: TOMA la solicitud (el slot queda vacio pase lo que pase). Id distinto o slot
    /// vacio -> `NotFound`; sin reconocer -> `NotAcknowledged`.
    pub fn take_for_empty(&mut self, id: u64) -> Result<AcknowledgedEmpty, EmptyRequestError> {
        match self.stage.take() {
            Some(Stage::Acknowledged(ack)) if ack.id() == id => Ok(ack),
            Some(Stage::Prepared(prepared)) if prepared.id == id => {
                Err(EmptyRequestError::NotAcknowledged)
            }
            _ => Err(EmptyRequestError::NotFound),
        }
    }
}

#[cfg(test)]
#[path = "request_tests.rs"]
mod tests;

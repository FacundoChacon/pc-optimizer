//! Errores de la aplicacion, serializables hacia el frontend como `{ kind, message }`.

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

/// Error de aplicacion devuelto por los comandos IPC.
///
/// Cada variante tiene un `kind` estable en `snake_case`; el `message` es el `Display`.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// Fallo interno inesperado.
    #[error("internal error: {0}")]
    Internal(String),
}

impl AppError {
    fn kind(&self) -> &'static str {
        match self {
            Self::Internal(_) => "internal",
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct("AppError", 2)?;
        state.serialize_field("kind", self.kind())?;
        state.serialize_field("message", &self.to_string())?;
        state.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn internal_should_serialize_with_stable_kind_and_display_message() {
        let err = AppError::Internal("x".into());
        let value = serde_json::to_value(&err).unwrap();
        assert_eq!(
            value,
            json!({ "kind": "internal", "message": err.to_string() })
        );
        assert!(err.to_string().contains('x'));
    }

    #[test]
    fn different_messages_should_yield_different_messages_and_same_kind() {
        let a = serde_json::to_value(AppError::Internal("uno".into())).unwrap();
        let b = serde_json::to_value(AppError::Internal("dos".into())).unwrap();
        assert_ne!(a["message"], b["message"]);
        assert_eq!(a["kind"], b["kind"]);
        assert_eq!(a["kind"], "internal");
    }
}

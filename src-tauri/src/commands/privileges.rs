//! Comando `get_privilege_context`: el contexto de privilegios calculado al arrancar.

use serde::Serialize;
use tauri::State;

use crate::domain::privileges::{PrivilegeContext, UserMatch};

/// Contexto de privilegios para el frontend (D-12): solo estado, motivo y elevacion.
///
/// Nunca incluye SID, nombres de cuenta ni rutas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PrivilegeContextDto {
    Match {
        elevated: bool,
    },
    Mismatch {
        reason: &'static str,
        elevated: bool,
    },
}

impl From<PrivilegeContext> for PrivilegeContextDto {
    fn from(context: PrivilegeContext) -> Self {
        let elevated = context.elevated();
        match context.user_match() {
            UserMatch::Match => Self::Match { elevated },
            UserMatch::Mismatch(reason) => Self::Mismatch {
                reason: reason.as_str(),
                elevated,
            },
        }
    }
}

/// Comando IPC: contexto de privilegios (calculado una vez; no consulta el SO).
#[tauri::command]
pub async fn get_privilege_context(
    context: State<'_, PrivilegeContext>,
) -> Result<PrivilegeContextDto, crate::error::AppError> {
    Ok((*context.inner()).into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::privileges::MismatchReason;
    use serde_json::json;

    fn dto(user_match: UserMatch, elevated: bool) -> serde_json::Value {
        serde_json::to_value(PrivilegeContextDto::from(PrivilegeContext::new(
            user_match, elevated,
        )))
        .unwrap()
    }

    #[test]
    fn dto_should_serialize_the_match_shapes() {
        assert_eq!(
            dto(UserMatch::Match, true),
            json!({"status":"match","elevated":true})
        );
        assert_eq!(
            dto(UserMatch::Match, false),
            json!({"status":"match","elevated":false})
        );
    }

    #[test]
    fn dto_should_serialize_the_mismatch_shapes_with_reason() {
        assert_eq!(
            dto(UserMatch::Mismatch(MismatchReason::DifferentUser), true),
            json!({"status":"mismatch","reason":"different_user","elevated":true})
        );
        assert_eq!(
            dto(UserMatch::Mismatch(MismatchReason::Undetermined), false),
            json!({"status":"mismatch","reason":"undetermined","elevated":false})
        );
    }

    #[test]
    fn dto_json_should_not_leak_identities_or_paths() {
        for user_match in [
            UserMatch::Match,
            UserMatch::Mismatch(MismatchReason::DifferentUser),
            UserMatch::Mismatch(MismatchReason::Undetermined),
        ] {
            let text = dto(user_match, true).to_string();
            assert!(!text.contains("S-1-"), "{text}");
            assert!(!text.contains(char::from(0x5c)), "{text}");
        }
    }
}

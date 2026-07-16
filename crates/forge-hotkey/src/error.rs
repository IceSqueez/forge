use thiserror::Error;

#[derive(Debug, Error)]
pub enum HotkeyError {
    #[error("combo already registered: {combo:?}")]
    AlreadyRegistered { combo: String },

    #[error("invalid combo string: {0:?}")]
    InvalidCombo(String),

    #[error("portal unavailable: {reason}")]
    PortalUnavailable { reason: String },

    #[error("permission denied - ensure user is in the 'input' group")]
    PermissionDenied,

    #[error("backend error: {0}")]
    Backend(String),
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn already_registered_display_contains_combo() {
        let e = HotkeyError::AlreadyRegistered {
            combo: "Ctrl+A".to_owned(),
        };
        assert!(e.to_string().contains("Ctrl+A"));
    }

    #[test]
    fn invalid_combo_display_contains_input() {
        let e = HotkeyError::InvalidCombo("bad+".to_owned());
        assert!(e.to_string().contains("bad+"));
    }

    #[test]
    fn portal_unavailable_display_contains_reason() {
        let e = HotkeyError::PortalUnavailable {
            reason: "no D-Bus".to_owned(),
        };
        assert!(e.to_string().contains("no D-Bus"));
    }

    #[test]
    fn permission_denied_display_mentions_input_group() {
        let e = HotkeyError::PermissionDenied;
        assert!(e.to_string().contains("input"));
    }

    #[test]
    fn backend_display_contains_message() {
        let e = HotkeyError::Backend("channel closed".to_owned());
        assert!(e.to_string().contains("channel closed"));
    }
}

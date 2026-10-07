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

    #[error("the desktop did not bind {combo:?}: {reason}")]
    BindRejected { combo: String, reason: String },

    #[error("backend error: {0}")]
    Backend(String),

    #[error("hotkey supervisor task is not running")]
    SupervisorUnavailable,

    #[error("the main-thread hotkey host is not running")]
    MainThreadUnavailable,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_carries_every_field_of_the_variant() {
        let bind_rejected = HotkeyError::BindRejected {
            combo: "Ctrl+F1".to_owned(),
            reason: "BindShortcuts was cancelled on the desktop".to_owned(),
        };
        for (error, fields) in [
            (
                HotkeyError::AlreadyRegistered {
                    combo: "Ctrl+A".to_owned(),
                },
                vec!["Ctrl+A"],
            ),
            (HotkeyError::InvalidCombo("bad+".to_owned()), vec!["bad+"]),
            (
                HotkeyError::PortalUnavailable {
                    reason: "no D-Bus".to_owned(),
                },
                vec!["no D-Bus"],
            ),
            (
                bind_rejected,
                vec!["Ctrl+F1", "BindShortcuts was cancelled on the desktop"],
            ),
            (
                HotkeyError::Backend("channel closed".to_owned()),
                vec!["channel closed"],
            ),
        ] {
            let shown = error.to_string();
            for field in fields {
                assert!(shown.contains(field), "{shown:?} lacks {field:?}");
            }
        }
    }
}

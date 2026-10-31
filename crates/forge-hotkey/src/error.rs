use forge_storage::StorageError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum HotkeyError {
    #[error("combo already registered: {combo:?}")]
    AlreadyRegistered { combo: String },

    #[error("invalid combo string: {0:?}")]
    InvalidCombo(String),

    #[error("portal unavailable: {reason}")]
    PortalUnavailable { reason: String },

    #[error("permission denied reading the keyboard")]
    PermissionDenied,

    #[error("the desktop did not bind {combo:?}: {reason}")]
    BindRejected { combo: String, reason: String },

    #[error("{method} was cancelled on the desktop")]
    PortalCancelled { method: &'static str },

    #[error("{method} was ended by the desktop (response {code})")]
    PortalEnded { method: &'static str, code: u32 },

    #[error("{0}")]
    PortalCall(String),

    #[error("CreateSession returned no usable session_handle")]
    PortalSessionMissing,

    #[error("CreateSession failed: {0}")]
    PortalSessionNotRecreated(Box<HotkeyError>),

    #[error("backend error: {0}")]
    Backend(String),

    #[error("hotkey supervisor task is not running")]
    SupervisorUnavailable,

    #[error("the main-thread hotkey host is not running")]
    MainThreadUnavailable,

    #[error("no global shortcut backend is available")]
    BackendUnavailable,

    #[error("hotkey storage failed: {0}")]
    Storage(#[from] StorageError),

    #[error("the hotkey binding no longer exists")]
    BindingNotFound,

    #[error("{combo} already has a binding on that edge")]
    EdgeTaken { combo: String },
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
            (
                HotkeyError::Storage(StorageError::Connection {
                    reason: "disk full".to_owned(),
                }),
                vec!["disk full"],
            ),
            (
                HotkeyError::EdgeTaken {
                    combo: "Ctrl+F2".to_owned(),
                },
                vec!["Ctrl+F2"],
            ),
        ] {
            let shown = error.to_string();
            for field in fields {
                assert!(shown.contains(field), "{shown:?} lacks {field:?}");
            }
        }
    }

    #[test]
    fn portal_failures_render_the_wording_the_session_lost_detail_shows() {
        for (error, expected) in [
            (
                HotkeyError::PortalCancelled {
                    method: "BindShortcuts",
                },
                "BindShortcuts was cancelled on the desktop",
            ),
            (
                HotkeyError::PortalEnded {
                    method: "CreateSession",
                    code: 2,
                },
                "CreateSession was ended by the desktop (response 2)",
            ),
            (
                HotkeyError::PortalCall("org.freedesktop.DBus.Error.NoReply".to_owned()),
                "org.freedesktop.DBus.Error.NoReply",
            ),
            (
                HotkeyError::PortalSessionMissing,
                "CreateSession returned no usable session_handle",
            ),
            (
                HotkeyError::PortalSessionNotRecreated(Box::new(HotkeyError::PortalEnded {
                    method: "CreateSession",
                    code: 2,
                })),
                "CreateSession failed: CreateSession was ended by the desktop (response 2)",
            ),
        ] {
            assert_eq!(error.to_string(), expected);
        }
    }
}

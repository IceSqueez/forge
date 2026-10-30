use forge_types::Secret;

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("could not bind to {addr}: {reason}")]
    Bind { addr: String, reason: String },

    #[error("storage error: {0}")]
    Storage(#[from] forge_storage::StorageError),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("refusing to bind to {addr}: server.lan_bind_enabled is false")]
    LanBindNotEnabled { addr: String },

    #[error("overlay pages could not be rewritten with the new credentials: {reason}")]
    OverlayRematerialization { reason: String },

    #[error("the overlay token was regenerated but its pages could not be rewritten: {reason}")]
    OverlayRotationIncomplete { token: Secret, reason: String },

    #[error(
        "the previous overlay token could not be restored after a failed regenerate, so the new token is now the active one: {reason}"
    )]
    OverlayTokenRestoreFailed { token: Secret, reason: String },

    #[error("the server is switched off in settings")]
    Disabled,

    #[error("audio clip of {bytes} bytes is over the {ceiling} byte per-clip ceiling")]
    ClipTooLarge { bytes: usize, ceiling: usize },

    #[error("audio clip of {bytes} bytes does not fit the {budget} byte clip budget")]
    ClipBudgetExhausted { bytes: usize, budget: usize },
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIVE_TOKEN: &str = "fgo_q8Vn2LxR0tYb7WcZ4sKe";

    #[test]
    fn debug_of_an_error_holding_the_overlay_token_never_carries_it() {
        for error in [
            ServerError::OverlayRotationIncomplete {
                token: Secret::new(LIVE_TOKEN),
                reason: "2 overlay pages failed to rewrite".into(),
            },
            ServerError::OverlayTokenRestoreFailed {
                token: Secret::new(LIVE_TOKEN),
                reason: "database is locked; disk is full".into(),
            },
        ] {
            let debug = format!("{error:?}");
            assert!(!debug.contains(LIVE_TOKEN), "token leaked: {debug}");
        }
    }
}

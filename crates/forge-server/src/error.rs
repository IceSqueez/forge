#[derive(Clone, PartialEq, Eq)]
pub struct SecretToken(String);

impl SecretToken {
    pub fn new(token: String) -> Self {
        Self(token)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

impl std::ops::Deref for SecretToken {
    type Target = str;

    fn deref(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for SecretToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<redacted>")
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("could not bind to {addr}: {reason}")]
    Bind { addr: String, reason: String },

    #[error("storage error: {0}")]
    Storage(String),

    #[error("authentication required for this request")]
    AuthRequired,

    #[error("authentication rejected: {reason}")]
    AuthInvalid { reason: String },

    #[error("unknown request method: {request}")]
    UnknownRequest { request: String },

    #[error("malformed frame: {reason}")]
    MalformedFrame { reason: String },

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("refusing to bind to {addr}: server.lan_bind_enabled is false")]
    LanBindNotEnabled { addr: String },

    #[error(
        "refusing to bind to {addr}: bearer token missing - generate one before exposing the server"
    )]
    NoTokenForLanBind { addr: String },

    #[error("overlay pages could not be rewritten with the new credentials: {reason}")]
    OverlayRematerialization { reason: String },

    #[error("the overlay token was regenerated but its pages could not be rewritten: {reason}")]
    OverlayRotationIncomplete { token: SecretToken, reason: String },

    #[error(
        "the previous overlay token could not be restored after a failed regenerate, so the new token is now the active one: {reason}"
    )]
    OverlayTokenRestoreFailed { token: SecretToken, reason: String },

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

    #[test]
    fn auth_invalid_display_carries_reason() {
        let e = ServerError::AuthInvalid {
            reason: "bad token".into(),
        };
        assert!(e.to_string().contains("bad token"));
    }
}

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

    #[error("path traversal blocked: {requested}")]
    PathTraversal { requested: String },

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
        "refusing to bind to {addr}: bearer token missing — generate one before exposing the server"
    )]
    NoTokenForLanBind { addr: String },
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

    #[test]
    fn path_traversal_display_carries_requested_path() {
        let path = "../etc/passwd".to_owned();
        let e = ServerError::PathTraversal {
            requested: path.clone(),
        };
        assert!(e.to_string().contains(&path));
    }
}

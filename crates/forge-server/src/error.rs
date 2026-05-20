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
    use std::error::Error;

    #[test]
    fn bind_display_non_empty() {
        let e = ServerError::Bind {
            addr: "0.0.0.0:9000".into(),
            reason: "address in use".into(),
        };
        assert!(!e.to_string().is_empty());
    }

    #[test]
    fn auth_required_display_constant() {
        let e = ServerError::AuthRequired;
        assert_eq!(e.to_string(), "authentication required for this request");
    }

    #[test]
    fn auth_invalid_display_non_empty() {
        let e = ServerError::AuthInvalid {
            reason: "bad token".into(),
        };
        assert!(!e.to_string().is_empty());
        assert!(e.to_string().contains("bad token"));
    }

    #[test]
    fn path_traversal_carries_requested_path() {
        let path = "../etc/passwd".to_owned();
        let e = ServerError::PathTraversal {
            requested: path.clone(),
        };
        assert!(e.to_string().contains(&path));
    }

    #[test]
    fn unknown_request_display_non_empty() {
        let e = ServerError::UnknownRequest {
            request: "doEvil".into(),
        };
        assert!(!e.to_string().is_empty());
    }

    #[test]
    fn malformed_frame_display_non_empty() {
        let e = ServerError::MalformedFrame {
            reason: "missing id field".into(),
        };
        assert!(!e.to_string().is_empty());
    }

    #[test]
    fn serialization_from_serde_json() {
        if let Err(inner) = serde_json::from_str::<serde_json::Value>("{bad}") {
            let e = ServerError::from(inner);
            assert!(!e.to_string().is_empty());
        }
    }

    #[test]
    fn io_from_std_io() {
        let inner = std::io::Error::new(std::io::ErrorKind::NotFound, "file gone");
        let e = ServerError::from(inner);
        assert!(!e.to_string().is_empty());
    }

    #[test]
    fn implements_std_error_trait() {
        let e: &dyn Error = &ServerError::AuthRequired;
        let _ = e.to_string();
    }
}

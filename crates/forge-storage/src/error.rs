#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("connection failed: {reason}")]
    Connection { reason: String },

    #[error("migration '{migration}' failed: {reason}")]
    Migration { migration: String, reason: String },

    #[error("encryption failed")]
    Encryption,

    #[error("decryption failed")]
    Decryption,

    #[error("schema version mismatch: expected {expected}, found {found}")]
    SchemaMismatch { expected: u32, found: u32 },

    #[error("key not found: {key}")]
    NotFound { key: String },

    #[error("global '{name}' has type {actual}, expected numeric")]
    TypeMismatch { name: String, actual: String },

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("validation failed for '{field}': {reason}")]
    ValidationFailed { field: String, reason: String },

    #[error("cannot delete trigger instance: still referenced by {used_in_count} action(s)")]
    ReferenceBlock {
        used_in_count: u32,
        sample_action_names: Vec<String>,
    },

    #[error("bundle bytes are not valid JSON or do not match the expected envelope shape")]
    MalformedBundle,

    #[error(
        "bundle format version {found} is below the minimum supported version \
         {minimum_supported}; upgrade the bundle to continue"
    )]
    BundleVersionTooOld { found: u32, minimum_supported: u32 },

    #[error("parse error: {0}")]
    Parse(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("operation not ready: implementation pending")]
    NotReady,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn assert_error<E: std::error::Error>(_: &E) {}

    fn make_serde_error() -> serde_json::Error {
        serde_json::from_str::<()>("bad").unwrap_err()
    }

    #[test]
    fn all_variants_display_non_empty() {
        let variants: &[StorageError] = &[
            StorageError::Connection {
                reason: "refused".into(),
            },
            StorageError::Migration {
                migration: "0001_init".into(),
                reason: "locked".into(),
            },
            StorageError::Encryption,
            StorageError::Decryption,
            StorageError::SchemaMismatch {
                expected: 3,
                found: 1,
            },
            StorageError::NotFound {
                key: "theme".into(),
            },
            StorageError::TypeMismatch {
                name: "counter".into(),
                actual: "string".into(),
            },
            StorageError::ValidationFailed {
                field: "server.bind_address".into(),
                reason: "must be one of: 127.0.0.1, 0.0.0.0".into(),
            },
            StorageError::Serialization(make_serde_error()),
            StorageError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, "missing")),
        ];
        for v in variants {
            assert!(
                !v.to_string().is_empty(),
                "variant displayed empty: {:?}",
                v
            );
        }
    }

    #[test]
    fn schema_mismatch_carries_both_fields() {
        let err = StorageError::SchemaMismatch {
            expected: 5,
            found: 2,
        };
        let msg = err.to_string();
        assert!(msg.contains("5"), "expected not in message: {msg}");
        assert!(msg.contains("2"), "found not in message: {msg}");
    }

    #[test]
    fn storage_error_implements_std_error() {
        let err = StorageError::Decryption;
        assert_error(&err);
    }
}

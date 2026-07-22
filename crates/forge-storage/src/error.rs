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

    #[error("name '{name}' is already in use")]
    NameCollision { name: String },

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

    #[error("parse error: {0}")]
    Parse(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("operation not ready: implementation pending")]
    NotReady,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_mismatch_display_carries_both_fields() {
        let err = StorageError::SchemaMismatch {
            expected: 5,
            found: 2,
        };
        let msg = err.to_string();
        assert!(msg.contains("5"), "expected not in message: {msg}");
        assert!(msg.contains("2"), "found not in message: {msg}");
    }
}

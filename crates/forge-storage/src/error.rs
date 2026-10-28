use crate::media::{MediaFormat, MediaKind};

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

    #[error(
        "database predates the baseline schema (legacy version {found}); open it once with forge {release} first",
        release = crate::LAST_PRE_BASELINE_RELEASE
    )]
    PreBaselineSchema { found: u32 },

    #[error("key not found: {key}")]
    NotFound { key: String },

    #[error("name '{name}' is already in use")]
    NameCollision { name: String },

    #[error("this viewer already has a different voice alias")]
    AliasViewerTaken,

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

    #[error("the default queue cannot be deleted")]
    DefaultQueueUndeletable,

    #[error("'{label}' is not an accepted audio or image file")]
    MediaUnsupported { label: String },

    #[error("'{label}' is named {claimed} but its content is {detected}")]
    MediaTypeMismatch {
        label: String,
        claimed: MediaFormat,
        detected: MediaFormat,
    },

    #[error("'{label}' is {size} bytes, over the {limit} byte limit for {kind} files")]
    MediaTooLarge {
        label: String,
        size: u64,
        limit: u64,
        kind: MediaKind,
    },

    #[error("cannot delete media: still referenced by {referrer_count} item(s)")]
    MediaReferenced { referrer_count: u32 },

    #[error("'{raw}' is not a media blob id")]
    MalformedMediaBlobId { raw: String },

    #[error("parse error: {0}")]
    Parse(String),

    #[error("stored setting '{key}' cannot be decoded")]
    UndecodableSetting { key: String },

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("operation not ready: implementation pending")]
    NotReady,
}

impl StorageError {
    pub fn stored_secret_unrecoverable(&self) -> bool {
        matches!(self, Self::Decryption)
    }
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

    #[test]
    fn only_a_decryption_failure_marks_the_stored_secret_unrecoverable() {
        let text = || "x".to_owned();
        let cases = [
            (StorageError::Decryption, true),
            (StorageError::Connection { reason: text() }, false),
            (
                StorageError::Migration {
                    migration: text(),
                    reason: text(),
                },
                false,
            ),
            (StorageError::Encryption, false),
            (
                StorageError::SchemaMismatch {
                    expected: 2,
                    found: 1,
                },
                false,
            ),
            (StorageError::PreBaselineSchema { found: 1 }, false),
            (StorageError::NotFound { key: text() }, false),
            (StorageError::NameCollision { name: text() }, false),
            (StorageError::AliasViewerTaken, false),
            (
                StorageError::TypeMismatch {
                    name: text(),
                    actual: text(),
                },
                false,
            ),
            (
                StorageError::Serialization(
                    serde_json::from_str::<serde_json::Value>("{").unwrap_err(),
                ),
                false,
            ),
            (
                StorageError::ValidationFailed {
                    field: text(),
                    reason: text(),
                },
                false,
            ),
            (
                StorageError::ReferenceBlock {
                    used_in_count: 1,
                    sample_action_names: vec![text()],
                },
                false,
            ),
            (StorageError::DefaultQueueUndeletable, false),
            (StorageError::MediaUnsupported { label: text() }, false),
            (
                StorageError::MediaTypeMismatch {
                    label: text(),
                    claimed: MediaFormat::Png,
                    detected: MediaFormat::Wav,
                },
                false,
            ),
            (
                StorageError::MediaTooLarge {
                    label: text(),
                    size: 2,
                    limit: 1,
                    kind: MediaKind::Image,
                },
                false,
            ),
            (StorageError::MediaReferenced { referrer_count: 1 }, false),
            (StorageError::MalformedMediaBlobId { raw: text() }, false),
            (StorageError::Parse(text()), false),
            (StorageError::UndecodableSetting { key: text() }, false),
            (StorageError::Io(std::io::Error::other("disk")), false),
            (StorageError::NotReady, false),
        ];
        for (error, expected) in cases {
            assert_eq!(
                error.stored_secret_unrecoverable(),
                expected,
                "for {error:?}"
            );
        }
    }
}

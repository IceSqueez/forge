use forge_components::{fmt_bytes, tr};
use forge_soundboard::{ClipRefusal, SoundboardError};
use forge_storage::StorageError;

pub(crate) fn failure_message(error: &SoundboardError) -> String {
    match error {
        SoundboardError::ImportRefused(refusal) => refusal_message(refusal),
        SoundboardError::SourceMissing(name) => {
            tr!("soundboard_error_source_missing", name = name.as_str())
        }
        SoundboardError::ClipNotFound(_) => tr!("soundboard_error_clip_gone"),
        other => other.to_string(),
    }
}

pub(crate) fn clip_refusal_message(refusal: &ClipRefusal) -> String {
    refusal_message(&match refusal {
        ClipRefusal::Unsupported { label } => StorageError::MediaUnsupported {
            label: label.clone(),
        },
        ClipRefusal::TypeMismatch {
            label,
            claimed,
            detected,
        } => StorageError::MediaTypeMismatch {
            label: label.clone(),
            claimed: *claimed,
            detected: *detected,
        },
        ClipRefusal::TooLarge {
            label,
            size,
            limit,
            kind,
        } => StorageError::MediaTooLarge {
            label: label.clone(),
            size: *size,
            limit: *limit,
            kind: *kind,
        },
    })
}

fn refusal_message(refusal: &StorageError) -> String {
    match refusal {
        StorageError::MediaUnsupported { label } => {
            tr!("soundboard_import_unsupported", file = label.as_str())
        }
        StorageError::MediaTypeMismatch {
            label,
            claimed,
            detected,
        } => tr!(
            "soundboard_import_type_mismatch",
            file = label.as_str(),
            named = claimed.as_str(),
            detected = detected.as_str()
        ),
        StorageError::MediaTooLarge {
            label, size, limit, ..
        } => {
            let size = fmt_bytes(*size);
            let limit = fmt_bytes(*limit);
            tr!(
                "soundboard_import_too_large",
                file = label.as_str(),
                size = size.as_str(),
                limit = limit.as_str()
            )
        }
        other => other.to_string(),
    }
}

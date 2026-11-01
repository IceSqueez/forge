use forge_components::{fmt_bytes, tr};
use forge_soundboard::SoundboardError;
use forge_storage::{MediaRefusal, StorageError};

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

pub(crate) fn storage_refusal_message(error: &StorageError) -> String {
    match error {
        StorageError::MediaRefused(refusal) => refusal_message(refusal),
        other => other.to_string(),
    }
}

pub(crate) fn refusal_message(refusal: &MediaRefusal) -> String {
    match refusal {
        MediaRefusal::Unsupported { label } => {
            tr!("soundboard_import_unsupported", file = label.as_str())
        }
        MediaRefusal::TypeMismatch {
            label,
            claimed,
            detected,
        } => tr!(
            "soundboard_import_type_mismatch",
            file = label.as_str(),
            named = claimed.as_str(),
            detected = detected.as_str()
        ),
        MediaRefusal::TooLarge {
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
    }
}

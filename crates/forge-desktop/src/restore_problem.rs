use forge_components::tr;
use forge_transfer::TransferError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RestoreProblem {
    NotABundle,
    UnknownKind,
    NewerFormat { found: u32, supported: u32 },
    NewerSchema { found: u32, supported: u32 },
    SchemaTooOld { found: u32, release: &'static str },
    UnrecognizedDatabase,
    UnknownEncryption,
    WrongPassphrase,
    Damaged,
    MissingCredentialsKey,
    UpdateInProgress,
    RestoreAlreadyPending,
    Unavailable,
    Failed(String),
}

impl RestoreProblem {
    pub(crate) fn of(error: &TransferError) -> Self {
        match error {
            TransferError::NotABundle => Self::NotABundle,
            TransferError::UnknownKind { .. } => Self::UnknownKind,
            TransferError::NewerFormat { found, supported } => Self::NewerFormat {
                found: *found,
                supported: *supported,
            },
            TransferError::NewerSchema { found, supported } => Self::NewerSchema {
                found: *found,
                supported: *supported,
            },
            TransferError::SchemaNotAdoptable { found, release } => Self::SchemaTooOld {
                found: *found,
                release,
            },
            TransferError::UnrecognizedDatabase => Self::UnrecognizedDatabase,
            TransferError::UnknownEncryption { .. } => Self::UnknownEncryption,
            TransferError::WrongPassphrase => Self::WrongPassphrase,
            TransferError::Damaged { .. }
            | TransferError::UnsafeEntry { .. }
            | TransferError::EntryTooLarge { .. }
            | TransferError::ArchiveTooLarge { .. }
            | TransferError::TooManyEntries { .. }
            | TransferError::DigestMismatch { .. }
            | TransferError::ContentsMismatch { .. }
            | TransferError::MissingDatabase
            | TransferError::ExcessiveWorkFactor { .. } => Self::Damaged,
            TransferError::MissingCredentialsKey => Self::MissingCredentialsKey,
            TransferError::UpdateInProgress => Self::UpdateInProgress,
            TransferError::RestoreAlreadyPending => Self::RestoreAlreadyPending,
            other => Self::Failed(other.to_string()),
        }
    }

    pub(crate) fn message(&self) -> String {
        match self {
            Self::NotABundle => tr!("restore_problem_not_a_bundle"),
            Self::UnknownKind => tr!("restore_problem_unknown_kind"),
            Self::NewerFormat { found, supported } => tr!(
                "restore_problem_newer_format",
                found = found.to_string(),
                supported = supported.to_string()
            ),
            Self::NewerSchema { found, supported } => tr!(
                "restore_problem_newer_schema",
                found = found.to_string(),
                supported = supported.to_string()
            ),
            Self::SchemaTooOld { found, release } => tr!(
                "restore_problem_schema_too_old",
                found = found.to_string(),
                release = *release
            ),
            Self::UnrecognizedDatabase => tr!("restore_problem_unrecognized_database"),
            Self::UnknownEncryption => tr!("restore_problem_unknown_encryption"),
            Self::WrongPassphrase => tr!("restore_problem_wrong_passphrase"),
            Self::Damaged => tr!("restore_problem_damaged"),
            Self::MissingCredentialsKey => tr!("restore_problem_missing_key"),
            Self::UpdateInProgress => tr!("restore_problem_update_in_progress"),
            Self::RestoreAlreadyPending => tr!("restore_problem_already_pending"),
            Self::Unavailable => tr!("restore_problem_unavailable"),
            Self::Failed(error) => tr!("restore_problem_failed", error = error.as_str()),
        }
    }
}

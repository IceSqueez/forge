use forge_components::tr;
use forge_transfer::{BundleKind, TransferError};

use crate::user_text::resolve_message;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RestoreProblem {
    NotABundle,
    UnknownKind,
    KindMismatch {
        expected: BundleKind,
        found: BundleKind,
    },
    NewerDocuments {
        found: u32,
        supported: u32,
    },
    NewerFormat {
        found: u32,
        supported: u32,
    },
    NewerSchema {
        found: u32,
        supported: u32,
    },
    SchemaTooOld {
        found: u32,
        release: &'static str,
    },
    UnrecognizedDatabase,
    UnknownEncryption,
    PassphraseRequired,
    PassphraseTooShort {
        minimum: usize,
    },
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
        Self::classified(error)
            .unwrap_or_else(|| Self::Failed(resolve_message(&error.user_message()).inline()))
    }

    pub(crate) fn classified(error: &TransferError) -> Option<Self> {
        Some(match error {
            TransferError::NotABundle => Self::NotABundle,
            TransferError::UnknownKind { .. } => Self::UnknownKind,
            TransferError::KindMismatch { expected, found } => Self::KindMismatch {
                expected: *expected,
                found: *found,
            },
            TransferError::NewerPackDocuments { found, supported }
            | TransferError::NewerSetupDocuments { found, supported } => Self::NewerDocuments {
                found: *found,
                supported: *supported,
            },
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
            TransferError::PassphraseRequired => Self::PassphraseRequired,
            TransferError::PassphraseTooShort { minimum } => {
                Self::PassphraseTooShort { minimum: *minimum }
            }
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
            _ => return None,
        })
    }

    pub(crate) fn plain_text(&self) -> String {
        match self {
            Self::Failed(text) => text.clone(),
            other => other.message(),
        }
    }

    pub(crate) fn message(&self) -> String {
        match self {
            Self::NotABundle => tr!("restore_problem_not_a_bundle"),
            Self::UnknownKind => tr!("restore_problem_unknown_kind"),
            Self::KindMismatch { expected, found } => tr!(
                "restore_problem_kind_mismatch",
                found = kind_name(*found),
                expected = kind_name(*expected)
            ),
            Self::NewerDocuments { found, supported } => tr!(
                "restore_problem_newer_documents",
                found = found.to_string(),
                supported = supported.to_string()
            ),
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
            Self::PassphraseRequired => tr!("restore_problem_passphrase_required"),
            Self::PassphraseTooShort { minimum } => tr!(
                "restore_problem_passphrase_too_short",
                min = minimum.to_string()
            ),
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

fn kind_name(kind: BundleKind) -> String {
    match kind {
        BundleKind::Full => tr!("bundle_kind_full"),
        BundleKind::Pack => tr!("bundle_kind_pack"),
        BundleKind::Setup => tr!("bundle_kind_setup"),
    }
}

#[cfg(test)]
mod tests {
    use forge_storage::Language;
    use forge_transfer::{BundleKind, TransferError};

    use super::RestoreProblem;
    use crate::i18n::install_language;

    #[test]
    fn every_integrity_failure_is_reported_as_a_damaged_backup() {
        for error in [
            TransferError::Damaged {
                reason: "truncated".to_owned(),
            },
            TransferError::UnsafeEntry {
                path: "../escape".to_owned(),
                reason: "parent component",
            },
            TransferError::EntryTooLarge {
                path: "forge/forge.db".to_owned(),
                size: 2,
                limit: 1,
            },
            TransferError::ArchiveTooLarge { limit: 1 },
            TransferError::TooManyEntries { limit: 1 },
            TransferError::DigestMismatch {
                path: "forge/forge.db".to_owned(),
            },
            TransferError::ContentsMismatch {
                path: "forge/media".to_owned(),
            },
            TransferError::MissingDatabase,
            TransferError::ExcessiveWorkFactor {
                required: 30,
                cap: 20,
            },
        ] {
            assert_eq!(
                RestoreProblem::of(&error),
                RestoreProblem::Damaged,
                "{error:?}"
            );
        }
    }

    #[test]
    fn a_wrong_passphrase_is_never_reported_as_damage() {
        install_language(Language::En);

        let problem = RestoreProblem::of(&TransferError::WrongPassphrase);

        assert_eq!(
            (
                problem.clone(),
                problem.message() == RestoreProblem::Damaged.message()
            ),
            (RestoreProblem::WrongPassphrase, false)
        );
    }

    #[test]
    fn refusals_the_user_can_act_on_keep_their_own_class() {
        for (error, expected) in [
            (TransferError::NotABundle, RestoreProblem::NotABundle),
            (
                TransferError::UnknownKind {
                    kind: "theme".to_owned(),
                },
                RestoreProblem::UnknownKind,
            ),
            (
                TransferError::UnrecognizedDatabase,
                RestoreProblem::UnrecognizedDatabase,
            ),
            (
                TransferError::UnknownEncryption {
                    scheme: "rot13".to_owned(),
                    version: 9,
                },
                RestoreProblem::UnknownEncryption,
            ),
            (
                TransferError::PassphraseRequired,
                RestoreProblem::PassphraseRequired,
            ),
            (
                TransferError::PassphraseTooShort { minimum: 12 },
                RestoreProblem::PassphraseTooShort { minimum: 12 },
            ),
            (
                TransferError::MissingCredentialsKey,
                RestoreProblem::MissingCredentialsKey,
            ),
            (
                TransferError::UpdateInProgress,
                RestoreProblem::UpdateInProgress,
            ),
            (
                TransferError::RestoreAlreadyPending,
                RestoreProblem::RestoreAlreadyPending,
            ),
            (
                TransferError::KindMismatch {
                    expected: BundleKind::Pack,
                    found: BundleKind::Full,
                },
                RestoreProblem::KindMismatch {
                    expected: BundleKind::Pack,
                    found: BundleKind::Full,
                },
            ),
            (
                TransferError::NewerPackDocuments {
                    found: 4,
                    supported: 2,
                },
                RestoreProblem::NewerDocuments {
                    found: 4,
                    supported: 2,
                },
            ),
            (
                TransferError::NewerSetupDocuments {
                    found: 6,
                    supported: 5,
                },
                RestoreProblem::NewerDocuments {
                    found: 6,
                    supported: 5,
                },
            ),
        ] {
            assert_eq!(RestoreProblem::of(&error), expected, "{error:?}");
        }
    }

    #[test]
    fn version_refusals_name_the_found_and_supported_versions() {
        install_language(Language::En);
        for (error, wanted) in [
            (
                TransferError::NewerFormat {
                    found: 7,
                    supported: 3,
                },
                ["7", "3"],
            ),
            (
                TransferError::NewerSchema {
                    found: 91,
                    supported: 58,
                },
                ["91", "58"],
            ),
            (
                TransferError::NewerPackDocuments {
                    found: 4,
                    supported: 2,
                },
                ["4", "2"],
            ),
            (
                TransferError::SchemaNotAdoptable {
                    found: 12,
                    release: "0.4.2",
                },
                ["12", "0.4.2"],
            ),
        ] {
            let message = RestoreProblem::of(&error).message();
            assert!(
                wanted.iter().all(|part| message.contains(part)),
                "{error:?} gave {message:?}"
            );
        }
    }

    #[test]
    fn a_too_short_passphrase_names_the_minimum_length_in_every_language() {
        for language in [Language::En, Language::Uk] {
            install_language(language);

            let message =
                RestoreProblem::of(&TransferError::PassphraseTooShort { minimum: 12 }).message();

            assert!(message.contains("12"), "{language:?}: {message:?}");
        }
    }

    #[test]
    fn unclassified_errors_surface_their_localized_description_with_the_offending_path() {
        install_language(Language::En);
        let error = TransferError::RestoreBlocked {
            path: "media-in-the-way".to_owned(),
        };

        let problem = RestoreProblem::of(&error);

        let RestoreProblem::Failed(text) = &problem else {
            panic!("expected Failed, got {problem:?}");
        };
        assert_eq!(
            text.replace(crate::test_support::FORMAT_ISOLATES, ""),
            "\"media-in-the-way\" is in the way of the restore"
        );
    }

    #[test]
    fn plain_text_is_the_stored_text_for_failures_and_the_message_for_classified_problems() {
        install_language(Language::En);
        let failed = RestoreProblem::Failed("disk on fire".to_owned());
        let classified = RestoreProblem::NotABundle;

        assert_eq!(failed.plain_text(), "disk on fire");
        assert_eq!(classified.plain_text(), classified.message());
    }

    #[test]
    fn a_kind_mismatch_names_the_kind_found_before_the_kind_expected() {
        install_language(Language::En);
        let error = TransferError::KindMismatch {
            expected: BundleKind::Full,
            found: BundleKind::Setup,
        };

        let message = RestoreProblem::of(&error)
            .message()
            .replace(crate::test_support::FORMAT_ISOLATES, "");

        assert!(
            message.contains("a forge setup, not a full backup"),
            "{message:?}"
        );
    }
}

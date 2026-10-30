use std::fmt;

use forge_hotkey::HotkeyError;
use forge_hotkey::vocabulary::messages::failure;
use forge_storage::StorageError;
use forge_types::{MessageCatalog, MessageCode, MessageRef, VerbatimText};

use crate::user_text::resolve_message;

const NAMESPACE: &str = "repo";

const NAME_ARG: &str = "name";
const COUNT_ARG: &str = "count";

const STORAGE_FAILED: MessageCode = MessageCode::from_static(NAMESPACE, "storage_failed");
const ITEM_GONE: MessageCode = MessageCode::from_static(NAMESPACE, "item_gone");
const NAME_TAKEN: MessageCode = MessageCode::from_static(NAMESPACE, "name_taken");
const VALUE_REJECTED: MessageCode = MessageCode::from_static(NAMESPACE, "value_rejected");
const STILL_REFERENCED: MessageCode = MessageCode::from_static(NAMESPACE, "still_referenced");
const TRIGGER_GONE: MessageCode = MessageCode::from_static(NAMESPACE, "trigger_gone");
const ACTION_GONE: MessageCode = MessageCode::from_static(NAMESPACE, "action_gone");
const STEPS_GONE: MessageCode = MessageCode::from_static(NAMESPACE, "steps_gone");
const OPERATION_FAILED: MessageCode = MessageCode::from_static(NAMESPACE, "operation_failed");

pub const REPO_MESSAGES: MessageCatalog = MessageCatalog {
    namespace: NAMESPACE,
    codes: &[
        STORAGE_FAILED,
        ITEM_GONE,
        NAME_TAKEN,
        VALUE_REJECTED,
        STILL_REFERENCED,
        TRIGGER_GONE,
        ACTION_GONE,
        STEPS_GONE,
        OPERATION_FAILED,
    ],
    en_ftl: include_str!("../locales/en/repo_messages.ftl"),
    uk_ftl: include_str!("../locales/uk/repo_messages.ftl"),
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoFailure {
    message: MessageRef,
    diagnostic: String,
}

impl RepoFailure {
    pub fn trigger_gone() -> Self {
        Self {
            message: MessageRef::new(TRIGGER_GONE),
            diagnostic: "trigger instance not found".to_owned(),
        }
    }

    pub fn action_gone() -> Self {
        Self {
            message: MessageRef::new(ACTION_GONE),
            diagnostic: "action not found".to_owned(),
        }
    }

    pub fn steps_gone() -> Self {
        Self {
            message: MessageRef::new(STEPS_GONE),
            diagnostic: "step list not found".to_owned(),
        }
    }

    pub fn operation(diagnostic: impl ToString) -> Self {
        Self {
            message: MessageRef::new(OPERATION_FAILED),
            diagnostic: diagnostic.to_string(),
        }
    }

    pub fn hotkey(error: &HotkeyError) -> Self {
        Self {
            message: failure(error),
            diagnostic: error.to_string(),
        }
    }

    pub fn text(&self) -> String {
        resolve_message(&self.message).primary
    }
}

impl fmt::Display for RepoFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.diagnostic)
    }
}

impl From<StorageError> for RepoFailure {
    fn from(error: StorageError) -> Self {
        Self::from_storage_ref(&error)
    }
}

impl RepoFailure {
    pub fn from_storage_ref(error: &StorageError) -> Self {
        let message = match error {
            StorageError::NotFound { .. } => MessageRef::new(ITEM_GONE),
            StorageError::NameCollision { name } => {
                MessageRef::new(NAME_TAKEN).with_arg(NAME_ARG, VerbatimText::new(name.as_str()))
            }
            StorageError::ValidationFailed { .. } | StorageError::TypeMismatch { .. } => {
                MessageRef::new(VALUE_REJECTED)
            }
            StorageError::ReferenceBlock { used_in_count, .. } => {
                MessageRef::new(STILL_REFERENCED).with_arg(COUNT_ARG, i64::from(*used_in_count))
            }
            _ => MessageRef::new(STORAGE_FAILED),
        };
        Self {
            message,
            diagnostic: error.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use forge_storage::{Language, MediaFormat, MediaKind};

    use super::*;
    use crate::i18n::install_language;

    const STORAGE_FAILED_EN: &str =
        "forge could not read or write its data. Details are in the log.";
    const STORAGE_FAILED_UK: &str =
        "forge не зміг прочитати або записати свої дані. Подробиці в журналі.";

    #[test]
    fn a_hotkey_failure_shows_the_localized_message_and_keeps_the_cause_for_the_log() {
        install_language(Language::En);
        let error = forge_hotkey::HotkeyError::Storage(StorageError::Connection {
            reason: "disk full".to_owned(),
        });

        let failure = RepoFailure::hotkey(&error);

        assert_eq!(failure.to_string(), error.to_string());
        assert_eq!(
            failure.text(),
            "The hotkey binding could not be read or saved"
        );
    }

    fn text_of(error: StorageError) -> String {
        RepoFailure::from(error)
            .text()
            .replace(['\u{2068}', '\u{2069}'], "")
    }

    fn reference_block(used_in_count: u32) -> StorageError {
        StorageError::ReferenceBlock {
            used_in_count,
            sample_action_names: Vec::new(),
        }
    }

    fn text() -> String {
        "x".to_owned()
    }

    fn validation_failed() -> StorageError {
        StorageError::ValidationFailed {
            field: text(),
            reason: text(),
        }
    }

    fn type_mismatch() -> StorageError {
        StorageError::TypeMismatch {
            name: text(),
            actual: text(),
        }
    }

    fn not_found() -> StorageError {
        StorageError::NotFound { key: text() }
    }

    fn name_collision() -> StorageError {
        StorageError::NameCollision {
            name: "Шанувальник".to_owned(),
        }
    }

    fn opaque_errors() -> Vec<StorageError> {
        vec![
            StorageError::Connection { reason: text() },
            StorageError::Migration {
                migration: text(),
                reason: text(),
            },
            StorageError::Encryption,
            StorageError::Decryption,
            StorageError::SchemaMismatch {
                expected: 2,
                found: 1,
            },
            StorageError::PreBaselineSchema { found: 1 },
            StorageError::AliasViewerTaken,
            StorageError::Serialization(serde_json::from_str::<u8>("x").unwrap_err()),
            StorageError::DefaultQueueUndeletable,
            StorageError::MediaUnsupported { label: text() },
            StorageError::MediaTypeMismatch {
                label: text(),
                claimed: MediaFormat::Wav,
                detected: MediaFormat::Png,
            },
            StorageError::MediaTooLarge {
                label: text(),
                size: 2,
                limit: 1,
                kind: MediaKind::Audio,
            },
            StorageError::MediaReferenced { referrer_count: 1 },
            StorageError::MalformedMediaBlobId { raw: text() },
            StorageError::Parse(text()),
            StorageError::UndecodableSetting { key: text() },
            StorageError::DuplicateSettingSpec { key: text() },
            StorageError::UnstorableSetting { key: text() },
            StorageError::InvalidSettingMember {
                family: text(),
                member: text(),
            },
            StorageError::Io(std::io::Error::other("disk gone")),
            StorageError::NotReady,
        ]
    }

    #[test]
    fn each_storage_error_reads_as_its_own_message_in_english() {
        install_language(Language::En);
        for (error, expected) in [
            (
                not_found(),
                "This item no longer exists. Reload the screen and try again.",
            ),
            (
                name_collision(),
                "The name \"Шанувальник\" is already in use.",
            ),
            (
                validation_failed(),
                "This value was not accepted. Check it and try again.",
            ),
            (
                type_mismatch(),
                "This value was not accepted. Check it and try again.",
            ),
        ] {
            assert_eq!(text_of(error), expected);
        }
    }

    #[test]
    fn each_storage_error_reads_as_its_own_message_in_ukrainian() {
        install_language(Language::Uk);
        for (error, expected) in [
            (
                not_found(),
                "Цього елемента більше немає. Оновіть екран і спробуйте ще раз.",
            ),
            (
                name_collision(),
                "Назва \"Шанувальник\" вже використовується.",
            ),
            (
                validation_failed(),
                "Це значення не прийнято. Перевірте його і спробуйте ще раз.",
            ),
            (
                type_mismatch(),
                "Це значення не прийнято. Перевірте його і спробуйте ще раз.",
            ),
        ] {
            assert_eq!(text_of(error), expected);
        }
    }

    #[test]
    fn a_blocked_delete_counts_its_actions_with_the_plural_of_each_language() {
        for (language, count, expected) in [
            (Language::En, 1, "It is still used by 1 action."),
            (Language::En, 0, "It is still used by 0 actions."),
            (Language::En, 2, "It is still used by 2 actions."),
            (Language::Uk, 1, "Його все ще використовує 1 дія."),
            (Language::Uk, 3, "Його все ще використовують 3 дії."),
            (Language::Uk, 5, "Його все ще використовують 5 дій."),
            (Language::Uk, 21, "Його все ще використовує 21 дія."),
        ] {
            install_language(language);
            assert_eq!(text_of(reference_block(count)), expected, "{count}");
        }
    }

    #[test]
    fn a_storage_error_with_no_message_of_its_own_reads_as_a_plain_storage_failure() {
        for (language, expected) in [
            (Language::En, STORAGE_FAILED_EN),
            (Language::Uk, STORAGE_FAILED_UK),
        ] {
            install_language(language);
            for error in opaque_errors() {
                let diagnostic = error.to_string();
                assert_eq!(text_of(error), expected, "{diagnostic}");
            }
        }
    }

    #[test]
    fn the_log_line_keeps_the_technical_error_the_user_never_sees() {
        install_language(Language::En);
        let error = StorageError::Connection {
            reason: "database is locked".to_owned(),
        };
        let diagnostic = error.to_string();

        let failure = RepoFailure::from(error);

        assert_eq!(
            (failure.to_string(), failure.text().contains("locked")),
            (diagnostic, false)
        );
    }

    #[test]
    fn a_named_failure_reads_as_its_message_and_logs_its_diagnostic() {
        install_language(Language::Uk);
        for (failure, expected, diagnostic) in [
            (
                RepoFailure::trigger_gone(),
                "Цього тригера більше немає. Можливо, його видалено.",
                "trigger instance not found",
            ),
            (
                RepoFailure::action_gone(),
                "Цієї дії більше немає.",
                "action not found",
            ),
            (
                RepoFailure::steps_gone(),
                "Списку кроків цієї дії більше немає.",
                "step list not found",
            ),
            (
                RepoFailure::operation("disk full"),
                "Операцію не виконано. Подробиці в журналі.",
                "disk full",
            ),
        ] {
            assert_eq!(
                (failure.text().as_str(), failure.to_string().as_str()),
                (expected, diagnostic)
            );
        }
    }
}

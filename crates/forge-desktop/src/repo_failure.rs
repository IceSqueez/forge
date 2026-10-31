use std::fmt;
use std::path::Path;

use forge_audio::AudioError;
use forge_discord::DiscordError;
use forge_hotkey::HotkeyError;
use forge_hotkey::vocabulary::messages::failure;
use forge_overlay::OverlayError;
use forge_runtime::{
    DispatchError, IntegrationStartError, LatestResetError, OverlayServiceError, ScheduleError,
    SchedulerError, ScriptRegistryError,
};
use forge_script::ScriptError;
use forge_server::ServerError;
use forge_speak_queue::vocabulary::messages::{audio_failure, queue_failure, tts_failure};
use forge_speak_queue::{FilterMappingError, SpeakError, TtsError};
use forge_storage::StorageError;
use forge_transfer::TransferError;
use forge_types::{
    MessageCatalog, MessageCode, MessageRef, UpstreamDetail, UserText, VerbatimText,
};
use forge_update::UpdateError;

use crate::error::DesktopError;
use crate::restore_problem::RestoreProblem;
use crate::user_text::resolve_user_text;

const NAMESPACE: &str = "repo";

const NAME_ARG: &str = "name";
const COUNT_ARG: &str = "count";
const PATH_ARG: &str = "path";
const ADDRESS_ARG: &str = "address";
const KIND_ARG: &str = "kind";
const CAPACITY_ARG: &str = "capacity";

const STORAGE_FAILED: MessageCode = MessageCode::from_static(NAMESPACE, "storage_failed");
const ITEM_GONE: MessageCode = MessageCode::from_static(NAMESPACE, "item_gone");
const NAME_TAKEN: MessageCode = MessageCode::from_static(NAMESPACE, "name_taken");
const VALUE_REJECTED: MessageCode = MessageCode::from_static(NAMESPACE, "value_rejected");
const STILL_REFERENCED: MessageCode = MessageCode::from_static(NAMESPACE, "still_referenced");
const TRIGGER_GONE: MessageCode = MessageCode::from_static(NAMESPACE, "trigger_gone");
const ACTION_GONE: MessageCode = MessageCode::from_static(NAMESPACE, "action_gone");
const STEPS_GONE: MessageCode = MessageCode::from_static(NAMESPACE, "steps_gone");
const OPERATION_FAILED: MessageCode = MessageCode::from_static(NAMESPACE, "operation_failed");
const DATA_DIR_UNAVAILABLE: MessageCode =
    MessageCode::from_static(NAMESPACE, "data_dir_unavailable");
const STARTUP_STOPPED: MessageCode = MessageCode::from_static(NAMESPACE, "startup_stopped");
const FILE_FAILED: MessageCode = MessageCode::from_static(NAMESPACE, "file_failed");
const SERVER_ADDRESS_BUSY: MessageCode = MessageCode::from_static(NAMESPACE, "server_address_busy");
const SERVER_LAN_DISABLED: MessageCode = MessageCode::from_static(NAMESPACE, "server_lan_disabled");
const SERVER_SWITCHED_OFF: MessageCode = MessageCode::from_static(NAMESPACE, "server_switched_off");
const SCRIPT_REJECTED: MessageCode = MessageCode::from_static(NAMESPACE, "script_rejected");
const OVERLAY_KIND_UNAVAILABLE: MessageCode =
    MessageCode::from_static(NAMESPACE, "overlay_kind_unavailable");
const OVERLAY_QUEUE_FULL: MessageCode = MessageCode::from_static(NAMESPACE, "overlay_queue_full");
const OVERLAY_SLOT_BOUND: MessageCode = MessageCode::from_static(NAMESPACE, "overlay_slot_bound");

pub(crate) const REPO_MESSAGES: MessageCatalog = MessageCatalog {
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
        DATA_DIR_UNAVAILABLE,
        STARTUP_STOPPED,
        FILE_FAILED,
        SERVER_ADDRESS_BUSY,
        SERVER_LAN_DISABLED,
        SERVER_SWITCHED_OFF,
        SCRIPT_REJECTED,
        OVERLAY_KIND_UNAVAILABLE,
        OVERLAY_QUEUE_FULL,
        OVERLAY_SLOT_BOUND,
    ],
    en_ftl: include_str!("../locales/en/repo_messages.ftl"),
    uk_ftl: include_str!("../locales/uk/repo_messages.ftl"),
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RepoFailure {
    message: Box<UserText>,
    diagnostic: String,
    shows_detail: bool,
    problem: Option<RestoreProblem>,
}

impl RepoFailure {
    pub(crate) fn trigger_gone() -> Self {
        Self {
            message: Box::new(MessageRef::new(TRIGGER_GONE).into()),
            diagnostic: "trigger instance not found".to_owned(),
            shows_detail: false,
            problem: None,
        }
    }

    pub(crate) fn action_gone() -> Self {
        Self {
            message: Box::new(MessageRef::new(ACTION_GONE).into()),
            diagnostic: "action not found".to_owned(),
            shows_detail: false,
            problem: None,
        }
    }

    pub(crate) fn steps_gone() -> Self {
        Self {
            message: Box::new(MessageRef::new(STEPS_GONE).into()),
            diagnostic: "step list not found".to_owned(),
            shows_detail: false,
            problem: None,
        }
    }

    pub(crate) fn operation(diagnostic: impl ToString) -> Self {
        Self {
            message: Box::new(MessageRef::new(OPERATION_FAILED).into()),
            diagnostic: diagnostic.to_string(),
            shows_detail: false,
            problem: None,
        }
    }

    pub(crate) fn data_dir_unavailable(path: &Path, cause: &std::io::Error) -> Self {
        let shown = path.display().to_string();
        Self {
            message: Box::new(
                MessageRef::new(DATA_DIR_UNAVAILABLE)
                    .with_arg(PATH_ARG, VerbatimText::new(shown.as_str()))
                    .into(),
            ),
            diagnostic: format!("could not create data directory {shown}: {cause}"),
            shows_detail: false,
            problem: None,
        }
    }

    pub(crate) fn startup_stopped(diagnostic: impl ToString) -> Self {
        Self {
            message: Box::new(MessageRef::new(STARTUP_STOPPED).into()),
            diagnostic: diagnostic.to_string(),
            shows_detail: false,
            problem: None,
        }
    }

    pub(crate) fn hotkey(error: &HotkeyError) -> Self {
        Self {
            message: Box::new(failure(error).into()),
            diagnostic: error.to_string(),
            shows_detail: false,
            problem: None,
        }
    }

    pub(crate) fn localized(text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            message: Box::new(VerbatimText::new(text.as_str()).into()),
            diagnostic: text,
            shows_detail: false,
            problem: None,
        }
    }

    pub(crate) fn shown(message: impl Into<UserText>, diagnostic: impl ToString) -> Self {
        Self {
            message: Box::new(message.into()),
            diagnostic: diagnostic.to_string(),
            shows_detail: true,
            problem: None,
        }
    }

    pub(crate) fn text(&self) -> String {
        if let Some(problem) = &self.problem {
            return problem.message();
        }
        let resolved = resolve_user_text(&self.message);
        if self.shows_detail {
            resolved.inline()
        } else {
            resolved.primary
        }
    }
}

impl fmt::Display for RepoFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.diagnostic)
    }
}

impl From<std::io::Error> for RepoFailure {
    fn from(error: std::io::Error) -> Self {
        Self::shown(MessageRef::new(FILE_FAILED), error)
    }
}

impl From<tokio::task::JoinError> for RepoFailure {
    fn from(error: tokio::task::JoinError) -> Self {
        Self::operation(error)
    }
}

impl From<DispatchError> for RepoFailure {
    fn from(error: DispatchError) -> Self {
        Self::operation(error)
    }
}

impl From<UpdateError> for RepoFailure {
    fn from(error: UpdateError) -> Self {
        Self::operation(error)
    }
}

impl From<forge_awake::AwakeError> for RepoFailure {
    fn from(error: forge_awake::AwakeError) -> Self {
        Self::operation(error)
    }
}

impl From<DesktopError> for RepoFailure {
    fn from(error: DesktopError) -> Self {
        match error {
            DesktopError::Storage(storage) => storage.into(),
            DesktopError::Io(io) => io.into(),
            other => Self::shown(VerbatimText::new(other.user_message()), other),
        }
    }
}

impl From<HotkeyError> for RepoFailure {
    fn from(error: HotkeyError) -> Self {
        Self::hotkey(&error)
    }
}

impl From<TransferError> for RepoFailure {
    fn from(error: TransferError) -> Self {
        let problem = RestoreProblem::classified(&error);
        Self {
            problem,
            ..Self::shown(error.user_message(), &error)
        }
    }
}

impl From<DiscordError> for RepoFailure {
    fn from(error: DiscordError) -> Self {
        Self::shown(error.user_message(), error)
    }
}

impl From<AudioError> for RepoFailure {
    fn from(error: AudioError) -> Self {
        Self::shown(audio_failure(&error), error)
    }
}

impl From<TtsError> for RepoFailure {
    fn from(error: TtsError) -> Self {
        Self::shown(tts_failure(&error), error)
    }
}

impl From<SpeakError> for RepoFailure {
    fn from(error: SpeakError) -> Self {
        Self::shown(queue_failure(&error), error)
    }
}

impl From<IntegrationStartError> for RepoFailure {
    fn from(error: IntegrationStartError) -> Self {
        Self::shown(error.message().clone(), &error)
    }
}

impl From<ServerError> for RepoFailure {
    fn from(error: ServerError) -> Self {
        let message = match &error {
            ServerError::Storage(storage) => return Self::from_storage_ref(storage),
            ServerError::Bind { addr, .. } => MessageRef::new(SERVER_ADDRESS_BUSY)
                .with_arg(ADDRESS_ARG, VerbatimText::new(addr.as_str())),
            ServerError::LanBindNotEnabled { .. } => MessageRef::new(SERVER_LAN_DISABLED),
            ServerError::Disabled => MessageRef::new(SERVER_SWITCHED_OFF),
            _ => MessageRef::new(OPERATION_FAILED),
        };
        Self::shown(message, error)
    }
}

impl From<FilterMappingError> for RepoFailure {
    fn from(error: FilterMappingError) -> Self {
        Self::shown(
            MessageRef::new(VALUE_REJECTED)
                .with_detail(UpstreamDetail::from_redacted(error.to_string())),
            error,
        )
    }
}

impl From<ScriptError> for RepoFailure {
    fn from(error: ScriptError) -> Self {
        Self::shown(
            MessageRef::new(SCRIPT_REJECTED)
                .with_detail(UpstreamDetail::from_redacted(error.to_string())),
            error,
        )
    }
}

impl From<ScriptRegistryError> for RepoFailure {
    fn from(error: ScriptRegistryError) -> Self {
        match error {
            ScriptRegistryError::Storage(storage) => storage.into(),
            ScriptRegistryError::Compile(detail) => Self::shown(
                MessageRef::new(SCRIPT_REJECTED)
                    .with_detail(UpstreamDetail::from_redacted(detail.as_str())),
                detail,
            ),
        }
    }
}

impl From<SchedulerError> for RepoFailure {
    fn from(error: SchedulerError) -> Self {
        match error {
            SchedulerError::QueueNotFound(_) => Self::shown(MessageRef::new(ITEM_GONE), error),
            other => Self::operation(other),
        }
    }
}

impl From<ScheduleError> for RepoFailure {
    fn from(error: ScheduleError) -> Self {
        match error {
            ScheduleError::UnknownAction => Self::action_gone(),
            other => Self::shown(MessageRef::new(VALUE_REJECTED), other),
        }
    }
}

impl From<LatestResetError> for RepoFailure {
    fn from(error: LatestResetError) -> Self {
        match error {
            LatestResetError::Storage(storage) => storage.into(),
            other => Self::shown(MessageRef::new(ITEM_GONE), other),
        }
    }
}

impl From<OverlayServiceError> for RepoFailure {
    fn from(error: OverlayServiceError) -> Self {
        let message = match &error {
            OverlayServiceError::Storage(storage) => return Self::from_storage_ref(storage),
            OverlayServiceError::Unknown(_) => MessageRef::new(ITEM_GONE),
            OverlayServiceError::UnavailableKind { kind_id, .. } => {
                MessageRef::new(OVERLAY_KIND_UNAVAILABLE)
                    .with_arg(KIND_ARG, VerbatimText::new(kind_id.as_str()))
            }
            OverlayServiceError::Overlay(overlay) => return Self::from(overlay),
            OverlayServiceError::Interrupted => MessageRef::new(OPERATION_FAILED),
            OverlayServiceError::ShowQueueFull { capacity, .. } => {
                MessageRef::new(OVERLAY_QUEUE_FULL)
                    .with_arg(CAPACITY_ARG, i64::try_from(*capacity).unwrap_or(i64::MAX))
            }
            OverlayServiceError::SlotBound(_) => MessageRef::new(OVERLAY_SLOT_BOUND),
        };
        Self::shown(message, error)
    }
}

impl From<&OverlayError> for RepoFailure {
    fn from(error: &OverlayError) -> Self {
        Self::shown(MessageRef::new(VALUE_REJECTED), error)
    }
}

impl From<StorageError> for RepoFailure {
    fn from(error: StorageError) -> Self {
        Self::from_storage_ref(&error)
    }
}

impl RepoFailure {
    pub(crate) fn from_storage_ref(error: &StorageError) -> Self {
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
            message: Box::new(message.into()),
            diagnostic: error.to_string(),
            shows_detail: false,
            problem: None,
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
            .replace(crate::test_support::FORMAT_ISOLATES, "")
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

    fn plain(failure: &RepoFailure) -> String {
        failure
            .text()
            .replace(crate::test_support::FORMAT_ISOLATES, "")
    }

    fn converted<E>(error: E, expected: &str) -> (String, String, String)
    where
        E: Into<RepoFailure> + fmt::Display,
    {
        let full_cause = error.to_string();
        let failure = error.into();
        assert!(
            full_cause.contains(&failure.to_string()),
            "{full_cause} lost its cause: {failure}"
        );
        (plain(&failure), expected.to_owned(), full_cause)
    }

    fn connection_lost() -> StorageError {
        StorageError::Connection {
            reason: "database is locked".to_owned(),
        }
    }

    fn overlay_id() -> forge_storage::OverlayId {
        forge_storage::OverlayId::new("overlay-1")
    }

    fn regex_error(pattern: &str) -> regex::Error {
        regex::Regex::new(pattern).unwrap_err()
    }

    #[test]
    fn each_conversion_shows_its_fixed_message_and_keeps_the_cause_for_the_log() {
        install_language(Language::En);
        const OPERATION: &str = "The operation failed. Details are in the log.";
        const FILE: &str = "The file could not be read or written. Details are in the log.";
        const GONE: &str = "This item no longer exists. Reload the screen and try again.";
        const REJECTED: &str = "This value was not accepted. Check it and try again.";
        let disk = || std::io::Error::other("disk gone");
        let cases = [
            converted(disk(), FILE),
            converted(forge_runtime::DispatchError::ChannelClosed, OPERATION),
            converted(UpdateError::RateLimited, OPERATION),
            converted(forge_awake::AwakeError::Stopped, OPERATION),
            converted(DesktopError::Storage(connection_lost()), STORAGE_FAILED_EN),
            converted(DesktopError::Io(disk()), FILE),
            converted(DesktopError::SignInCancelled, "The sign-in was cancelled."),
            converted(
                TransferError::NotABundle,
                "This file is not a forge backup.",
            ),
            converted(
                AudioError::NoDefaultDevice,
                "No audio output device is available.",
            ),
            converted(SpeakError::ActorGone, "The speech queue has stopped."),
            converted(ServerError::Storage(connection_lost()), STORAGE_FAILED_EN),
            converted(
                ServerError::Bind {
                    addr: "127.0.0.1:7474".to_owned(),
                    reason: "os error 98".to_owned(),
                },
                "The server could not listen on 127.0.0.1:7474. Another program may be using that port.",
            ),
            converted(
                ServerError::LanBindNotEnabled {
                    addr: "0.0.0.0:7474".to_owned(),
                },
                "This address needs LAN access, which is turned off in the server settings.",
            ),
            converted(
                ServerError::Disabled,
                "The server is switched off in settings.",
            ),
            converted(ServerError::Io(disk()), OPERATION),
            converted(
                ScriptRegistryError::Storage(connection_lost()),
                STORAGE_FAILED_EN,
            ),
            converted(
                SchedulerError::QueueNotFound(forge_types::QueueId::new()),
                GONE,
            ),
            converted(SchedulerError::ChannelClosed, OPERATION),
            converted(ScheduleError::DelayTooShort, REJECTED),
            converted(
                LatestResetError::Storage(connection_lost()),
                STORAGE_FAILED_EN,
            ),
            converted(LatestResetError::UnknownSlot("follower".to_owned()), GONE),
            converted(
                OverlayServiceError::Storage(connection_lost()),
                STORAGE_FAILED_EN,
            ),
            converted(OverlayServiceError::Unknown(overlay_id()), GONE),
            converted(
                OverlayServiceError::UnavailableKind {
                    id: overlay_id(),
                    kind_id: "alert".to_owned(),
                },
                "This overlay needs the type alert, which this version of forge does not have.",
            ),
            converted(
                OverlayServiceError::Overlay(OverlayError::UnknownKind("alert".to_owned())),
                REJECTED,
            ),
            converted(OverlayServiceError::Interrupted, OPERATION),
            converted(
                OverlayServiceError::ShowQueueFull {
                    id: overlay_id(),
                    capacity: 8,
                },
                "This overlay already has 8 shows waiting, so this one was not queued.",
            ),
            converted(
                OverlayServiceError::SlotBound(overlay_id()),
                "This overlay shows a latest value and takes no content from actions.",
            ),
        ];

        for (shown, expected, cause) in cases {
            assert_eq!(shown, expected, "{cause}");
        }
    }

    #[test]
    fn a_schedule_for_a_vanished_action_names_the_action() {
        install_language(Language::En);

        let failure = RepoFailure::from(ScheduleError::UnknownAction);

        assert_eq!(failure, RepoFailure::action_gone());
    }

    #[test]
    fn a_borrowed_overlay_error_reads_as_a_rejected_value() {
        install_language(Language::En);
        let error = OverlayError::OutOfRange {
            key: "size".to_owned(),
            min: 1,
            max: 9,
        };

        let failure = RepoFailure::from(&error);

        assert_eq!(
            (failure.text(), failure.to_string()),
            (
                "This value was not accepted. Check it and try again.".to_owned(),
                error.to_string()
            )
        );
    }

    #[test]
    fn a_stopped_task_reads_as_a_failed_operation() {
        install_language(Language::En);
        let rt = crate::test_support::runtime();
        let task = rt.spawn(std::future::pending::<()>());
        task.abort();
        let error = rt.block_on(task).unwrap_err();
        let diagnostic = error.to_string();

        let failure = RepoFailure::from(error);

        assert_eq!(
            (failure.text(), failure.to_string()),
            (
                "The operation failed. Details are in the log.".to_owned(),
                diagnostic
            )
        );
    }

    #[test]
    fn a_failure_the_user_can_act_on_shows_its_detail_beside_the_message() {
        install_language(Language::En);
        let cases: [(RepoFailure, &str); 7] = [
            (
                ScriptError::Compile {
                    reason: "unexpected token".to_owned(),
                }
                .into(),
                "unexpected token",
            ),
            (
                ScriptRegistryError::Compile("missing brace".to_owned()).into(),
                "missing brace",
            ),
            (
                FilterMappingError::InvalidSkipRegex {
                    index: 3,
                    pattern: "broken(".to_owned(),
                    source: regex_error("broken("),
                }
                .into(),
                "broken(",
            ),
            (
                TtsError::AuthFailed {
                    reason: "key expired".to_owned(),
                }
                .into(),
                "key expired",
            ),
            (AudioError::Host("alsa busy".to_owned()).into(), "alsa busy"),
            (
                DiscordError::WebhookNotFound {
                    name: "alerts".to_owned(),
                }
                .into(),
                "alerts",
            ),
            (
                RepoFailure::shown(
                    MessageRef::new(OPERATION_FAILED)
                        .with_detail(UpstreamDetail::from_redacted("upstream said no")),
                    "logged",
                ),
                "upstream said no",
            ),
        ];

        for (failure, detail) in cases {
            assert!(failure.text().contains(detail), "{}", failure.text());
        }
    }

    #[test]
    fn a_failure_that_hides_its_detail_leaves_it_to_the_log() {
        install_language(Language::En);
        let error = HotkeyError::Backend("portal socket refused".to_owned());

        let failure = RepoFailure::hotkey(&error);

        assert_eq!(
            (
                failure.text().contains("portal socket refused"),
                failure.to_string()
            ),
            (false, error.to_string())
        );
    }

    #[test]
    fn a_shown_failure_without_a_detail_reads_as_the_bare_message() {
        install_language(Language::En);

        let failure = RepoFailure::shown(MessageRef::new(OPERATION_FAILED), "logged only");

        assert_eq!(
            failure.text(),
            "The operation failed. Details are in the log."
        );
    }

    #[test]
    fn already_localized_text_is_shown_and_logged_verbatim() {
        install_language(Language::Uk);

        let failure = RepoFailure::localized("Дію скасовано");

        assert_eq!(
            (failure.text(), failure.to_string()),
            ("Дію скасовано".to_owned(), "Дію скасовано".to_owned())
        );
    }

    #[test]
    fn a_data_folder_failure_names_the_folder_and_logs_the_cause() {
        install_language(Language::En);
        let path = std::path::Path::new("/data/forge");

        let failure = RepoFailure::data_dir_unavailable(path, &std::io::Error::other("read-only"));

        assert_eq!(
            (plain(&failure), failure.to_string()),
            (
                "forge could not create its data folder /data/forge.".to_owned(),
                "could not create data directory /data/forge: read-only".to_owned()
            )
        );
    }

    #[test]
    fn a_failed_overlay_token_rotation_keeps_the_token_out_of_the_text_and_the_log() {
        install_language(Language::En);
        const TOKEN: &str = "fresh-overlay-token";
        for error in [
            ServerError::OverlayRotationIncomplete {
                token: forge_types::Secret::new(TOKEN.to_owned()),
                reason: "pages locked".to_owned(),
            },
            ServerError::OverlayTokenRestoreFailed {
                token: forge_types::Secret::new(TOKEN.to_owned()),
                reason: "pages locked".to_owned(),
            },
        ] {
            let failure = RepoFailure::from(error);

            assert!(
                !failure.text().contains(TOKEN) && !failure.to_string().contains(TOKEN),
                "{failure}"
            );
        }
    }
}

use forge_hotkey::HotkeyError;
use forge_hotkey::vocabulary::messages as hotkey_messages;
use forge_midi::MidiError;
use forge_platform_core::PlatformError;
use forge_runtime::{IntegrationStartError, StartFailureCategory};
use forge_storage::StorageError;
use forge_types::{MessageCode, MessageRef, UpstreamDetail};

pub(crate) fn youtube_start_error(error: &PlatformError) -> IntegrationStartError {
    stored_sign_in_unreadable(
        forge_platform_youtube::vocabulary::messages::STORED_SIGN_IN_UNREADABLE,
        error,
    )
}

pub(crate) fn twitch_start_error(error: &StorageError) -> IntegrationStartError {
    stored_sign_in_unreadable(
        forge_platform_twitch::vocabulary::messages::STORED_SIGN_IN_UNREADABLE,
        error,
    )
}

pub(crate) fn kick_start_error(error: &PlatformError) -> IntegrationStartError {
    stored_sign_in_unreadable(
        forge_platform_kick::vocabulary::messages::STORED_SIGN_IN_UNREADABLE,
        error,
    )
}

fn stored_sign_in_unreadable(
    code: MessageCode,
    error: &dyn std::error::Error,
) -> IntegrationStartError {
    tracing::warn!(code = %code, error = %error, "stored sign-in could not be loaded");
    IntegrationStartError::new(StartFailureCategory::Credentials, MessageRef::new(code))
}

pub(crate) fn midi_start_error(error: &MidiError) -> IntegrationStartError {
    tracing::warn!(error = %error, "midi engine could not start");
    let message = MessageRef::new(forge_midi::vocabulary::messages::BACKEND_UNAVAILABLE);
    let message = match error {
        MidiError::MidirInit(cause) => message.with_detail(UpstreamDetail::from_redacted(cause)),
        _ => message,
    };
    IntegrationStartError::new(StartFailureCategory::Unreachable, message)
}

pub(crate) fn hotkey_start_error(error: &HotkeyError) -> IntegrationStartError {
    tracing::warn!(error = %error, "hotkey engine could not start");
    let category = match error {
        HotkeyError::PortalUnavailable { .. } | HotkeyError::BackendUnavailable => {
            StartFailureCategory::Unreachable
        }
        HotkeyError::PermissionDenied
        | HotkeyError::AlreadyRegistered { .. }
        | HotkeyError::InvalidCombo(_)
        | HotkeyError::BindRejected { .. }
        | HotkeyError::EdgeTaken { .. }
        | HotkeyError::BindingNotFound => StartFailureCategory::Refused,
        HotkeyError::SupervisorUnavailable
        | HotkeyError::MainThreadUnavailable
        | HotkeyError::Storage(_)
        | HotkeyError::Backend(_) => StartFailureCategory::Internal,
    };
    IntegrationStartError::new(category, hotkey_messages::failure(error))
}

#[cfg(test)]
mod tests {
    use forge_types::MessageArg;

    use super::*;

    const UPSTREAM: &str = "org.freedesktop.portal.Error.NotAllowed";

    type Mapped<'a> = (
        StartFailureCategory,
        &'a MessageCode,
        Option<&'a MessageArg>,
        Option<&'a str>,
    );

    fn mapped(error: &IntegrationStartError) -> Mapped<'_> {
        let message = error.message();
        (
            error.category(),
            message.code(),
            message.arg(hotkey_messages::COMBO_ARG),
            message.detail().map(UpstreamDetail::as_str),
        )
    }

    fn combo(text: &str) -> MessageArg {
        MessageArg::Verbatim(forge_types::VerbatimText::new(text))
    }

    #[test]
    fn an_unreadable_stored_sign_in_asks_for_credentials_under_the_platform_code_without_its_text()
    {
        let secret = PlatformError::Network {
            reason: "refresh failed for access_token=s3cr3t".to_owned(),
        };
        for (error, code) in [
            (
                youtube_start_error(&secret),
                forge_platform_youtube::vocabulary::messages::STORED_SIGN_IN_UNREADABLE,
            ),
            (
                kick_start_error(&secret),
                forge_platform_kick::vocabulary::messages::STORED_SIGN_IN_UNREADABLE,
            ),
        ] {
            assert_eq!(
                (error.category(), error.message(),),
                (
                    StartFailureCategory::Credentials,
                    &MessageRef::new(code.clone())
                ),
                "{code}"
            );
        }
    }

    #[test]
    fn a_midi_start_failure_is_unreachable_and_carries_only_the_midir_init_text() {
        let init = midi_start_error(&MidiError::MidirInit("ALSA sequencer missing".to_owned()));
        let other = midi_start_error(&MidiError::MidirConnect("port busy".to_owned()));

        assert_eq!(
            (mapped(&init), mapped(&other)),
            (
                (
                    StartFailureCategory::Unreachable,
                    &forge_midi::vocabulary::messages::BACKEND_UNAVAILABLE,
                    None,
                    Some("ALSA sequencer missing"),
                ),
                (
                    StartFailureCategory::Unreachable,
                    &forge_midi::vocabulary::messages::BACKEND_UNAVAILABLE,
                    None,
                    None,
                ),
            )
        );
    }

    #[test]
    fn each_hotkey_error_maps_to_its_category_code_combo_and_upstream_detail() {
        let cases = [
            (
                HotkeyError::PortalUnavailable {
                    reason: UPSTREAM.to_owned(),
                },
                (
                    StartFailureCategory::Unreachable,
                    hotkey_messages::PORTAL_UNAVAILABLE,
                    None,
                    Some(UPSTREAM),
                ),
            ),
            (
                HotkeyError::PermissionDenied,
                (
                    StartFailureCategory::Refused,
                    hotkey_messages::PERMISSION_DENIED,
                    None,
                    None,
                ),
            ),
            (
                HotkeyError::AlreadyRegistered {
                    combo: "Ctrl+F1".to_owned(),
                },
                (
                    StartFailureCategory::Refused,
                    hotkey_messages::COMBO_REJECTED,
                    Some(combo("Ctrl+F1")),
                    None,
                ),
            ),
            (
                HotkeyError::InvalidCombo("Ctrl++".to_owned()),
                (
                    StartFailureCategory::Refused,
                    hotkey_messages::COMBO_REJECTED,
                    Some(combo("Ctrl++")),
                    None,
                ),
            ),
            (
                HotkeyError::BindRejected {
                    combo: "Super+K".to_owned(),
                    reason: UPSTREAM.to_owned(),
                },
                (
                    StartFailureCategory::Refused,
                    hotkey_messages::COMBO_REJECTED,
                    Some(combo("Super+K")),
                    Some(UPSTREAM),
                ),
            ),
            (
                HotkeyError::SupervisorUnavailable,
                (
                    StartFailureCategory::Internal,
                    hotkey_messages::ENGINE_STOPPED,
                    None,
                    None,
                ),
            ),
            (
                HotkeyError::MainThreadUnavailable,
                (
                    StartFailureCategory::Internal,
                    hotkey_messages::ENGINE_STOPPED,
                    None,
                    None,
                ),
            ),
            (
                HotkeyError::Backend("evdev read failed".to_owned()),
                (
                    StartFailureCategory::Internal,
                    hotkey_messages::BACKEND_FAILED,
                    None,
                    Some("evdev read failed"),
                ),
            ),
        ];
        for (error, (category, code, combo, detail)) in cases {
            let started = hotkey_start_error(&error);
            assert_eq!(
                mapped(&started),
                (category, &code, combo.as_ref(), detail),
                "{error:?}"
            );
        }
    }

    #[test]
    fn the_new_hotkey_failures_keep_their_start_category() {
        for (error, category) in [
            (
                HotkeyError::BackendUnavailable,
                StartFailureCategory::Unreachable,
            ),
            (HotkeyError::BindingNotFound, StartFailureCategory::Refused),
            (
                HotkeyError::EdgeTaken {
                    combo: "Ctrl+E".to_owned(),
                },
                StartFailureCategory::Refused,
            ),
            (
                HotkeyError::Storage(forge_storage::StorageError::Connection {
                    reason: "disk full".to_owned(),
                }),
                StartFailureCategory::Internal,
            ),
        ] {
            assert_eq!(hotkey_start_error(&error).category(), category, "{error:?}");
        }
    }
}

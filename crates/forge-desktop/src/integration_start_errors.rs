use forge_hotkey::HotkeyError;
use forge_hotkey::vocabulary::messages as hotkey_messages;
use forge_midi::MidiError;
use forge_platform_core::PlatformError;
use forge_runtime::{IntegrationStartError, StartFailureCategory};
use forge_storage::StorageError;
use forge_types::{MessageCode, MessageRef, UpstreamDetail, VerbatimText};

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
    let (category, message) = match error {
        HotkeyError::PortalUnavailable { reason } => (
            StartFailureCategory::Unreachable,
            MessageRef::new(hotkey_messages::PORTAL_UNAVAILABLE)
                .with_detail(UpstreamDetail::from_redacted(reason)),
        ),
        HotkeyError::PermissionDenied => (
            StartFailureCategory::Refused,
            MessageRef::new(hotkey_messages::PERMISSION_DENIED),
        ),
        HotkeyError::AlreadyRegistered { combo } | HotkeyError::InvalidCombo(combo) => {
            (StartFailureCategory::Refused, combo_rejected(combo))
        }
        HotkeyError::BindRejected { combo, reason } => (
            StartFailureCategory::Refused,
            combo_rejected(combo).with_detail(UpstreamDetail::from_redacted(reason)),
        ),
        HotkeyError::SupervisorUnavailable | HotkeyError::MainThreadUnavailable => (
            StartFailureCategory::Internal,
            MessageRef::new(hotkey_messages::ENGINE_STOPPED),
        ),
        HotkeyError::Backend(cause) => (
            StartFailureCategory::Internal,
            MessageRef::new(hotkey_messages::BACKEND_FAILED)
                .with_detail(UpstreamDetail::from_redacted(cause)),
        ),
    };
    IntegrationStartError::new(category, message)
}

fn combo_rejected(combo: &str) -> MessageRef {
    MessageRef::new(hotkey_messages::COMBO_REJECTED)
        .with_arg(hotkey_messages::COMBO_ARG, VerbatimText::new(combo))
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
        MessageArg::Verbatim(VerbatimText::new(text))
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
}

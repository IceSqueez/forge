use forge_hotkey::HotkeyError;
use forge_hotkey::vocabulary::messages as hotkey_messages;
use forge_midi::MidiError;
use forge_platform_core::PlatformError;
use forge_runtime::{IntegrationStartError, StartFailureCategory};
use forge_types::{MessageCode, MessageRef, UpstreamDetail, VerbatimText};

pub(crate) fn youtube_start_error(error: &PlatformError) -> IntegrationStartError {
    stored_sign_in_unreadable(
        forge_platform_youtube::vocabulary::messages::STORED_SIGN_IN_UNREADABLE,
        error,
    )
}

pub(crate) fn kick_start_error(error: &PlatformError) -> IntegrationStartError {
    stored_sign_in_unreadable(
        forge_platform_kick::vocabulary::messages::STORED_SIGN_IN_UNREADABLE,
        error,
    )
}

fn stored_sign_in_unreadable(code: MessageCode, error: &PlatformError) -> IntegrationStartError {
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

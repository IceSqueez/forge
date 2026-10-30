use forge_types::{MessageRef, SubActionOutcome};

use crate::bot_account::BotUnavailable;
use crate::chat::{ChatSendError, MAX_MESSAGE_LEN, MAX_WHISPER_CHARS, WhisperError};
use crate::chat_sender::SendFailure;
use crate::helix::HelixError;
use crate::vocabulary::messages;

pub(crate) fn helix_message(error: &HelixError) -> MessageRef {
    match error {
        HelixError::RateLimited => messages::rate_limited(),
        HelixError::ReauthRequired => messages::reauth_required(),
        HelixError::Credentials(detail) => messages::credentials_unavailable(detail),
        HelixError::Http { status, body } => messages::request_refused(*status, body),
        HelixError::Transport(detail) => messages::request_failed(detail),
        HelixError::UndecodableResponse(detail) => messages::response_unreadable(detail),
    }
}

pub(crate) fn failed(error: &HelixError) -> SubActionOutcome {
    SubActionOutcome::Failed(helix_message(error).into())
}

pub(crate) fn outcome<T>(result: &Result<T, HelixError>) -> SubActionOutcome {
    match result {
        Ok(_) => SubActionOutcome::Success,
        Err(error) => failed(error),
    }
}

fn bot_unavailable_message(reason: BotUnavailable) -> MessageRef {
    match reason {
        BotUnavailable::NotConnected => messages::bot_not_signed_in(),
        BotUnavailable::ReauthRequired => messages::bot_reauth_required(),
        BotUnavailable::StorageFailure => messages::bot_credentials_unreadable(),
    }
}

fn chat_send_message(error: &ChatSendError) -> MessageRef {
    match error {
        ChatSendError::RateLimited => messages::rate_limited(),
        ChatSendError::MessageTooLong => messages::message_too_long(MAX_MESSAGE_LEN),
        ChatSendError::NotConnected => messages::chat_not_connected(),
        ChatSendError::Http(detail) => messages::request_failed(detail),
        ChatSendError::ReauthRequired => messages::reauth_required(),
        ChatSendError::Dropped { code, message } => messages::message_dropped(code, message),
        ChatSendError::Forbidden(detail) => messages::message_forbidden(detail),
    }
}

fn whisper_message(error: &WhisperError) -> MessageRef {
    match error {
        WhisperError::EmptyRecipient => messages::recipient_empty(),
        WhisperError::EmptyMessage => messages::message_empty(),
        WhisperError::MessageTooLong => messages::message_too_long(MAX_WHISPER_CHARS),
        WhisperError::RecipientRejectsWhispers => messages::recipient_rejects_whispers(),
        WhisperError::Helix(error) => helix_message(error),
    }
}

pub(crate) fn send_failure(failure: &SendFailure) -> MessageRef {
    match failure {
        SendFailure::BotNotConnected(reason) => bot_unavailable_message(*reason),
        SendFailure::BotPhoneUnverified => messages::bot_phone_unverified(),
        SendFailure::Chat(error) => chat_send_message(error),
        SendFailure::Whisper(error) => whisper_message(error),
        SendFailure::BotNotModerator => messages::bot_not_moderator(),
        SendFailure::PermissionDenied(detail) => messages::permission_denied(detail),
        SendFailure::Refused(detail) => messages::request_failed(detail),
        SendFailure::ShoutoutCooldown { retry_after_secs } => {
            messages::shoutout_cooldown(*retry_after_secs)
        }
        SendFailure::ModerationThrottled => messages::rate_limited(),
        SendFailure::Helix(error) => helix_message(error),
        SendFailure::Credentials(detail) => messages::credentials_unavailable(detail),
    }
}

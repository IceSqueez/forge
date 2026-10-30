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

#[cfg(test)]
mod tests {
    use forge_types::{MessageArg, UserText};

    use super::*;

    fn code(message: &MessageRef) -> String {
        message.code().to_string()
    }

    fn detail(message: &MessageRef) -> Option<&str> {
        message.detail().map(|d| d.as_str())
    }

    #[test]
    fn each_helix_error_maps_to_its_own_code_with_status_and_detail_only_where_they_exist() {
        let cases: [(HelixError, &str, Option<i64>, Option<&str>); 6] = [
            (HelixError::RateLimited, "twitch.rate_limited", None, None),
            (
                HelixError::ReauthRequired,
                "twitch.reauth_required",
                None,
                None,
            ),
            (
                HelixError::Credentials("keyring locked".into()),
                "twitch.credentials_unavailable",
                None,
                Some("keyring locked"),
            ),
            (
                HelixError::Http {
                    status: 418,
                    body: "teapot".into(),
                },
                "twitch.request_refused",
                Some(418),
                Some("teapot"),
            ),
            (
                HelixError::Transport("connection reset".into()),
                "twitch.request_failed",
                None,
                Some("connection reset"),
            ),
            (
                HelixError::UndecodableResponse("expected object".into()),
                "twitch.response_unreadable",
                None,
                Some("expected object"),
            ),
        ];
        for (error, expected_code, status, expected_detail) in cases {
            let message = helix_message(&error);
            assert_eq!(code(&message), expected_code);
            assert_eq!(message.arg("status"), status.map(MessageArg::Int).as_ref());
            assert_eq!(detail(&message), expected_detail);
        }
    }

    #[test]
    fn failed_wraps_the_mapped_message_and_outcome_is_success_only_for_ok() {
        let error = || HelixError::Http {
            status: 500,
            body: "boom".into(),
        };

        let failed_outcome = outcome::<()>(&Err(error()));

        assert_eq!(
            failed_outcome,
            SubActionOutcome::Failed(UserText::Message(helix_message(&error())))
        );
        assert_eq!(outcome(&Ok(())), SubActionOutcome::Success);
    }

    #[test]
    fn bot_unavailable_reasons_map_to_distinct_bot_codes() {
        for (reason, expected) in [
            (BotUnavailable::NotConnected, "twitch.bot_not_signed_in"),
            (BotUnavailable::ReauthRequired, "twitch.bot_reauth_required"),
            (
                BotUnavailable::StorageFailure,
                "twitch.bot_credentials_unreadable",
            ),
        ] {
            let message = send_failure(&SendFailure::BotNotConnected(reason));
            assert_eq!(code(&message), expected);
            assert_eq!(detail(&message), None);
        }
    }

    #[test]
    fn chat_send_errors_map_to_codes_with_limit_and_detail_arguments() {
        let too_long = send_failure(&SendFailure::Chat(ChatSendError::MessageTooLong));
        let dropped = send_failure(&SendFailure::Chat(ChatSendError::Dropped {
            code: "msg_duplicate".into(),
            message: "duplicate".into(),
        }));

        assert_eq!(code(&too_long), "twitch.message_too_long");
        assert_eq!(
            too_long.arg("limit"),
            Some(&MessageArg::Int(MAX_MESSAGE_LEN as i64))
        );
        assert_eq!(code(&dropped), "twitch.message_dropped");
        assert_eq!(detail(&dropped), Some("duplicate"));
        assert_eq!(
            dropped.arg("code"),
            Some(&MessageArg::Verbatim(forge_types::VerbatimText::new(
                "msg_duplicate"
            )))
        );
    }

    #[test]
    fn remaining_chat_send_errors_map_to_their_codes() {
        for (error, expected, expected_detail) in [
            (ChatSendError::RateLimited, "twitch.rate_limited", None),
            (
                ChatSendError::NotConnected,
                "twitch.chat_not_connected",
                None,
            ),
            (
                ChatSendError::ReauthRequired,
                "twitch.reauth_required",
                None,
            ),
            (
                ChatSendError::Http("socket closed".into()),
                "twitch.request_failed",
                Some("socket closed"),
            ),
            (
                ChatSendError::Forbidden("followers only".into()),
                "twitch.message_forbidden",
                Some("followers only"),
            ),
        ] {
            let message = send_failure(&SendFailure::Chat(error));
            assert_eq!(code(&message), expected);
            assert_eq!(detail(&message), expected_detail);
        }
    }

    #[test]
    fn whisper_errors_map_to_codes_and_helix_failures_delegate_to_the_helix_mapping() {
        let whisper_limit = send_failure(&SendFailure::Whisper(WhisperError::MessageTooLong));
        assert_eq!(code(&whisper_limit), "twitch.message_too_long");
        assert_eq!(
            whisper_limit.arg("limit"),
            Some(&MessageArg::Int(MAX_WHISPER_CHARS as i64))
        );

        for (error, expected) in [
            (WhisperError::EmptyRecipient, "twitch.recipient_empty"),
            (WhisperError::EmptyMessage, "twitch.message_empty"),
            (
                WhisperError::RecipientRejectsWhispers,
                "twitch.recipient_rejects_whispers",
            ),
        ] {
            assert_eq!(code(&send_failure(&SendFailure::Whisper(error))), expected);
        }

        let wrapped = send_failure(&SendFailure::Whisper(WhisperError::Helix(
            HelixError::Http {
                status: 403,
                body: "scope".into(),
            },
        )));
        assert_eq!(wrapped.arg("status"), Some(&MessageArg::Int(403)));
        assert_eq!(code(&wrapped), "twitch.request_refused");
    }

    #[test]
    fn moderation_failures_map_to_throttle_cooldown_permission_and_refusal_codes() {
        let cooldown = send_failure(&SendFailure::ShoutoutCooldown {
            retry_after_secs: 90,
        });
        assert_eq!(code(&cooldown), "twitch.shoutout_cooldown");
        assert_eq!(cooldown.arg("seconds"), Some(&MessageArg::Int(90)));

        for (failure, expected, expected_detail) in [
            (
                SendFailure::ModerationThrottled,
                "twitch.rate_limited",
                None,
            ),
            (
                SendFailure::BotPhoneUnverified,
                "twitch.bot_phone_unverified",
                None,
            ),
            (
                SendFailure::BotNotModerator,
                "twitch.bot_not_moderator",
                None,
            ),
            (
                SendFailure::PermissionDenied("HTTP 403: no".into()),
                "twitch.permission_denied",
                Some("HTTP 403: no"),
            ),
            (
                SendFailure::Refused("HTTP 400: offline".into()),
                "twitch.request_failed",
                Some("HTTP 400: offline"),
            ),
            (
                SendFailure::Credentials("unreadable".into()),
                "twitch.credentials_unavailable",
                Some("unreadable"),
            ),
            (
                SendFailure::Helix(HelixError::ReauthRequired),
                "twitch.reauth_required",
                None,
            ),
        ] {
            let message = send_failure(&failure);
            assert_eq!(code(&message), expected, "{failure:?}");
            assert_eq!(detail(&message), expected_detail, "{failure:?}");
        }
    }
}

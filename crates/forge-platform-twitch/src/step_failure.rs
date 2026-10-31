use crate::error::TwitchError;
use forge_types::{MessageRef, SubActionOutcome};

use crate::bot_account::BotUnavailable;
use crate::chat::{MAX_MESSAGE_LEN, MAX_WHISPER_CHARS};
use crate::vocabulary::messages;

pub(crate) fn helix_message(error: &TwitchError) -> MessageRef {
    match error {
        TwitchError::RateLimited | TwitchError::ModerationThrottled => messages::rate_limited(),
        TwitchError::ReauthRequired => messages::reauth_required(),
        TwitchError::Credentials(detail) | TwitchError::SendCredentials(detail) => {
            messages::credentials_unavailable(detail)
        }
        TwitchError::Http { status, .. } => messages::request_refused(*status),
        TwitchError::Transport(detail)
        | TwitchError::SendFailed(detail)
        | TwitchError::SendRefused(detail) => messages::request_failed(detail),
        TwitchError::UndecodableResponse(detail) => messages::response_unreadable(detail),
        TwitchError::MessageTooLong => messages::message_too_long(MAX_MESSAGE_LEN),
        TwitchError::NotConnected => messages::chat_not_connected(),
        TwitchError::MessageDropped { code, message } => messages::message_dropped(code, message),
        TwitchError::MessageForbidden(detail) => messages::message_forbidden(detail),
        TwitchError::WhisperRecipientEmpty => messages::recipient_empty(),
        TwitchError::WhisperMessageEmpty => messages::message_empty(),
        TwitchError::WhisperTooLong => messages::message_too_long(MAX_WHISPER_CHARS),
        TwitchError::RecipientRejectsWhispers => messages::recipient_rejects_whispers(),
        TwitchError::BotNotConnected(reason) => bot_unavailable_message(*reason),
        TwitchError::BotPhoneUnverified => messages::bot_phone_unverified(),
        TwitchError::BotNotModerator => messages::bot_not_moderator(),
        TwitchError::PermissionDenied(detail) => messages::permission_denied(detail),
        TwitchError::ShoutoutCooldown { retry_after_secs } => {
            messages::shoutout_cooldown(*retry_after_secs)
        }
        TwitchError::NotSignedIn => messages::credentials_unavailable(&error.to_string()),
        other => messages::request_failed(&other.to_string()),
    }
}

pub(crate) fn failed(error: &TwitchError) -> SubActionOutcome {
    SubActionOutcome::Failed(helix_message(error).into())
}

pub(crate) fn outcome<T>(result: &Result<T, TwitchError>) -> SubActionOutcome {
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
        let cases: [(TwitchError, &str, Option<i64>, Option<&str>); 6] = [
            (TwitchError::RateLimited, "twitch.rate_limited", None, None),
            (
                TwitchError::ReauthRequired,
                "twitch.reauth_required",
                None,
                None,
            ),
            (
                TwitchError::Credentials("keyring locked".into()),
                "twitch.credentials_unavailable",
                None,
                Some("keyring locked"),
            ),
            (
                TwitchError::Http {
                    status: 418,
                    body: "teapot".into(),
                },
                "twitch.request_refused",
                Some(418),
                None,
            ),
            (
                TwitchError::Transport("connection reset".into()),
                "twitch.request_failed",
                None,
                Some("connection reset"),
            ),
            (
                TwitchError::UndecodableResponse("expected object".into()),
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
        let error = || TwitchError::Http {
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
            let message = helix_message(&TwitchError::BotNotConnected(reason));
            assert_eq!(code(&message), expected);
            assert_eq!(detail(&message), None);
        }
    }

    #[test]
    fn chat_send_errors_map_to_codes_with_limit_and_detail_arguments() {
        let too_long = helix_message(&TwitchError::MessageTooLong);
        let dropped = helix_message(&TwitchError::MessageDropped {
            code: "msg_duplicate".into(),
            message: "duplicate".into(),
        });

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
            (TwitchError::RateLimited, "twitch.rate_limited", None),
            (TwitchError::NotConnected, "twitch.chat_not_connected", None),
            (TwitchError::ReauthRequired, "twitch.reauth_required", None),
            (
                TwitchError::SendFailed("socket closed".into()),
                "twitch.request_failed",
                Some("socket closed"),
            ),
            (
                TwitchError::MessageForbidden("followers only".into()),
                "twitch.message_forbidden",
                Some("followers only"),
            ),
        ] {
            let message = helix_message(&error);
            assert_eq!(code(&message), expected);
            assert_eq!(detail(&message), expected_detail);
        }
    }

    #[test]
    fn whisper_errors_map_to_codes_and_helix_failures_delegate_to_the_helix_mapping() {
        let whisper_limit = helix_message(&TwitchError::WhisperTooLong);
        assert_eq!(code(&whisper_limit), "twitch.message_too_long");
        assert_eq!(
            whisper_limit.arg("limit"),
            Some(&MessageArg::Int(MAX_WHISPER_CHARS as i64))
        );

        for (error, expected) in [
            (TwitchError::WhisperRecipientEmpty, "twitch.recipient_empty"),
            (TwitchError::WhisperMessageEmpty, "twitch.message_empty"),
            (
                TwitchError::RecipientRejectsWhispers,
                "twitch.recipient_rejects_whispers",
            ),
        ] {
            assert_eq!(code(&helix_message(&error)), expected);
        }

        let wrapped = helix_message(&TwitchError::Http {
            status: 403,
            body: "scope".into(),
        });
        assert_eq!(wrapped.arg("status"), Some(&MessageArg::Int(403)));
        assert_eq!(code(&wrapped), "twitch.request_refused");
    }

    #[test]
    fn moderation_failures_map_to_throttle_cooldown_permission_and_refusal_codes() {
        let cooldown = helix_message(&TwitchError::ShoutoutCooldown {
            retry_after_secs: 90,
        });
        assert_eq!(code(&cooldown), "twitch.shoutout_cooldown");
        assert_eq!(cooldown.arg("seconds"), Some(&MessageArg::Int(90)));

        for (failure, expected, expected_detail) in [
            (
                TwitchError::ModerationThrottled,
                "twitch.rate_limited",
                None,
            ),
            (
                TwitchError::BotPhoneUnverified,
                "twitch.bot_phone_unverified",
                None,
            ),
            (
                TwitchError::BotNotModerator,
                "twitch.bot_not_moderator",
                None,
            ),
            (
                TwitchError::PermissionDenied("HTTP 403: no".into()),
                "twitch.permission_denied",
                Some("HTTP 403: no"),
            ),
            (
                TwitchError::SendRefused("HTTP 400: offline".into()),
                "twitch.request_failed",
                Some("HTTP 400: offline"),
            ),
            (
                TwitchError::SendCredentials("unreadable".into()),
                "twitch.credentials_unavailable",
                Some("unreadable"),
            ),
            (TwitchError::ReauthRequired, "twitch.reauth_required", None),
        ] {
            let message = helix_message(&failure);
            assert_eq!(code(&message), expected, "{failure:?}");
            assert_eq!(detail(&message), expected_detail, "{failure:?}");
        }
    }
}

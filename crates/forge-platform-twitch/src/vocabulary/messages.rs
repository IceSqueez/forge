use forge_types::{MessageCatalog, MessageCode, MessageRef, UpstreamDetail, VerbatimText};

use crate::vocabulary::PLATFORM_ID;

pub const HEALTH_CHAT: MessageCode = MessageCode::from_static(PLATFORM_ID, "health_chat");
pub const HEALTH_EVENTSUB: MessageCode = MessageCode::from_static(PLATFORM_ID, "health_eventsub");
pub const HEALTH_API_CALLS: MessageCode = MessageCode::from_static(PLATFORM_ID, "health_api_calls");

pub const HEALTH_CHAT_JOINED: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "health_chat_joined");
pub const HEALTH_EVENTSUB_SUBS: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "health_eventsub_subs");
pub const HEALTH_VIEWERS_LIVE: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "health_viewers_live");
pub const HEALTH_VIEWERS_OFFLINE: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "health_viewers_offline");
pub const HEALTH_RESETS_IN: MessageCode = MessageCode::from_static(PLATFORM_ID, "health_resets_in");
pub const HEALTH_NO_USAGE: MessageCode = MessageCode::from_static(PLATFORM_ID, "health_no_usage");

const RATE_LIMITED: MessageCode = MessageCode::from_static(PLATFORM_ID, "rate_limited");
const REAUTH_REQUIRED: MessageCode = MessageCode::from_static(PLATFORM_ID, "reauth_required");
const CREDENTIALS_UNAVAILABLE: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "credentials_unavailable");
const REQUEST_REFUSED: MessageCode = MessageCode::from_static(PLATFORM_ID, "request_refused");
const REQUEST_FAILED: MessageCode = MessageCode::from_static(PLATFORM_ID, "request_failed");
const RESPONSE_UNREADABLE: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "response_unreadable");
const FIELD_REQUIRED: MessageCode = MessageCode::from_static(PLATFORM_ID, "field_required");
const UNRESOLVED_REFERENCE: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "unresolved_reference");
const INVALID_HEX_COLOR: MessageCode = MessageCode::from_static(PLATFORM_ID, "invalid_hex_color");
const TARGET_EMPTY: MessageCode = MessageCode::from_static(PLATFORM_ID, "target_empty");
const CATEGORY_EMPTY: MessageCode = MessageCode::from_static(PLATFORM_ID, "category_empty");
const EMPTY_RESPONSE: MessageCode = MessageCode::from_static(PLATFORM_ID, "empty_response");
const RESPONSE_FIELD_MISSING: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "response_field_missing");
const MESSAGE_EMPTY: MessageCode = MessageCode::from_static(PLATFORM_ID, "message_empty");
const MESSAGE_TOO_LONG: MessageCode = MessageCode::from_static(PLATFORM_ID, "message_too_long");
const CHAT_NOT_CONNECTED: MessageCode = MessageCode::from_static(PLATFORM_ID, "chat_not_connected");
const BOT_NOT_SIGNED_IN: MessageCode = MessageCode::from_static(PLATFORM_ID, "bot_not_signed_in");
const BOT_REAUTH_REQUIRED: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "bot_reauth_required");
const BOT_CREDENTIALS_UNREADABLE: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "bot_credentials_unreadable");
const BOT_PHONE_UNVERIFIED: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "bot_phone_unverified");
const BOT_NOT_MODERATOR: MessageCode = MessageCode::from_static(PLATFORM_ID, "bot_not_moderator");
const PERMISSION_DENIED: MessageCode = MessageCode::from_static(PLATFORM_ID, "permission_denied");
const MESSAGE_FORBIDDEN: MessageCode = MessageCode::from_static(PLATFORM_ID, "message_forbidden");
const MESSAGE_DROPPED: MessageCode = MessageCode::from_static(PLATFORM_ID, "message_dropped");
const SHOUTOUT_COOLDOWN: MessageCode = MessageCode::from_static(PLATFORM_ID, "shoutout_cooldown");
const RECIPIENT_EMPTY: MessageCode = MessageCode::from_static(PLATFORM_ID, "recipient_empty");
const RECIPIENT_REJECTS_WHISPERS: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "recipient_rejects_whispers");

pub(crate) const CONDITION_ANY_SHARED_CHAT_MESSAGE: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "condition_any_shared_chat_message");
pub(crate) const CONDITION_ANY_PROFILE_CHANGE: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "condition_any_profile_change");
pub(crate) const CONDITION_ANY_REWARD: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "condition_any_reward");
pub(crate) const CONDITION_ANY_AMOUNT: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "condition_any_amount");
pub(crate) const CONDITION_ANY_STATE: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "condition_any_state");
pub(crate) const CONDITION_ANY_WHISPER: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "condition_any_whisper");
pub(crate) const CONDITION_ANY_STATUS: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "condition_any_status");
const CONDITION_REWARD_ID: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "condition_reward_id");
const CONDITION_REWARD: MessageCode = MessageCode::from_static(PLATFORM_ID, "condition_reward");
const CONDITION_STATE: MessageCode = MessageCode::from_static(PLATFORM_ID, "condition_state");
const CONDITION_STATUS: MessageCode = MessageCode::from_static(PLATFORM_ID, "condition_status");
const CONDITION_WHISPER_FROM: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "condition_whisper_from");
const CONDITION_WHISPER_TEXT: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "condition_whisper_text");
const CONDITION_AMOUNT_AT_LEAST: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "condition_amount_at_least");
const CONDITION_BITS_AT_LEAST: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "condition_bits_at_least");
const CONDITION_LEVEL_AT_LEAST: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "condition_level_at_least");
const REWARD_CONDITION_ARG: &str = "reward";
const STATE_CONDITION_ARG: &str = "state";
const STATUS_CONDITION_ARG: &str = "status";
const SENDER_CONDITION_ARG: &str = "sender";
const TEXT_CONDITION_ARG: &str = "text";
const MIN_CONDITION_ARG: &str = "min";
const LEVEL_CONDITION_ARG: &str = "level";

const CONDITION_BITS_BETWEEN: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "condition_bits_between");
const MAX_CONDITION_ARG: &str = "max";

pub const TWITCH_MESSAGES: MessageCatalog = MessageCatalog {
    namespace: PLATFORM_ID,
    codes: &[
        HEALTH_CHAT,
        HEALTH_EVENTSUB,
        HEALTH_API_CALLS,
        HEALTH_CHAT_JOINED,
        HEALTH_EVENTSUB_SUBS,
        HEALTH_VIEWERS_LIVE,
        HEALTH_VIEWERS_OFFLINE,
        HEALTH_RESETS_IN,
        HEALTH_NO_USAGE,
        RATE_LIMITED,
        REAUTH_REQUIRED,
        CREDENTIALS_UNAVAILABLE,
        REQUEST_REFUSED,
        REQUEST_FAILED,
        RESPONSE_UNREADABLE,
        FIELD_REQUIRED,
        UNRESOLVED_REFERENCE,
        INVALID_HEX_COLOR,
        TARGET_EMPTY,
        CATEGORY_EMPTY,
        EMPTY_RESPONSE,
        RESPONSE_FIELD_MISSING,
        MESSAGE_EMPTY,
        MESSAGE_TOO_LONG,
        CHAT_NOT_CONNECTED,
        BOT_NOT_SIGNED_IN,
        BOT_REAUTH_REQUIRED,
        BOT_CREDENTIALS_UNREADABLE,
        BOT_PHONE_UNVERIFIED,
        BOT_NOT_MODERATOR,
        PERMISSION_DENIED,
        MESSAGE_FORBIDDEN,
        MESSAGE_DROPPED,
        SHOUTOUT_COOLDOWN,
        RECIPIENT_EMPTY,
        RECIPIENT_REJECTS_WHISPERS,
        CONDITION_ANY_SHARED_CHAT_MESSAGE,
        CONDITION_BITS_BETWEEN,
        CONDITION_ANY_PROFILE_CHANGE,
        CONDITION_ANY_REWARD,
        CONDITION_ANY_AMOUNT,
        CONDITION_ANY_STATE,
        CONDITION_ANY_WHISPER,
        CONDITION_ANY_STATUS,
        CONDITION_REWARD_ID,
        CONDITION_REWARD,
        CONDITION_STATE,
        CONDITION_STATUS,
        CONDITION_WHISPER_FROM,
        CONDITION_WHISPER_TEXT,
        CONDITION_AMOUNT_AT_LEAST,
        CONDITION_BITS_AT_LEAST,
        CONDITION_LEVEL_AT_LEAST,
    ],
    en_ftl: include_str!("../../locales/en/messages.ftl"),
    uk_ftl: include_str!("../../locales/uk/messages.ftl"),
};

const STATUS_ARG: &str = "status";
const FIELD_ARG: &str = "field";
const LIMIT_ARG: &str = "limit";
const CODE_ARG: &str = "code";
const COUNT_ARG: &str = "count";
const SECONDS_ARG: &str = "seconds";

fn detailed(code: MessageCode, detail: &str) -> MessageRef {
    MessageRef::new(code).with_detail(UpstreamDetail::from_redacted(detail))
}

fn field(code: MessageCode, name: &str) -> MessageRef {
    MessageRef::new(code).with_arg(FIELD_ARG, VerbatimText::new(name))
}

pub(crate) fn health_chat_joined() -> MessageRef {
    MessageRef::new(HEALTH_CHAT_JOINED)
}

pub(crate) fn health_eventsub_subs(count: usize) -> MessageRef {
    MessageRef::new(HEALTH_EVENTSUB_SUBS).with_arg(COUNT_ARG, count)
}

pub(crate) fn health_viewers_live() -> MessageRef {
    MessageRef::new(HEALTH_VIEWERS_LIVE)
}

pub(crate) fn health_viewers_offline() -> MessageRef {
    MessageRef::new(HEALTH_VIEWERS_OFFLINE)
}

pub(crate) fn health_resets_in(seconds: u64) -> MessageRef {
    MessageRef::new(HEALTH_RESETS_IN).with_arg(SECONDS_ARG, seconds)
}

pub(crate) fn health_no_usage() -> MessageRef {
    MessageRef::new(HEALTH_NO_USAGE)
}

pub(crate) fn rate_limited() -> MessageRef {
    MessageRef::new(RATE_LIMITED)
}

pub(crate) fn reauth_required() -> MessageRef {
    MessageRef::new(REAUTH_REQUIRED)
}

pub(crate) fn credentials_unavailable(detail: &str) -> MessageRef {
    detailed(CREDENTIALS_UNAVAILABLE, detail)
}

pub(crate) fn request_refused(status: u16) -> MessageRef {
    MessageRef::new(REQUEST_REFUSED).with_arg(STATUS_ARG, i64::from(status))
}

pub(crate) fn request_failed(detail: &str) -> MessageRef {
    detailed(REQUEST_FAILED, detail)
}

pub(crate) fn response_unreadable(detail: &str) -> MessageRef {
    detailed(RESPONSE_UNREADABLE, detail)
}

pub(crate) fn field_required(name: &str) -> MessageRef {
    field(FIELD_REQUIRED, name)
}

pub(crate) fn unresolved_reference() -> MessageRef {
    MessageRef::new(UNRESOLVED_REFERENCE)
}

pub(crate) fn invalid_hex_color() -> MessageRef {
    MessageRef::new(INVALID_HEX_COLOR)
}

pub(crate) fn target_empty(login_field: &str, id_field: &str) -> MessageRef {
    MessageRef::new(TARGET_EMPTY)
        .with_arg("login", VerbatimText::new(login_field))
        .with_arg("id", VerbatimText::new(id_field))
}

pub(crate) fn category_empty() -> MessageRef {
    MessageRef::new(CATEGORY_EMPTY)
}

pub(crate) fn empty_response() -> MessageRef {
    MessageRef::new(EMPTY_RESPONSE)
}

pub(crate) fn response_field_missing(name: &str) -> MessageRef {
    field(RESPONSE_FIELD_MISSING, name)
}

pub(crate) fn message_empty() -> MessageRef {
    MessageRef::new(MESSAGE_EMPTY)
}

pub(crate) fn message_too_long(limit: usize) -> MessageRef {
    MessageRef::new(MESSAGE_TOO_LONG).with_arg(LIMIT_ARG, limit)
}

pub(crate) fn chat_not_connected() -> MessageRef {
    MessageRef::new(CHAT_NOT_CONNECTED)
}

pub(crate) fn bot_not_signed_in() -> MessageRef {
    MessageRef::new(BOT_NOT_SIGNED_IN)
}

pub(crate) fn bot_reauth_required() -> MessageRef {
    MessageRef::new(BOT_REAUTH_REQUIRED)
}

pub(crate) fn bot_credentials_unreadable() -> MessageRef {
    MessageRef::new(BOT_CREDENTIALS_UNREADABLE)
}

pub(crate) fn bot_phone_unverified() -> MessageRef {
    MessageRef::new(BOT_PHONE_UNVERIFIED)
}

pub(crate) fn bot_not_moderator() -> MessageRef {
    MessageRef::new(BOT_NOT_MODERATOR)
}

pub(crate) fn permission_denied(detail: &str) -> MessageRef {
    detailed(PERMISSION_DENIED, detail)
}

pub(crate) fn message_forbidden(detail: &str) -> MessageRef {
    detailed(MESSAGE_FORBIDDEN, detail)
}

pub(crate) fn message_dropped(code: &str, detail: &str) -> MessageRef {
    detailed(MESSAGE_DROPPED, detail).with_arg(CODE_ARG, VerbatimText::new(code))
}

pub(crate) fn recipient_empty() -> MessageRef {
    MessageRef::new(RECIPIENT_EMPTY)
}

pub(crate) fn recipient_rejects_whispers() -> MessageRef {
    MessageRef::new(RECIPIENT_REJECTS_WHISPERS)
}

pub(crate) fn shoutout_cooldown(retry_after_secs: u64) -> MessageRef {
    MessageRef::new(SHOUTOUT_COOLDOWN).with_arg("seconds", retry_after_secs)
}

pub(crate) fn condition_reward_id(reward: &str) -> MessageRef {
    MessageRef::new(CONDITION_REWARD_ID).with_arg(REWARD_CONDITION_ARG, VerbatimText::new(reward))
}

pub(crate) fn condition_reward(reward: &str) -> MessageRef {
    MessageRef::new(CONDITION_REWARD).with_arg(REWARD_CONDITION_ARG, VerbatimText::new(reward))
}

pub(crate) fn condition_state(state: &str) -> MessageRef {
    MessageRef::new(CONDITION_STATE).with_arg(STATE_CONDITION_ARG, VerbatimText::new(state))
}

pub(crate) fn condition_status(status: &str) -> MessageRef {
    MessageRef::new(CONDITION_STATUS).with_arg(STATUS_CONDITION_ARG, VerbatimText::new(status))
}

pub(crate) fn condition_whisper_from(sender: &str) -> MessageRef {
    MessageRef::new(CONDITION_WHISPER_FROM)
        .with_arg(SENDER_CONDITION_ARG, VerbatimText::new(sender))
}

pub(crate) fn condition_whisper_text(text: &str) -> MessageRef {
    MessageRef::new(CONDITION_WHISPER_TEXT).with_arg(TEXT_CONDITION_ARG, VerbatimText::new(text))
}

pub(crate) fn condition_amount_at_least(min: i64) -> MessageRef {
    MessageRef::new(CONDITION_AMOUNT_AT_LEAST).with_arg(MIN_CONDITION_ARG, min)
}

pub(crate) fn condition_bits_between(min: i64, max: i64) -> MessageRef {
    MessageRef::new(CONDITION_BITS_BETWEEN)
        .with_arg(MIN_CONDITION_ARG, min)
        .with_arg(MAX_CONDITION_ARG, max)
}

pub(crate) fn condition_bits_at_least(min: i64) -> MessageRef {
    MessageRef::new(CONDITION_BITS_AT_LEAST).with_arg(MIN_CONDITION_ARG, min)
}

pub(crate) fn condition_level_at_least(level: i64) -> MessageRef {
    MessageRef::new(CONDITION_LEVEL_AT_LEAST).with_arg(LEVEL_CONDITION_ARG, level)
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::*;

    fn entries(source: &str) -> BTreeMap<String, BTreeSet<String>> {
        let mut found: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut current = None;
        for line in source.lines() {
            if let Some((id, _)) = line.split_once(" = ").filter(|_| line.starts_with("msg-")) {
                current = Some(id.trim().to_owned());
                found.entry(id.trim().to_owned()).or_default();
            }
            let Some(id) = &current else { continue };
            let mut rest = line;
            while let Some(at) = rest.find('$') {
                rest = &rest[at + 1..];
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                    .collect();
                found.get_mut(id).unwrap().insert(name);
            }
        }
        found
    }

    fn every_constructor() -> Vec<MessageRef> {
        vec![
            rate_limited(),
            reauth_required(),
            credentials_unavailable("d"),
            request_refused(403),
            request_failed("d"),
            response_unreadable("d"),
            field_required("f"),
            unresolved_reference(),
            invalid_hex_color(),
            target_empty("l", "i"),
            category_empty(),
            empty_response(),
            response_field_missing("f"),
            message_empty(),
            message_too_long(500),
            chat_not_connected(),
            bot_not_signed_in(),
            bot_reauth_required(),
            bot_credentials_unreadable(),
            bot_phone_unverified(),
            bot_not_moderator(),
            permission_denied("d"),
            message_forbidden("d"),
            message_dropped("c", "d"),
            recipient_empty(),
            recipient_rejects_whispers(),
            shoutout_cooldown(30),
            health_chat_joined(),
            health_eventsub_subs(3),
            health_viewers_live(),
            health_viewers_offline(),
            health_resets_in(9),
            health_no_usage(),
            MessageRef::new(CONDITION_ANY_SHARED_CHAT_MESSAGE),
            MessageRef::new(CONDITION_ANY_PROFILE_CHANGE),
            MessageRef::new(CONDITION_ANY_REWARD),
            MessageRef::new(CONDITION_ANY_AMOUNT),
            MessageRef::new(CONDITION_ANY_STATE),
            MessageRef::new(CONDITION_ANY_WHISPER),
            MessageRef::new(CONDITION_ANY_STATUS),
            condition_reward_id("r"),
            condition_reward("r"),
            condition_state("s"),
            condition_status("fulfilled"),
            condition_whisper_from("u"),
            condition_whisper_text("t"),
            condition_amount_at_least(1),
            condition_bits_between(1, 2),
            condition_bits_at_least(1),
            condition_level_at_least(1),
        ]
    }

    #[test]
    fn every_constructor_code_is_catalogued_and_localized_with_the_variables_it_passes() {
        let en = entries(TWITCH_MESSAGES.en_ftl);
        let uk = entries(TWITCH_MESSAGES.uk_ftl);
        for message in every_constructor() {
            let code = message.code();
            assert!(
                TWITCH_MESSAGES.codes.contains(code),
                "{code} not catalogued"
            );
            let passed: BTreeSet<String> = message.args().map(|(n, _)| n.to_owned()).collect();
            for (language, ftl) in [("en", &en), ("uk", &uk)] {
                let used = ftl
                    .get(&code.ftl_key())
                    .unwrap_or_else(|| panic!("{code} missing in {language}"));
                assert_eq!(used, &passed, "{code} variables in {language}");
            }
        }
    }

    #[test]
    fn every_catalogued_code_is_produced_by_a_constructor_or_is_a_health_label() {
        let produced: Vec<MessageCode> = every_constructor()
            .iter()
            .map(|m| m.code().clone())
            .collect();
        let health = [HEALTH_CHAT, HEALTH_EVENTSUB, HEALTH_API_CALLS];
        for code in TWITCH_MESSAGES.codes {
            assert!(
                produced.contains(code) || health.contains(code),
                "{code} has no constructor in the table"
            );
        }
    }

    #[test]
    fn english_and_ukrainian_declare_the_same_messages_with_the_same_variables() {
        assert_eq!(
            entries(TWITCH_MESSAGES.en_ftl),
            entries(TWITCH_MESSAGES.uk_ftl)
        );
    }

    #[test]
    fn refused_request_carries_the_status_and_no_upstream_detail() {
        let refused = request_refused(429);

        assert_eq!(
            refused.arg("status"),
            Some(&forge_types::MessageArg::Int(429))
        );
        assert!(refused.detail().is_none());
    }
}

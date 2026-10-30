use forge_types::{MessageCatalog, MessageCode, MessageRef, UpstreamDetail, VerbatimText};

use crate::vocabulary::PLATFORM_ID;

pub const HEALTH_CHAT: MessageCode = MessageCode::from_static(PLATFORM_ID, "health_chat");
pub const HEALTH_EVENTSUB: MessageCode = MessageCode::from_static(PLATFORM_ID, "health_eventsub");
pub const HEALTH_API_CALLS: MessageCode = MessageCode::from_static(PLATFORM_ID, "health_api_calls");

const RATE_LIMITED: MessageCode = MessageCode::from_static(PLATFORM_ID, "rate_limited");
const REAUTH_REQUIRED: MessageCode = MessageCode::from_static(PLATFORM_ID, "reauth_required");
const CREDENTIALS_UNAVAILABLE: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "credentials_unavailable");
const REQUEST_REFUSED: MessageCode = MessageCode::from_static(PLATFORM_ID, "request_refused");
const REQUEST_FAILED: MessageCode = MessageCode::from_static(PLATFORM_ID, "request_failed");
const RESPONSE_UNREADABLE: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "response_unreadable");
const FIELD_REQUIRED: MessageCode = MessageCode::from_static(PLATFORM_ID, "field_required");
const EMPTY_AFTER_INTERPOLATION: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "empty_after_interpolation");
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

pub const TWITCH_MESSAGES: MessageCatalog = MessageCatalog {
    namespace: PLATFORM_ID,
    codes: &[
        HEALTH_CHAT,
        HEALTH_EVENTSUB,
        HEALTH_API_CALLS,
        RATE_LIMITED,
        REAUTH_REQUIRED,
        CREDENTIALS_UNAVAILABLE,
        REQUEST_REFUSED,
        REQUEST_FAILED,
        RESPONSE_UNREADABLE,
        FIELD_REQUIRED,
        EMPTY_AFTER_INTERPOLATION,
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
    ],
    en_ftl: include_str!("../../locales/en/messages.ftl"),
    uk_ftl: include_str!("../../locales/uk/messages.ftl"),
};

const STATUS_ARG: &str = "status";
const FIELD_ARG: &str = "field";
const LIMIT_ARG: &str = "limit";
const CODE_ARG: &str = "code";

fn detailed(code: MessageCode, detail: &str) -> MessageRef {
    MessageRef::new(code).with_detail(UpstreamDetail::from_redacted(detail))
}

fn field(code: MessageCode, name: &str) -> MessageRef {
    MessageRef::new(code).with_arg(FIELD_ARG, VerbatimText::new(name))
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

pub(crate) fn request_refused(status: u16, detail: &str) -> MessageRef {
    detailed(REQUEST_REFUSED, detail).with_arg(STATUS_ARG, i64::from(status))
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

pub(crate) fn empty_after_interpolation(name: &str) -> MessageRef {
    field(EMPTY_AFTER_INTERPOLATION, name)
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

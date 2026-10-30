use crate::message::catalog::MessageCatalog;
use crate::message::code::MessageCode;

pub const COMMON_NAMESPACE: &str = "common";

pub const UPSTREAM_FAILURE: MessageCode =
    MessageCode::from_static(COMMON_NAMESPACE, "upstream_failure");

pub const INTEGRATION_DISABLED: MessageCode =
    MessageCode::from_static(COMMON_NAMESPACE, "integration_disabled");

pub const NO_CHAT_PLATFORM_ENABLED: MessageCode =
    MessageCode::from_static(COMMON_NAMESPACE, "no_chat_platform_enabled");

pub const NO_WHISPER_PLATFORM_ENABLED: MessageCode =
    MessageCode::from_static(COMMON_NAMESPACE, "no_whisper_platform_enabled");

pub const WHISPERS_UNSUPPORTED: MessageCode =
    MessageCode::from_static(COMMON_NAMESPACE, "whispers_unsupported");

pub const UNKNOWN_CHAT_TARGET: MessageCode =
    MessageCode::from_static(COMMON_NAMESPACE, "unknown_chat_target");

pub const COMMON_MESSAGES: MessageCatalog = MessageCatalog {
    namespace: COMMON_NAMESPACE,
    codes: &[
        UPSTREAM_FAILURE,
        INTEGRATION_DISABLED,
        NO_CHAT_PLATFORM_ENABLED,
        NO_WHISPER_PLATFORM_ENABLED,
        WHISPERS_UNSUPPORTED,
        UNKNOWN_CHAT_TARGET,
    ],
    en_ftl: include_str!("../../locales/en/messages.ftl"),
    uk_ftl: include_str!("../../locales/uk/messages.ftl"),
};

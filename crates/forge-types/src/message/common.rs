use crate::message::catalog::MessageCatalog;
use crate::message::code::MessageCode;

pub const COMMON_NAMESPACE: &str = "common";

pub const UPSTREAM_FAILURE: MessageCode =
    MessageCode::from_static(COMMON_NAMESPACE, "upstream_failure");

pub const COMMON_MESSAGES: MessageCatalog = MessageCatalog {
    namespace: COMMON_NAMESPACE,
    codes: &[UPSTREAM_FAILURE],
    en_ftl: include_str!("../../locales/en/messages.ftl"),
    uk_ftl: include_str!("../../locales/uk/messages.ftl"),
};

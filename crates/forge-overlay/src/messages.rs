use forge_types::{MessageCatalog, MessageCode};

const NAMESPACE: &str = "overlay";

pub(crate) const ALERT_LABEL: MessageCode = MessageCode::from_static(NAMESPACE, "kind_alert");
pub(crate) const ALERT_SUMMARY: MessageCode =
    MessageCode::from_static(NAMESPACE, "kind_alert_summary");
pub(crate) const BLANK_LABEL: MessageCode = MessageCode::from_static(NAMESPACE, "kind_blank");
pub(crate) const BLANK_SUMMARY: MessageCode =
    MessageCode::from_static(NAMESPACE, "kind_blank_summary");
pub(crate) const CHAT_LABEL: MessageCode = MessageCode::from_static(NAMESPACE, "kind_chat");
pub(crate) const CHAT_SUMMARY: MessageCode =
    MessageCode::from_static(NAMESPACE, "kind_chat_summary");
pub(crate) const FRAME_LABEL: MessageCode = MessageCode::from_static(NAMESPACE, "kind_frame");
pub(crate) const FRAME_SUMMARY: MessageCode =
    MessageCode::from_static(NAMESPACE, "kind_frame_summary");
pub(crate) const GOAL_LABEL: MessageCode = MessageCode::from_static(NAMESPACE, "kind_goal");
pub(crate) const GOAL_SUMMARY: MessageCode =
    MessageCode::from_static(NAMESPACE, "kind_goal_summary");
pub(crate) const LATEST_LABEL: MessageCode = MessageCode::from_static(NAMESPACE, "kind_latest");
pub(crate) const LATEST_SUMMARY: MessageCode =
    MessageCode::from_static(NAMESPACE, "kind_latest_summary");
pub(crate) const TICKER_LABEL: MessageCode = MessageCode::from_static(NAMESPACE, "kind_ticker");
pub(crate) const TICKER_SUMMARY: MessageCode =
    MessageCode::from_static(NAMESPACE, "kind_ticker_summary");

pub const OVERLAY_MESSAGES: MessageCatalog = MessageCatalog {
    namespace: NAMESPACE,
    codes: &[
        ALERT_LABEL,
        ALERT_SUMMARY,
        BLANK_LABEL,
        BLANK_SUMMARY,
        CHAT_LABEL,
        CHAT_SUMMARY,
        FRAME_LABEL,
        FRAME_SUMMARY,
        GOAL_LABEL,
        GOAL_SUMMARY,
        LATEST_LABEL,
        LATEST_SUMMARY,
        TICKER_LABEL,
        TICKER_SUMMARY,
    ],
    en_ftl: include_str!("../locales/en/messages.ftl"),
    uk_ftl: include_str!("../locales/uk/messages.ftl"),
};

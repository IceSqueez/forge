use forge_types::{MessageCatalog, MessageCode};

const INTEGRATION_ID: &str = "donatello";

pub(crate) const HEALTH_LAST_POLL: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_last_poll");
pub(crate) const HEALTH_LAST_ERROR: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_last_error");
pub(crate) const HEALTH_LAST_DONATION: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_last_donation");
pub(crate) const HEALTH_POLL_INTERVAL: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_poll_interval");
pub(crate) const HEALTH_TIME: MessageCode = MessageCode::from_static(INTEGRATION_ID, "health_time");
pub(crate) const PHASE_PAUSED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "phase_paused");
pub(crate) const PHASE_AWAITING_TOKEN: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "phase_awaiting_token");
pub(crate) const PHASE_STARTING: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "phase_starting");
pub(crate) const PHASE_POLLING: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "phase_polling");
pub(crate) const PHASE_RETRYING: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "phase_retrying");
pub(crate) const PHASE_ACTION_REQUIRED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "phase_action_required");
pub(crate) const FAILURE_NONE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "failure_none");
pub(crate) const FAILURE_MISSING_TOKEN: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "failure_missing_token");
pub(crate) const FAILURE_TOKEN_REJECTED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "failure_token_rejected");
pub(crate) const FAILURE_TOKEN_UNREADABLE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "failure_token_unreadable");
pub(crate) const FAILURE_RATE_LIMITED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "failure_rate_limited");
pub(crate) const FAILURE_NETWORK: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "failure_network");
pub(crate) const FAILURE_SERVER_ERROR: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "failure_server_error");
pub(crate) const FAILURE_MALFORMED_RESPONSE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "failure_malformed_response");
pub(crate) const FAILURE_INVALID_DONATION: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "failure_invalid_donation");
pub(crate) const FAILURE_LOCAL: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "failure_local");
pub(crate) const FAILURE_PROFILE_INCOMPLETE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "failure_profile_incomplete");

pub(crate) const LABEL_ACCOUNT_TITLE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "label_account_title");
pub(crate) const LABEL_NICKNAME: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "label_nickname");
pub(crate) const LABEL_PAGE: MessageCode = MessageCode::from_static(INTEGRATION_ID, "label_page");
pub(crate) const LABEL_STATUS: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "label_status");
pub(crate) const LABEL_POLL_INTERVAL: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "label_poll_interval");
pub(crate) const LABEL_POLL_INTERVAL_VALUE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "label_poll_interval_value");
pub(crate) const LABEL_BANNER_PROFILE_INCOMPLETE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "label_banner_profile_incomplete");
pub(crate) const LABEL_BANNER_TOKEN_REJECTED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "label_banner_token_rejected");
pub(crate) const LABEL_BANNER_TOKEN_UNREADABLE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "label_banner_token_unreadable");
pub(crate) const LABEL_BANNER_TOKEN_MISSING: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "label_banner_token_missing");

pub const DONATELLO_MESSAGES: MessageCatalog = MessageCatalog {
    namespace: INTEGRATION_ID,
    codes: &[
        HEALTH_LAST_POLL,
        HEALTH_LAST_ERROR,
        HEALTH_LAST_DONATION,
        HEALTH_POLL_INTERVAL,
        HEALTH_TIME,
        PHASE_PAUSED,
        PHASE_AWAITING_TOKEN,
        PHASE_STARTING,
        PHASE_POLLING,
        PHASE_RETRYING,
        PHASE_ACTION_REQUIRED,
        FAILURE_NONE,
        FAILURE_MISSING_TOKEN,
        FAILURE_TOKEN_REJECTED,
        FAILURE_TOKEN_UNREADABLE,
        FAILURE_RATE_LIMITED,
        FAILURE_NETWORK,
        FAILURE_SERVER_ERROR,
        FAILURE_MALFORMED_RESPONSE,
        FAILURE_INVALID_DONATION,
        FAILURE_LOCAL,
        FAILURE_PROFILE_INCOMPLETE,
        LABEL_ACCOUNT_TITLE,
        LABEL_NICKNAME,
        LABEL_PAGE,
        LABEL_STATUS,
        LABEL_POLL_INTERVAL,
        LABEL_POLL_INTERVAL_VALUE,
        LABEL_BANNER_PROFILE_INCOMPLETE,
        LABEL_BANNER_TOKEN_REJECTED,
        LABEL_BANNER_TOKEN_UNREADABLE,
        LABEL_BANNER_TOKEN_MISSING,
    ],
    en_ftl: include_str!("../locales/en/messages.ftl"),
    uk_ftl: include_str!("../locales/uk/messages.ftl"),
};

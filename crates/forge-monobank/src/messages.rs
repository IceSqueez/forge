use forge_types::{MessageCatalog, MessageCode};

const INTEGRATION_ID: &str = "monobank";

pub const HEALTH_LAST_POLL: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_last_poll");
pub const HEALTH_LAST_ERROR: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_last_error");
pub const HEALTH_LAST_DONATION: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_last_donation");
pub const HEALTH_JAR: MessageCode = MessageCode::from_static(INTEGRATION_ID, "health_jar");
pub const HEALTH_TIME: MessageCode = MessageCode::from_static(INTEGRATION_ID, "health_time");
pub const PHASE_PAUSED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "phase_paused");
pub const PHASE_AWAITING_TOKEN: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "phase_awaiting_token");
pub const PHASE_STARTING: MessageCode = MessageCode::from_static(INTEGRATION_ID, "phase_starting");
pub const PHASE_POLLING: MessageCode = MessageCode::from_static(INTEGRATION_ID, "phase_polling");
pub const PHASE_RETRYING: MessageCode = MessageCode::from_static(INTEGRATION_ID, "phase_retrying");
pub const PHASE_ACTION_REQUIRED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "phase_action_required");
pub const FAILURE_NONE: MessageCode = MessageCode::from_static(INTEGRATION_ID, "failure_none");
pub const FAILURE_MISSING_TOKEN: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "failure_missing_token");
pub const FAILURE_TOKEN_REJECTED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "failure_token_rejected");
pub const FAILURE_RATE_LIMITED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "failure_rate_limited");
pub const FAILURE_NETWORK: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "failure_network");
pub const FAILURE_SERVER_ERROR: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "failure_server_error");
pub const FAILURE_MALFORMED_RESPONSE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "failure_malformed_response");
pub const FAILURE_INVALID_DONATION: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "failure_invalid_donation");
pub const FAILURE_LOCAL: MessageCode = MessageCode::from_static(INTEGRATION_ID, "failure_local");
pub const FAILURE_JAR_NOT_SELECTED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "failure_jar_not_selected");
pub const FAILURE_JAR_MISSING: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "failure_jar_missing");

pub const MONOBANK_MESSAGES: MessageCatalog = MessageCatalog {
    namespace: INTEGRATION_ID,
    codes: &[
        HEALTH_LAST_POLL,
        HEALTH_LAST_ERROR,
        HEALTH_LAST_DONATION,
        HEALTH_JAR,
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
        FAILURE_RATE_LIMITED,
        FAILURE_NETWORK,
        FAILURE_SERVER_ERROR,
        FAILURE_MALFORMED_RESPONSE,
        FAILURE_INVALID_DONATION,
        FAILURE_LOCAL,
        FAILURE_JAR_NOT_SELECTED,
        FAILURE_JAR_MISSING,
    ],
    en_ftl: include_str!("../locales/en/messages.ftl"),
    uk_ftl: include_str!("../locales/uk/messages.ftl"),
};

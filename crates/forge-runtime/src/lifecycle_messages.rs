use forge_types::{MessageCatalog, MessageCode};

const LIFECYCLE_NAMESPACE: &str = "lifecycle";

pub(crate) const INTEGRATION_ARG: &str = "integration";
pub(crate) const SECONDS_ARG: &str = "seconds";

pub(crate) const START_TIMED_OUT: MessageCode =
    MessageCode::from_static(LIFECYCLE_NAMESPACE, "start_timed_out");
pub(crate) const STATE_NOT_SAVED: MessageCode =
    MessageCode::from_static(LIFECYCLE_NAMESPACE, "state_not_saved");
pub(crate) const SIGNED_OUT: MessageCode =
    MessageCode::from_static(LIFECYCLE_NAMESPACE, "signed_out");
pub(crate) const START_INTERRUPTED: MessageCode =
    MessageCode::from_static(LIFECYCLE_NAMESPACE, "start_interrupted");

pub const LIFECYCLE_MESSAGES: MessageCatalog = MessageCatalog {
    namespace: LIFECYCLE_NAMESPACE,
    codes: &[
        START_TIMED_OUT,
        STATE_NOT_SAVED,
        SIGNED_OUT,
        START_INTERRUPTED,
    ],
    en_ftl: include_str!("../locales/en/lifecycle.ftl"),
    uk_ftl: include_str!("../locales/uk/lifecycle.ftl"),
};

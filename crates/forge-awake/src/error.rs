#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AwakeError {
    #[error("no {service} is running")]
    ServiceMissing { service: &'static str },
    #[error("the {service} is unreachable: {reason}")]
    Unreachable {
        service: &'static str,
        reason: String,
    },
    #[error("the {service} refused the request: {reason}")]
    Refused {
        service: &'static str,
        reason: String,
    },
    #[error("keeping the machine awake is not supported on this platform")]
    Unsupported,
    #[error("the stay-awake service has stopped")]
    Stopped,
}

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AwakeService {
    SessionBus,
    SystemBus,
    ScreenSaver,
    LoginManager,
    PowerManager,
}

impl fmt::Display for AwakeService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::SessionBus => "session bus",
            Self::SystemBus => "system bus",
            Self::ScreenSaver => "screen saver service",
            Self::LoginManager => "login manager",
            Self::PowerManager => "power manager",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AwakeError {
    #[error("no {service} is running")]
    ServiceMissing { service: AwakeService },
    #[error("the {service} is unreachable: {reason}")]
    Unreachable {
        service: AwakeService,
        reason: String,
    },
    #[error("the {service} refused the request: {reason}")]
    Refused {
        service: AwakeService,
        reason: String,
    },
    #[error("keeping the machine awake is not supported on this platform")]
    Unsupported,
    #[error("the stay-awake service has stopped")]
    Stopped,
}

use forge_components::tr;
use forge_donatello::DonatelloError;
use forge_monobank::MonobankError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenProblem {
    Missing,
    Malformed,
    Rejected,
    ProfileIncomplete,
    NoJarPicked,
    JarNotFound,
    CoolingDown { retry_after_secs: u32 },
    Unreachable,
    ServiceError { status: u16 },
    UnreadableAnswer,
    StorageFailed,
    Local,
}

impl TokenProblem {
    pub fn of_donatello(error: &DonatelloError) -> Self {
        match error {
            DonatelloError::MissingToken => Self::Missing,
            DonatelloError::MalformedToken => Self::Malformed,
            DonatelloError::Unauthorized => Self::Rejected,
            DonatelloError::ProfileIncomplete => Self::ProfileIncomplete,
            DonatelloError::RateLimited { retry_after_secs } => Self::CoolingDown {
                retry_after_secs: *retry_after_secs,
            },
            DonatelloError::RateLimitExhausted => Self::CoolingDown {
                retry_after_secs: 0,
            },
            DonatelloError::Http { status } => Self::ServiceError { status: *status },
            DonatelloError::Network { .. } => Self::Unreachable,
            DonatelloError::MalformedResponse { .. } | DonatelloError::InvalidDonation { .. } => {
                Self::UnreadableAnswer
            }
            DonatelloError::Credential { .. } => Self::StorageFailed,
            DonatelloError::ClientInit { .. } | DonatelloError::NoRuntime => Self::Local,
        }
    }

    pub fn of_monobank(error: &MonobankError) -> Self {
        match error {
            MonobankError::MissingToken => Self::Missing,
            MonobankError::MalformedToken => Self::Malformed,
            MonobankError::Unauthorized => Self::Rejected,
            MonobankError::MissingJar | MonobankError::InvalidJarId => Self::NoJarPicked,
            MonobankError::JarNotFound => Self::JarNotFound,
            MonobankError::CoolingDown { retry_after_secs }
            | MonobankError::RateLimited { retry_after_secs } => Self::CoolingDown {
                retry_after_secs: *retry_after_secs,
            },
            MonobankError::Http { status } => Self::ServiceError { status: *status },
            MonobankError::Network { .. } => Self::Unreachable,
            MonobankError::MalformedResponse { .. } | MonobankError::InvalidDonation { .. } => {
                Self::UnreadableAnswer
            }
            MonobankError::Credential { .. } => Self::StorageFailed,
            MonobankError::ClientInit { .. } | MonobankError::NoRuntime => Self::Local,
        }
    }

    pub fn message(&self, service: &str) -> String {
        match self {
            Self::Missing => tr!("donation_token_problem_missing"),
            Self::Malformed => tr!("donation_token_problem_malformed"),
            Self::Rejected => tr!("donation_token_problem_rejected", service = service),
            Self::ProfileIncomplete => tr!("donation_token_problem_profile_incomplete"),
            Self::NoJarPicked => tr!("donation_token_problem_no_jar"),
            Self::JarNotFound => tr!("donation_token_problem_jar_not_found"),
            Self::CoolingDown {
                retry_after_secs: 0,
            } => tr!(
                "donation_token_problem_cooling_down_soon",
                service = service
            ),
            Self::CoolingDown { retry_after_secs } => tr!(
                "donation_token_problem_cooling_down",
                service = service,
                seconds = i64::from(*retry_after_secs)
            ),
            Self::Unreachable => tr!("donation_token_problem_unreachable", service = service),
            Self::ServiceError { status } => tr!(
                "donation_token_problem_service_error",
                service = service,
                status = i64::from(*status)
            ),
            Self::UnreadableAnswer => {
                tr!("donation_token_problem_unreadable", service = service)
            }
            Self::StorageFailed => tr!("donation_token_problem_storage"),
            Self::Local => tr!("donation_token_problem_local"),
        }
    }
}

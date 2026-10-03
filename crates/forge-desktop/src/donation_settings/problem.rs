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

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    const SERVICE: &str = "Donatello";

    fn in_english() {
        crate::i18n::install_language(forge_storage::Language::En);
    }

    fn reason() -> String {
        "boom".to_owned()
    }

    #[test]
    fn every_donatello_error_maps_to_the_problem_the_user_can_act_on() {
        let cases = [
            (DonatelloError::MissingToken, TokenProblem::Missing),
            (DonatelloError::MalformedToken, TokenProblem::Malformed),
            (DonatelloError::Unauthorized, TokenProblem::Rejected),
            (
                DonatelloError::ProfileIncomplete,
                TokenProblem::ProfileIncomplete,
            ),
            (
                DonatelloError::RateLimited {
                    retry_after_secs: 42,
                },
                TokenProblem::CoolingDown {
                    retry_after_secs: 42,
                },
            ),
            (
                DonatelloError::RateLimitExhausted,
                TokenProblem::CoolingDown {
                    retry_after_secs: 0,
                },
            ),
            (
                DonatelloError::Http { status: 503 },
                TokenProblem::ServiceError { status: 503 },
            ),
            (
                DonatelloError::Network { reason: reason() },
                TokenProblem::Unreachable,
            ),
            (
                DonatelloError::MalformedResponse { reason: reason() },
                TokenProblem::UnreadableAnswer,
            ),
            (
                DonatelloError::InvalidDonation {
                    donation_id: "d-1".to_owned(),
                    reason: reason(),
                },
                TokenProblem::UnreadableAnswer,
            ),
            (
                DonatelloError::Credential { reason: reason() },
                TokenProblem::StorageFailed,
            ),
            (
                DonatelloError::ClientInit { reason: reason() },
                TokenProblem::Local,
            ),
            (DonatelloError::NoRuntime, TokenProblem::Local),
        ];

        for (error, expected) in cases {
            assert_eq!(TokenProblem::of_donatello(&error), expected, "{error:?}");
        }
    }

    #[test]
    fn every_monobank_error_maps_to_the_problem_the_user_can_act_on() {
        let cases = [
            (MonobankError::MissingToken, TokenProblem::Missing),
            (MonobankError::MalformedToken, TokenProblem::Malformed),
            (MonobankError::Unauthorized, TokenProblem::Rejected),
            (MonobankError::MissingJar, TokenProblem::NoJarPicked),
            (MonobankError::InvalidJarId, TokenProblem::NoJarPicked),
            (MonobankError::JarNotFound, TokenProblem::JarNotFound),
            (
                MonobankError::CoolingDown {
                    retry_after_secs: 17,
                },
                TokenProblem::CoolingDown {
                    retry_after_secs: 17,
                },
            ),
            (
                MonobankError::RateLimited {
                    retry_after_secs: 60,
                },
                TokenProblem::CoolingDown {
                    retry_after_secs: 60,
                },
            ),
            (
                MonobankError::Http { status: 500 },
                TokenProblem::ServiceError { status: 500 },
            ),
            (
                MonobankError::Network { reason: reason() },
                TokenProblem::Unreachable,
            ),
            (
                MonobankError::MalformedResponse { reason: reason() },
                TokenProblem::UnreadableAnswer,
            ),
            (
                MonobankError::InvalidDonation {
                    donation_id: "m-1".to_owned(),
                    reason: reason(),
                },
                TokenProblem::UnreadableAnswer,
            ),
            (
                MonobankError::Credential { reason: reason() },
                TokenProblem::StorageFailed,
            ),
            (
                MonobankError::ClientInit { reason: reason() },
                TokenProblem::Local,
            ),
            (MonobankError::NoRuntime, TokenProblem::Local),
        ];

        for (error, expected) in cases {
            assert_eq!(TokenProblem::of_monobank(&error), expected, "{error:?}");
        }
    }

    #[test]
    fn each_problem_reads_differently_and_never_as_a_raw_message_key() {
        in_english();
        let problems = [
            TokenProblem::Missing,
            TokenProblem::Malformed,
            TokenProblem::Rejected,
            TokenProblem::ProfileIncomplete,
            TokenProblem::NoJarPicked,
            TokenProblem::JarNotFound,
            TokenProblem::CoolingDown {
                retry_after_secs: 0,
            },
            TokenProblem::CoolingDown {
                retry_after_secs: 42,
            },
            TokenProblem::Unreachable,
            TokenProblem::ServiceError { status: 503 },
            TokenProblem::UnreadableAnswer,
            TokenProblem::StorageFailed,
            TokenProblem::Local,
        ];

        let messages: Vec<String> = problems
            .iter()
            .map(|problem| problem.message(SERVICE))
            .collect();

        for (problem, message) in problems.iter().zip(&messages) {
            assert!(
                !message.contains("donation_token_problem"),
                "{problem:?} rendered its key: {message}"
            );
        }
        let distinct: BTreeSet<&String> = messages.iter().collect();
        assert_eq!(
            distinct.len(),
            problems.len(),
            "two problems share one text: {messages:#?}"
        );
    }

    #[test]
    fn a_cooldown_names_the_wait_only_when_the_service_said_how_long() {
        in_english();
        let known = TokenProblem::CoolingDown {
            retry_after_secs: 42,
        }
        .message(SERVICE);
        let unknown = TokenProblem::CoolingDown {
            retry_after_secs: 0,
        }
        .message(SERVICE);

        assert!(known.contains("42"), "{known}");
        assert!(!unknown.contains('0'), "a zero wait is shown as {unknown}");
    }

    #[test]
    fn problems_about_the_service_name_it_and_carry_the_http_status() {
        in_english();
        for problem in [
            TokenProblem::Rejected,
            TokenProblem::CoolingDown {
                retry_after_secs: 0,
            },
            TokenProblem::Unreachable,
            TokenProblem::UnreadableAnswer,
        ] {
            assert!(
                problem.message("monobank").contains("monobank"),
                "{problem:?} does not say which service"
            );
        }
        assert!(
            TokenProblem::ServiceError { status: 503 }
                .message("monobank")
                .contains("503")
        );
    }
}

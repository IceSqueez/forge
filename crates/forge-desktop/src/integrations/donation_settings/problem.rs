use forge_components::tr;
pub(crate) use forge_platform_core::TokenProblem;

pub(crate) fn problem_message(problem: &TokenProblem, service: &str) -> String {
    match problem {
        TokenProblem::Missing => tr!("donation_token_problem_missing"),
        TokenProblem::Malformed => tr!("donation_token_problem_malformed"),
        TokenProblem::Rejected => tr!("donation_token_problem_rejected", service = service),
        TokenProblem::ProfileIncomplete => tr!("donation_token_problem_profile_incomplete"),
        TokenProblem::NoJarPicked => tr!("donation_token_problem_no_jar"),
        TokenProblem::JarNotFound => tr!("donation_token_problem_jar_not_found"),
        TokenProblem::CoolingDown {
            retry_after_secs: 0,
        } => tr!(
            "donation_token_problem_cooling_down_soon",
            service = service
        ),
        TokenProblem::CoolingDown { retry_after_secs } => tr!(
            "donation_token_problem_cooling_down",
            service = service,
            seconds = i64::from(*retry_after_secs)
        ),
        TokenProblem::Unreachable => tr!("donation_token_problem_unreachable", service = service),
        TokenProblem::ServiceError { status } => tr!(
            "donation_token_problem_service_error",
            service = service,
            status = i64::from(*status)
        ),
        TokenProblem::UnreadableAnswer => {
            tr!("donation_token_problem_unreadable", service = service)
        }
        TokenProblem::StorageFailed => tr!("donation_token_problem_storage"),
        TokenProblem::Local => tr!("donation_token_problem_local"),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use forge_donatello::DonatelloError;
    use forge_monobank::MonobankError;

    use super::*;

    const SERVICE: &str = "Donatello";

    fn in_english() {
        crate::app::i18n::install_language(forge_storage::Language::En);
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
                DonatelloError::CredentialBundle { reason: reason() },
                TokenProblem::StorageFailed,
            ),
            (
                DonatelloError::ClientInit { reason: reason() },
                TokenProblem::Local,
            ),
            (DonatelloError::NoRuntime, TokenProblem::Local),
        ];

        for (error, expected) in cases {
            assert_eq!(TokenProblem::from(&error), expected, "{error:?}");
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
            assert_eq!(TokenProblem::from(&error), expected, "{error:?}");
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
            .map(|problem| problem_message(problem, SERVICE))
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
        let known = problem_message(
            &TokenProblem::CoolingDown {
                retry_after_secs: 42,
            },
            SERVICE,
        );
        let unknown = problem_message(
            &TokenProblem::CoolingDown {
                retry_after_secs: 0,
            },
            SERVICE,
        );

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
                problem_message(&problem, "monobank").contains("monobank"),
                "{problem:?} does not say which service"
            );
        }
        assert!(
            problem_message(&TokenProblem::ServiceError { status: 503 }, "monobank")
                .contains("503")
        );
    }
}

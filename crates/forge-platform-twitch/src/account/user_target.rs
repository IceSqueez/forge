use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigProblem, ConfigReason, Interpolation, SubActionConfigExt, TextField, TypedField,
};
use forge_types::ArgStack;

use super::identity::{login_handle, resolve_user_id};
use crate::helix::{HelixError, HelixTransport};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UserTarget<'a> {
    Id(&'a str),
    Login(&'a str),
}

impl<'a> UserTarget<'a> {
    pub(crate) fn pick(id: &'a str, login: &'a str) -> Option<Self> {
        let id = id.trim();
        let login = login_handle(login);
        if !id.is_empty() {
            Some(Self::Id(id))
        } else if !login.is_empty() {
            Some(Self::Login(login))
        } else {
            None
        }
    }

    pub(crate) async fn user_id(
        self,
        transport: &dyn HelixTransport,
    ) -> Result<String, HelixError> {
        match self {
            Self::Id(id) => Ok(id.to_owned()),
            Self::Login(login) => resolve_user_id(transport, login).await,
        }
    }
}

pub(crate) struct TargetFields {
    pub(crate) id: TextField,
    pub(crate) login: TextField,
}

impl TargetFields {
    pub(crate) fn check(
        &self,
        config: &SubActionConfig,
        interpolation: Interpolation<'_>,
    ) -> Result<(), ConfigProblem> {
        self.id.peek(config, interpolation)?;
        let stored = |key| config.str(key).unwrap_or_default();
        if UserTarget::pick(stored(self.id.key), stored(self.login.key)).is_some() {
            return Ok(());
        }
        Err(ConfigProblem::field(self.login.key, ConfigReason::Required))
    }

    pub(crate) fn read(
        &self,
        config: &SubActionConfig,
        args: &ArgStack,
    ) -> Result<InterpolatedTarget, ConfigProblem> {
        Ok(InterpolatedTarget {
            id: self.id.read(config, args)?,
            login: self.login.read(config, args)?,
        })
    }

    pub(crate) fn empty_after_interpolation(&self) -> String {
        format!(
            "{} and {} are empty after interpolation",
            self.login.key, self.id.key
        )
    }
}

pub(crate) struct InterpolatedTarget {
    id: String,
    login: String,
}

impl InterpolatedTarget {
    pub(crate) fn target(&self) -> Option<UserTarget<'_>> {
        UserTarget::pick(&self.id, &self.login)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use forge_registry::{ConfigProblem, ConfigReason, RegistryError, SubActionRunner};
    use forge_types::{ArgStack, SubActionOutcome, Variant};

    use super::super::identity::SelfIdentity;
    use crate::helix::{HelixError, HelixRequest, HelixTransport};
    use crate::sub_actions::test_support::{MockCreds, MockTransport, make_ctx, users_fixture};
    use crate::sub_actions::{
        BanUserRunner, SendShoutoutRunner, SendWhisperRunner, TimeoutUserRunner, UnbanUserRunner,
    };

    const USERS_PATH: &str = "/helix/users";

    type BuildRunner = fn(Arc<dyn HelixTransport>, Arc<SelfIdentity>) -> Box<dyn SubActionRunner>;

    struct Case {
        kind: &'static str,
        build: BuildRunner,
        id_key: &'static str,
        login_key: &'static str,
        extra: &'static [(&'static str, &'static str)],
        sent_id: fn(&HelixRequest) -> Option<String>,
    }

    fn body_user_id(request: &HelixRequest) -> Option<String> {
        request.body.as_ref()?["data"]["user_id"]
            .as_str()
            .map(str::to_owned)
    }

    fn query_value(request: &HelixRequest, key: &str) -> Option<String> {
        request
            .query
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.clone())
    }

    fn cases() -> [Case; 5] {
        [
            Case {
                kind: "ban_user",
                build: |t, i| Box::new(BanUserRunner::new(t, i)),
                id_key: "target_user_id",
                login_key: "target_user_login",
                extra: &[("reason", "spam")],
                sent_id: body_user_id,
            },
            Case {
                kind: "timeout_user",
                build: |t, i| Box::new(TimeoutUserRunner::new(t, i)),
                id_key: "target_user_id",
                login_key: "target_user_login",
                extra: &[],
                sent_id: body_user_id,
            },
            Case {
                kind: "unban_user",
                build: |t, i| Box::new(UnbanUserRunner::new(t, i)),
                id_key: "target_user_id",
                login_key: "target_user_login",
                extra: &[],
                sent_id: |r| query_value(r, "user_id"),
            },
            Case {
                kind: "send_shoutout",
                build: |t, i| Box::new(SendShoutoutRunner::new(t, i)),
                id_key: "to_broadcaster_id",
                login_key: "to_broadcaster_login",
                extra: &[],
                sent_id: |r| query_value(r, "to_broadcaster_id"),
            },
            Case {
                kind: "send_whisper",
                build: |t, i| Box::new(SendWhisperRunner::new(t, i)),
                id_key: "to_user_id",
                login_key: "to_user_login",
                extra: &[("message", "hi")],
                sent_id: |r| query_value(r, "to_user_id"),
            },
        ]
    }

    impl Case {
        fn config(&self, id: Option<&str>, login: Option<&str>) -> BTreeMap<String, Variant> {
            let mut config: BTreeMap<String, Variant> = self
                .extra
                .iter()
                .map(|(k, v)| ((*k).to_owned(), Variant::String((*v).to_owned())))
                .collect();
            for (key, value) in [(self.id_key, id), (self.login_key, login)] {
                if let Some(value) = value {
                    config.insert(key.to_owned(), Variant::String(value.to_owned()));
                }
            }
            config
        }

        fn runner_with(
            &self,
            responses: Vec<Result<serde_json::Value, HelixError>>,
        ) -> (Arc<MockTransport>, Box<dyn SubActionRunner>) {
            let transport = Arc::new(MockTransport::returning_sequence(responses));
            let runner = (self.build)(
                Arc::clone(&transport) as Arc<dyn HelixTransport>,
                Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
            );
            (transport, runner)
        }

        async fn run(
            &self,
            config: &BTreeMap<String, Variant>,
            stack: &ArgStack,
        ) -> (Arc<MockTransport>, SubActionOutcome) {
            let (transport, runner) =
                self.runner_with(vec![users_fixture("555"), Ok(serde_json::Value::Null)]);
            let (telemetry, _) = runner.execute(config, &make_ctx(stack)).await;
            (transport, telemetry.outcome)
        }
    }

    fn assert_single_call_carrying(
        case: &Case,
        transport: &MockTransport,
        outcome: &SubActionOutcome,
        expected_id: &str,
    ) {
        assert_eq!(*outcome, SubActionOutcome::Success, "{}", case.kind);
        assert_eq!(
            transport.call_count(),
            1,
            "{}: no lookup expected",
            case.kind
        );
        let request = transport.last_request();
        assert_ne!(request.path, USERS_PATH, "{}", case.kind);
        assert_eq!(
            (case.sent_id)(&request).as_deref(),
            Some(expected_id),
            "{}",
            case.kind
        );
    }

    #[tokio::test]
    async fn id_key_targets_the_helix_call_without_a_users_lookup() {
        for case in cases() {
            let (transport, outcome) = case
                .run(&case.config(Some("1001"), None), &ArgStack::new())
                .await;

            assert_single_call_carrying(&case, &transport, &outcome, "1001");
        }
    }

    #[tokio::test]
    async fn id_key_wins_over_login_key_when_both_are_set() {
        for case in cases() {
            let (transport, outcome) = case
                .run(&case.config(Some("1001"), Some("target")), &ArgStack::new())
                .await;

            assert_single_call_carrying(&case, &transport, &outcome, "1001");
        }
    }

    #[tokio::test]
    async fn id_key_is_interpolated_from_variables() {
        let stack = ArgStack::new().set("viewer_id".to_owned(), Variant::String("2002".to_owned()));
        for case in cases() {
            let (transport, outcome) = case
                .run(&case.config(Some("%viewer_id%"), None), &stack)
                .await;

            assert_single_call_carrying(&case, &transport, &outcome, "2002");
        }
    }

    #[tokio::test]
    async fn empty_id_after_interpolation_falls_back_to_the_login_lookup() {
        let stack = ArgStack::new().set("viewer_id".to_owned(), Variant::String(String::new()));
        for case in cases() {
            let (transport, outcome) = case
                .run(&case.config(Some("%viewer_id%"), Some("target")), &stack)
                .await;

            assert_eq!(outcome, SubActionOutcome::Success, "{}", case.kind);
            assert_eq!(transport.call_count(), 2, "{}", case.kind);
            assert_eq!(transport.request(0).path, USERS_PATH, "{}", case.kind);
            assert_eq!(
                (case.sent_id)(&transport.last_request()).as_deref(),
                Some("555"),
                "{}",
                case.kind
            );
        }
    }

    #[tokio::test]
    async fn both_keys_empty_after_interpolation_fail_without_any_helix_call() {
        let stack = ArgStack::new()
            .set("viewer_id".to_owned(), Variant::String(String::new()))
            .set("user_login".to_owned(), Variant::String(String::new()));
        for case in cases() {
            let (transport, outcome) = case
                .run(
                    &case.config(Some("%viewer_id%"), Some("%user_login%")),
                    &stack,
                )
                .await;

            assert!(
                matches!(outcome, SubActionOutcome::Failed(_)),
                "{}: {outcome:?}",
                case.kind
            );
            assert_eq!(transport.call_count(), 0, "{}", case.kind);
        }
    }

    #[tokio::test]
    async fn bare_at_sign_or_blank_login_fails_without_any_helix_call() {
        for login in ["@", "   ", " @ "] {
            for case in cases() {
                let (transport, outcome) = case
                    .run(&case.config(Some("  "), Some(login)), &ArgStack::new())
                    .await;

                assert!(
                    matches!(outcome, SubActionOutcome::Failed(_)),
                    "{} login {login:?}: {outcome:?}",
                    case.kind
                );
                assert_eq!(transport.call_count(), 0, "{} login {login:?}", case.kind);
            }
        }
    }

    #[tokio::test]
    async fn a_non_text_id_or_login_fails_validation_and_execution_without_any_helix_call() {
        for case in cases() {
            for (non_text_key, other) in [
                (case.id_key, (None, Some("target"))),
                (case.login_key, (Some(""), None)),
            ] {
                let mut config = case.config(other.0, other.1);
                config.insert(non_text_key.to_owned(), Variant::Int(1001));
                let (_, runner) = case.runner_with(Vec::new());

                assert!(
                    runner.validate_config(&config).is_err(),
                    "{} {non_text_key}",
                    case.kind
                );
                let (transport, outcome) = case.run(&config, &ArgStack::new()).await;
                assert!(
                    matches!(outcome, SubActionOutcome::Failed(_)),
                    "{} {non_text_key}: {outcome:?}",
                    case.kind
                );
                assert_eq!(transport.call_count(), 0, "{} {non_text_key}", case.kind);
            }
        }
    }

    #[tokio::test]
    async fn whitespace_id_with_whitespace_or_bare_at_login_is_required_on_the_login_key() {
        for (id, login) in [
            (Some("  "), Some("  ")),
            (Some("  "), Some("@")),
            (Some(" "), Some(" @ ")),
            (None, Some("@")),
        ] {
            for case in cases() {
                let config = case.config(id, login);
                let (_, runner) = case.runner_with(Vec::new());

                let validated = runner.validate_config(&config);
                let (transport, outcome) = case.run(&config, &ArgStack::new()).await;

                let label = format!("{} id={id:?} login={login:?}", case.kind);
                assert_eq!(
                    validated.err().map(|e| e.to_string()),
                    Some(
                        RegistryError::InvalidConfig(ConfigProblem::field(
                            case.login_key,
                            ConfigReason::Required,
                        ))
                        .to_string()
                    ),
                    "{label}"
                );
                assert!(
                    matches!(outcome, SubActionOutcome::Failed(_)),
                    "{label}: {outcome:?}"
                );
                assert_eq!(transport.call_count(), 0, "{label}");
            }
        }
    }

    #[test]
    fn validate_config_requires_a_non_empty_id_or_login() {
        for case in cases() {
            let (_, runner) = case.runner_with(Vec::new());
            for (id, login, expected_ok) in [
                (Some("1001"), None, true),
                (None, Some("target"), true),
                (Some("1001"), Some(""), true),
                (Some(""), Some("target"), true),
                (None, None, false),
                (Some(""), Some(""), false),
                (Some(""), None, false),
            ] {
                let result = runner.validate_config(&case.config(id, login));
                assert_eq!(
                    result.is_ok(),
                    expected_ok,
                    "{} id={id:?} login={login:?}: {result:?}",
                    case.kind
                );
            }
        }
    }
}

use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::SubActionConfig;
use forge_registry::{
    ConfigProblem, ConfigSpec, DurationBounds, DurationField, Interpolation, RunContext, StepTimer,
    SubActionCategory, SubActionRunner, TypedField,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};

use super::fields::{MODERATION_TARGET, OPTIONAL_REASON};
use crate::account::identity::SelfIdentity;
use crate::account::user_target::{InterpolatedTarget, UserTarget};
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};
use crate::step_failure;
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::MODERATION_TIMEOUT_USER;
const DEFAULT_DURATION_SECONDS: i64 = 600;
const MIN_DURATION_SECONDS: i64 = 1;
const MAX_DURATION_SECONDS: i64 = 1_209_600;

pub(crate) const TIMEOUT_DURATION: DurationField = DurationField {
    key: config_keys::DURATION_SECONDS,
    label: "Duration",
    default: DEFAULT_DURATION_SECONDS,
    bounds: DurationBounds::in_seconds(MIN_DURATION_SECONDS, MAX_DURATION_SECONDS),
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[
    &MODERATION_TARGET.login,
    &OPTIONAL_REASON,
    &TIMEOUT_DURATION,
]);

struct TimeoutConfig {
    target: InterpolatedTarget,
    reason: String,
    duration: u64,
}

fn read_config(config: &SubActionConfig, args: &ArgStack) -> Result<TimeoutConfig, ConfigProblem> {
    Ok(TimeoutConfig {
        target: MODERATION_TARGET.read(config, args)?,
        reason: OPTIONAL_REASON.read(config, args)?,
        duration: TIMEOUT_DURATION.read(config, args)?.as_secs(),
    })
}

pub(crate) struct TimeoutUserRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl TimeoutUserRunner {
    pub(crate) fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn timeout(
        &self,
        target: Option<UserTarget<'_>>,
        reason: &str,
        duration: u64,
    ) -> SubActionOutcome {
        let Some(target) = target else {
            return SubActionOutcome::Failed(MODERATION_TARGET.empty_after_interpolation().into());
        };
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return step_failure::failed(&e),
        };
        let target_user_id = match target.user_id(self.transport.as_ref()).await {
            Ok(id) => id,
            Err(e) => return step_failure::failed(&e),
        };
        let mut data = serde_json::json!({ "user_id": target_user_id, "duration": duration });
        if !reason.is_empty() {
            data["reason"] = serde_json::Value::String(reason.to_owned());
        }
        let request = HelixRequest::new(HelixMethod::Post, "/helix/moderation/bans")
            .query("broadcaster_id", user_id.clone())
            .query("moderator_id", user_id)
            .body(serde_json::json!({ "data": data }));
        step_failure::outcome(&self.transport.execute(request).await)
    }
}

#[async_trait]
impl SubActionRunner for TimeoutUserRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Moderation
    }

    fn label(&self) -> &str {
        "Timeout User"
    }

    fn summary(&self) -> &str {
        "Temporarily times out a user from the channel chat."
    }

    fn search_text(&self) -> &str {
        "twitch moderation timeout mute silence user temporary"
    }

    fn icon_name(&self) -> &str {
        "clock-off"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn check_config(
        &self,
        config: &SubActionConfig,
        interpolation: Interpolation<'_>,
    ) -> Result<(), ConfigProblem> {
        MODERATION_TARGET.check(config, interpolation)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, KIND_ID);
        let read = match read_config(config, ctx.arg_stack) {
            Ok(read) => read,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let outcome = self
            .timeout(read.target.target(), &read.reason, read.duration)
            .await;
        (timer.finish(outcome), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helix::HelixError;
    use crate::sub_actions::fields::MAX_REASON_CHARS;
    use crate::sub_actions::test_support::{MockCreds, MockTransport, make_ctx, users_fixture};
    use forge_registry::{ConfigReason, RegistryError, ValueBounds};
    use forge_types::Variant;
    use std::collections::BTreeMap;

    fn runner_with(
        responses: Vec<Result<serde_json::Value, HelixError>>,
    ) -> (Arc<MockTransport>, TimeoutUserRunner) {
        let transport = Arc::new(MockTransport::returning_sequence(responses));
        let runner = TimeoutUserRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        );
        (transport, runner)
    }

    fn config(target: &str, reason: &str, duration: i64) -> SubActionConfig {
        BTreeMap::from([
            (
                "target_user_login".to_owned(),
                Variant::String(target.to_owned()),
            ),
            ("reason".to_owned(), Variant::String(reason.to_owned())),
            ("duration_seconds".to_owned(), Variant::Int(duration)),
        ])
    }

    #[tokio::test]
    async fn execute_posts_timeout_with_resolved_id_and_duration_in_body() {
        let (transport, runner) =
            runner_with(vec![users_fixture("555"), Ok(serde_json::Value::Null)]);
        let stack = ArgStack::new();

        let (telemetry, _) = runner
            .execute(&config("target", "calm down", 60), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let request = transport.last_request();
        assert_eq!(request.method, HelixMethod::Post);
        assert_eq!(request.path, "/helix/moderation/bans");
        assert_eq!(
            request.body,
            Some(serde_json::json!({
                "data": { "user_id": "555", "duration": 60, "reason": "calm down" }
            })),
            "duration is what distinguishes a timeout from a permanent ban"
        );
    }

    #[tokio::test]
    async fn execute_fails_an_out_of_range_duration_without_calling_helix() {
        let (transport, runner) =
            runner_with(vec![users_fixture("555"), Ok(serde_json::Value::Null)]);
        let stack = ArgStack::new();

        let (telemetry, _) = runner
            .execute(
                &config("target", "", MAX_DURATION_SECONDS + 1),
                &make_ctx(&stack),
            )
            .await;

        assert!(
            matches!(telemetry.outcome, SubActionOutcome::Failed(_)),
            "{:?}",
            telemetry.outcome
        );
        assert_eq!(transport.call_count(), 0);
    }

    #[tokio::test]
    async fn execute_reads_a_templated_duration_after_interpolation() {
        let (transport, runner) =
            runner_with(vec![users_fixture("555"), Ok(serde_json::Value::Null)]);
        let stack = ArgStack::new().set("secs".to_owned(), Variant::Int(90));
        let mut templated = config("target", "", 0);
        templated.insert(
            "duration_seconds".to_owned(),
            Variant::String("%secs%".to_owned()),
        );

        let (telemetry, _) = runner.execute(&templated, &make_ctx(&stack)).await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert_eq!(
            transport.last_request().body.unwrap()["data"]["duration"],
            serde_json::json!(90)
        );
    }

    #[test]
    fn validate_config_keys_out_of_range_durations_and_over_long_reasons_to_their_fields() {
        let runner = TimeoutUserRunner::new(
            Arc::new(MockTransport::returning(Ok(serde_json::Value::Null)))
                as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::empty()))),
        );
        let duration_problem = || {
            Some(forge_registry::ConfigProblem::field(
                "duration_seconds",
                ConfigReason::OutOfRange(ValueBounds::between(
                    MIN_DURATION_SECONDS,
                    MAX_DURATION_SECONDS,
                )),
            ))
        };
        let reason_problem = Some(forge_registry::ConfigProblem::field(
            "reason",
            ConfigReason::LengthOutOfRange(ValueBounds::at_most(MAX_REASON_CHARS)),
        ));
        for (duration, reason_chars, expected) in [
            (MIN_DURATION_SECONDS, 0, None),
            (MIN_DURATION_SECONDS - 1, 0, duration_problem()),
            (MAX_DURATION_SECONDS, 0, None),
            (MAX_DURATION_SECONDS + 1, 0, duration_problem()),
            (DEFAULT_DURATION_SECONDS, MAX_REASON_CHARS, None),
            (
                DEFAULT_DURATION_SECONDS,
                MAX_REASON_CHARS + 1,
                reason_problem,
            ),
        ] {
            let reason = "ї".repeat(usize::try_from(reason_chars).unwrap());
            let problem = runner
                .validate_config(&config("target", &reason, duration))
                .err()
                .map(RegistryError::into_config_problem);
            assert_eq!(
                problem, expected,
                "duration {duration}, reason {reason_chars} chars"
            );
        }
    }
}

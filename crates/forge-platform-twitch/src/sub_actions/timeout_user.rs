use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    FormField, RegistryError, RunContext, SubActionCategory, SubActionConfigExt, SubActionRunner,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use super::identity::{SelfIdentity, resolve_user_id};
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};

const KIND_ID: &str = "twitch.moderation.timeout_user";
const DEFAULT_DURATION_SECONDS: i64 = 600;
const MIN_DURATION_SECONDS: i64 = 1;
const MAX_DURATION_SECONDS: i64 = 1_209_600;

pub struct TimeoutUserRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl TimeoutUserRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn timeout(&self, target_login: &str, reason: &str, duration: i64) -> SubActionOutcome {
        if target_login.is_empty() {
            return SubActionOutcome::Failed(
                "target_user_login is empty after interpolation".to_owned(),
            );
        }
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(e.to_string()),
        };
        let target_user_id = match resolve_user_id(self.transport.as_ref(), target_login).await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(e.to_string()),
        };
        let mut data = serde_json::json!({ "user_id": target_user_id, "duration": duration });
        if !reason.is_empty() {
            data["reason"] = serde_json::Value::String(reason.to_owned());
        }
        let request = HelixRequest::new(HelixMethod::Post, "/helix/moderation/bans")
            .query("broadcaster_id", user_id.clone())
            .query("moderator_id", user_id)
            .body(serde_json::json!({ "data": data }));
        SubActionOutcome::from_result(&self.transport.execute(request).await)
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

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            (
                "target_user_login".to_owned(),
                Variant::String(String::new()),
            ),
            ("reason".to_owned(), Variant::String(String::new())),
            (
                "duration_seconds".to_owned(),
                Variant::Int(DEFAULT_DURATION_SECONDS),
            ),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "target_user_login",
                label: "Target Username",
                placeholder: "%user_login%",
            },
            FormField::Text {
                key: "reason",
                label: "Reason (optional, max 500 chars)",
                placeholder: "",
            },
            FormField::Integer {
                key: "duration_seconds",
                label: "Duration (seconds, 1-1209600)",
                min: MIN_DURATION_SECONDS,
                max: MAX_DURATION_SECONDS,
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("target_user_login") {
            Some(Variant::String(s)) if !s.is_empty() => {}
            _ => {
                return Err(RegistryError::InvalidConfig(format!(
                    "{KIND_ID}: 'target_user_login' must be a non-empty string"
                )));
            }
        }
        if let Some(Variant::String(r)) = config.get("reason")
            && r.chars().count() > 500
        {
            return Err(RegistryError::InvalidConfig(format!(
                "{KIND_ID}: 'reason' must not exceed 500 characters"
            )));
        }
        if let Some(Variant::Int(d)) = config.get("duration_seconds")
            && !((MIN_DURATION_SECONDS..=MAX_DURATION_SECONDS).contains(d))
        {
            return Err(RegistryError::InvalidConfig(format!(
                "{KIND_ID}: 'duration_seconds' must be between {MIN_DURATION_SECONDS} and {MAX_DURATION_SECONDS}"
            )));
        }
        Ok(())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let login_template = config.str("target_user_login").unwrap_or_default();
        let target_login = ctx.arg_stack.interpolate(login_template);
        let reason = config
            .str("reason")
            .map(|s| ctx.arg_stack.interpolate(s))
            .unwrap_or_default();
        let duration = config
            .get("duration_seconds")
            .and_then(|v| {
                if let Variant::Int(d) = v {
                    Some(*d)
                } else {
                    None
                }
            })
            .filter(|&d| (MIN_DURATION_SECONDS..=MAX_DURATION_SECONDS).contains(&d))
            .unwrap_or(DEFAULT_DURATION_SECONDS);

        let outcome = self.timeout(&target_login, &reason, duration).await;

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                kind: KIND_ID.to_owned(),
                started_at,
                duration_ms: start.elapsed().as_millis() as u64,
                outcome,
                index: ctx.index,
            },
            None,
        )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::helix::HelixError;
    use crate::sub_actions::test_support::{MockCreds, MockTransport, make_ctx, users_fixture};

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
    async fn empty_target_login_after_interpolation_fails_before_any_helix_call() {
        let (transport, runner) = runner_with(vec![users_fixture("555")]);
        let stack = ArgStack::new();

        let (telemetry, _) = runner.execute(&config("", "", 60), &make_ctx(&stack)).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert_eq!(
            transport.call_count(),
            0,
            "empty target must fail before the resolve call"
        );
    }

    #[tokio::test]
    async fn execute_replaces_out_of_range_duration_with_default() {
        let (transport, runner) =
            runner_with(vec![users_fixture("555"), Ok(serde_json::Value::Null)]);
        let stack = ArgStack::new();

        let (telemetry, _) = runner
            .execute(
                &config("target", "", MAX_DURATION_SECONDS + 1),
                &make_ctx(&stack),
            )
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let body = transport.last_request().body.unwrap();
        assert_eq!(
            body["data"]["duration"],
            serde_json::json!(DEFAULT_DURATION_SECONDS),
            "out-of-range duration must never reach Helix verbatim"
        );
    }

    #[test]
    fn validate_config_enforces_duration_bounds_inclusive() {
        let runner = TimeoutUserRunner::new(
            Arc::new(MockTransport::returning(Ok(serde_json::Value::Null)))
                as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::empty()))),
        );

        for (duration, expected_ok) in [
            (MIN_DURATION_SECONDS, true),
            (MIN_DURATION_SECONDS - 1, false),
            (MAX_DURATION_SECONDS, true),
            (MAX_DURATION_SECONDS + 1, false),
        ] {
            let result = runner.validate_config(&config("target", "", duration));
            assert_eq!(
                result.is_ok(),
                expected_ok,
                "duration {duration} expected ok={expected_ok}, got {result:?}"
            );
        }
    }
}

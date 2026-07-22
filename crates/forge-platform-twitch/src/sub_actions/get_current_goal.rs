use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use super::identity::SelfIdentity;
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};

const KIND_ID: &str = "twitch.goal.get_current";

pub struct GetCurrentGoalRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl GetCurrentGoalRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn fetch(&self) -> Result<Option<GoalData>, SubActionOutcome> {
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return Err(SubActionOutcome::Failed(e.to_string())),
        };

        // data[0] is the current active goal; Twitch allows at most one per type.
        let request =
            HelixRequest::new(HelixMethod::Get, "/helix/goals").query("broadcaster_id", user_id);

        let resp = self
            .transport
            .execute(request)
            .await
            .map_err(|e| SubActionOutcome::Failed(e.to_string()))?;

        let Some(first) = resp["data"].as_array().and_then(|arr| arr.first()) else {
            // Empty data array means no active goal - not an error.
            return Ok(None);
        };

        let id = first["id"]
            .as_str()
            .ok_or_else(|| SubActionOutcome::Failed("goal id missing in response".to_owned()))?
            .to_owned();

        let goal_type = first["type"]
            .as_str()
            .ok_or_else(|| SubActionOutcome::Failed("goal type missing in response".to_owned()))?
            .to_owned();

        let current_amount = first["current_amount"].as_i64().ok_or_else(|| {
            SubActionOutcome::Failed("goal current_amount missing in response".to_owned())
        })?;

        let target_amount = first["target_amount"].as_i64().ok_or_else(|| {
            SubActionOutcome::Failed("goal target_amount missing in response".to_owned())
        })?;

        // Twitch does not return an is_achieved field; derive it from the amounts.
        let is_achieved = current_amount >= target_amount;

        Ok(Some(GoalData {
            id,
            goal_type,
            current_amount,
            target_amount,
            is_achieved,
        }))
    }
}

struct GoalData {
    id: String,
    goal_type: String,
    current_amount: i64,
    target_amount: i64,
    is_achieved: bool,
}

#[async_trait]
impl SubActionRunner for GetCurrentGoalRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Twitch
    }

    fn label(&self) -> &str {
        "Get Current Goal"
    }

    fn summary(&self) -> &str {
        "Reads the broadcaster's active creator goal and exposes its details as variables."
    }

    fn search_text(&self) -> &str {
        "twitch goal creator follower subscription current target progress"
    }

    fn icon_name(&self) -> &str {
        "target"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![]
    }

    fn validate_config(&self, _config: &SubActionConfig) -> Result<(), RegistryError> {
        Ok(())
    }

    async fn execute(
        &self,
        _config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        match self.fetch().await {
            Ok(Some(goal)) => {
                let output_stack = ctx
                    .arg_stack
                    .clone()
                    .set("goal.exists".to_owned(), Variant::Bool(true))
                    .set("goal.id".to_owned(), Variant::String(goal.id))
                    .set("goal.type".to_owned(), Variant::String(goal.goal_type))
                    .set(
                        "goal.current_amount".to_owned(),
                        Variant::Int(goal.current_amount),
                    )
                    .set(
                        "goal.target_amount".to_owned(),
                        Variant::Int(goal.target_amount),
                    )
                    .set(
                        "goal.is_achieved".to_owned(),
                        Variant::Bool(goal.is_achieved),
                    );
                (
                    SubActionTelemetry {
                        args_in: ::std::collections::BTreeMap::new(),
                        produced: ::std::collections::BTreeMap::new(),
                        kind: KIND_ID.to_owned(),
                        started_at,
                        duration_ms: start.elapsed().as_millis() as u64,
                        outcome: SubActionOutcome::Success,
                        index: ctx.index,
                    },
                    Some(output_stack),
                )
            }
            // goal.exists=false on empty data - absence of an active goal is not an error.
            Ok(None) => {
                let output_stack = ctx
                    .arg_stack
                    .clone()
                    .set("goal.exists".to_owned(), Variant::Bool(false));
                (
                    SubActionTelemetry {
                        args_in: ::std::collections::BTreeMap::new(),
                        produced: ::std::collections::BTreeMap::new(),
                        kind: KIND_ID.to_owned(),
                        started_at,
                        duration_ms: start.elapsed().as_millis() as u64,
                        outcome: SubActionOutcome::Success,
                        index: ctx.index,
                    },
                    Some(output_stack),
                )
            }
            Err(outcome) => (
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
            ),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::helix::HelixError;
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, SELF_USER_ID, TOKEN_SENTINEL, make_ctx,
    };

    fn goal_payload(current: i64, target: i64) -> serde_json::Value {
        serde_json::json!({
            "data": [{
                "id": "goal-1",
                "type": "follower",
                "current_amount": current,
                "target_amount": target,
                "description": "ignored",
            }]
        })
    }

    fn runner_with(
        response: Result<serde_json::Value, HelixError>,
        creds: MockCreds,
    ) -> (Arc<MockTransport>, GetCurrentGoalRunner) {
        let transport = Arc::new(MockTransport::returning(response));
        let runner = GetCurrentGoalRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(creds))),
        );
        (transport, runner)
    }

    #[tokio::test]
    async fn execute_gets_goals_and_pushes_all_outputs() {
        let (transport, runner) =
            runner_with(Ok(goal_payload(80, 100)), MockCreds::with_identity());
        let stack = ArgStack::new();

        let (telemetry, output) = runner.execute(&BTreeMap::new(), &make_ctx(&stack)).await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let request = transport.request(0);
        assert_eq!(request.method, HelixMethod::Get);
        assert_eq!(request.path, "/helix/goals");
        assert!(
            request
                .query
                .contains(&("broadcaster_id".to_owned(), SELF_USER_ID.to_owned()))
        );

        let out = output.unwrap();
        assert_eq!(out.get("goal.exists"), Some(&Variant::Bool(true)));
        assert_eq!(
            out.get("goal.id"),
            Some(&Variant::String("goal-1".to_owned()))
        );
        assert_eq!(
            out.get("goal.type"),
            Some(&Variant::String("follower".to_owned()))
        );
        assert_eq!(out.get("goal.current_amount"), Some(&Variant::Int(80)));
        assert_eq!(out.get("goal.target_amount"), Some(&Variant::Int(100)));
        assert_eq!(out.get("goal.is_achieved"), Some(&Variant::Bool(false)));
    }

    #[tokio::test]
    async fn is_achieved_is_current_greater_or_equal_target() {
        for (label, current, target, expected) in [
            ("below target", 80, 100, false),
            ("exactly at target", 100, 100, true),
            ("above target", 101, 100, true),
        ] {
            let (_transport, runner) = runner_with(
                Ok(goal_payload(current, target)),
                MockCreds::with_identity(),
            );
            let stack = ArgStack::new();

            let (_telemetry, output) = runner.execute(&BTreeMap::new(), &make_ctx(&stack)).await;

            assert_eq!(
                output.unwrap().get("goal.is_achieved"),
                Some(&Variant::Bool(expected)),
                "case: {label}"
            );
        }
    }

    #[tokio::test]
    async fn empty_data_sets_exists_false_and_omits_goal_fields() {
        let (_transport, runner) = runner_with(
            Ok(serde_json::json!({ "data": [] })),
            MockCreds::with_identity(),
        );
        let stack = ArgStack::new();

        let (telemetry, output) = runner.execute(&BTreeMap::new(), &make_ctx(&stack)).await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let out = output.unwrap();
        assert_eq!(out.get("goal.exists"), Some(&Variant::Bool(false)));
        for absent in [
            "goal.id",
            "goal.type",
            "goal.current_amount",
            "goal.target_amount",
            "goal.is_achieved",
        ] {
            assert!(out.get(absent).is_none(), "{absent} must be absent");
        }
    }

    #[tokio::test]
    async fn missing_required_goal_field_maps_to_failed() {
        let bases: [(&str, serde_json::Value); 4] = [
            (
                "id",
                serde_json::json!({ "type": "follower", "current_amount": 1, "target_amount": 2 }),
            ),
            (
                "type",
                serde_json::json!({ "id": "g", "current_amount": 1, "target_amount": 2 }),
            ),
            (
                "current_amount",
                serde_json::json!({ "id": "g", "type": "follower", "target_amount": 2 }),
            ),
            (
                "target_amount",
                serde_json::json!({ "id": "g", "type": "follower", "current_amount": 1 }),
            ),
        ];
        for (missing, goal) in bases {
            let (_transport, runner) = runner_with(
                Ok(serde_json::json!({ "data": [goal] })),
                MockCreds::with_identity(),
            );
            let stack = ArgStack::new();

            let (telemetry, output) = runner.execute(&BTreeMap::new(), &make_ctx(&stack)).await;

            assert!(
                matches!(telemetry.outcome, SubActionOutcome::Failed(_)),
                "missing {missing} must fail"
            );
            assert!(output.is_none(), "missing {missing} must yield no outputs");
        }
    }

    #[tokio::test]
    async fn helix_failure_maps_to_failed_without_leaking_token() {
        let (_transport, runner) = runner_with(
            Err(HelixError::Http {
                status: 401,
                body: "unauthorized".to_owned(),
            }),
            MockCreds::with_identity(),
        );
        let stack = ArgStack::new();

        let (telemetry, output) = runner.execute(&BTreeMap::new(), &make_ctx(&stack)).await;

        assert!(output.is_none());
        assert!(matches!(
            telemetry.outcome,
            SubActionOutcome::Failed(msg) if msg.contains("401") && !msg.contains(TOKEN_SENTINEL)
        ));
    }

    #[tokio::test]
    async fn missing_identity_maps_to_failed_without_calling_helix() {
        let (transport, runner) = runner_with(Ok(goal_payload(80, 100)), MockCreds::empty());
        let stack = ArgStack::new();

        let (telemetry, output) = runner.execute(&BTreeMap::new(), &make_ctx(&stack)).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(output.is_none());
        assert_eq!(
            transport.call_count(),
            0,
            "must not reach Helix without identity"
        );
    }
}

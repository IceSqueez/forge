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

        // GET /helix/goals returns data[0] as the current active goal (one per type at most).
        // Reference: https://dev.twitch.tv/docs/api/reference/#get-creator-goals
        let request =
            HelixRequest::new(HelixMethod::Get, "/helix/goals").query("broadcaster_id", user_id);

        let resp = self
            .transport
            .execute(request)
            .await
            .map_err(|e| SubActionOutcome::Failed(e.to_string()))?;

        let Some(first) = resp["data"].as_array().and_then(|arr| arr.first()) else {
            // Empty data array means no active goal — not an error.
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
                        kind: KIND_ID.to_owned(),
                        started_at,
                        duration_ms: start.elapsed().as_millis() as u64,
                        outcome: SubActionOutcome::Success,
                        index: ctx.index,
                    },
                    Some(output_stack),
                )
            }
            // goal.exists=false on empty data — absence of an active goal is not an error.
            Ok(None) => {
                let output_stack = ctx
                    .arg_stack
                    .clone()
                    .set("goal.exists".to_owned(), Variant::Bool(false));
                (
                    SubActionTelemetry {
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

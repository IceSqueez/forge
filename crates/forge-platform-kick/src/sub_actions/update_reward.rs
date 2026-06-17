use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_platform_core::PlatformError;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use futures::future::BoxFuture;
use time::OffsetDateTime;

use crate::rewards::{KickRewards, UpdateRewardParams};

const KIND_ID: &str = "kick.reward.update";

pub struct UpdateRewardRunner {
    client: Arc<KickRewards>,
    token_source: Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>,
}

impl UpdateRewardRunner {
    pub fn new(
        client: Arc<KickRewards>,
        token_source: Arc<
            dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync,
        >,
    ) -> Self {
        Self {
            client,
            token_source,
        }
    }
}

#[async_trait]
impl SubActionRunner for UpdateRewardRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::ChannelPoints
    }

    fn label(&self) -> &str {
        "Update Channel Reward"
    }

    fn summary(&self) -> &str {
        "Updates an existing Kick channel reward. Requires channel:rewards:write scope and reward ownership."
    }

    fn search_text(&self) -> &str {
        "kick channel reward update edit points"
    }

    fn icon_name(&self) -> &str {
        "edit"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            ("reward_id".to_owned(), Variant::String(String::new())),
            ("title".to_owned(), Variant::String(String::new())),
            ("cost".to_owned(), Variant::String(String::new())),
            ("description".to_owned(), Variant::String(String::new())),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "reward_id",
                label: "Reward ID",
                placeholder: "%reward.id%",
            },
            FormField::Text {
                key: "title",
                label: "New Title (optional)",
                placeholder: "Leave empty to keep current",
            },
            FormField::Text {
                key: "cost",
                label: "New Cost (optional)",
                placeholder: "Leave empty to keep current",
            },
            FormField::Text {
                key: "description",
                label: "New Description (optional)",
                placeholder: "Leave empty to keep current",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        let reward_id = config
            .get("reward_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if reward_id.is_empty() {
            return Err(RegistryError::UnknownKindId(format!(
                "{KIND_ID}: 'reward_id' must be a non-empty string"
            )));
        }

        let title = config
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let cost = config
            .get("cost")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let description = config
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        if title.is_empty() && cost.is_empty() && description.is_empty() {
            return Err(RegistryError::UnknownKindId(format!(
                "{KIND_ID}: at least one of 'title', 'cost', or 'description' must be provided"
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

        let raw_reward_id = config
            .get("reward_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let raw_title = config
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let raw_cost = config
            .get("cost")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let raw_description = config
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        let reward_id = ctx.arg_stack.interpolate(raw_reward_id);
        let title_str = ctx.arg_stack.interpolate(raw_title);
        let cost_str = ctx.arg_stack.interpolate(raw_cost);
        let description_str = ctx.arg_stack.interpolate(raw_description);

        if reward_id.is_empty() {
            let outcome =
                SubActionOutcome::Failed("reward_id is empty after interpolation".to_owned());
            return (
                SubActionTelemetry {
                    kind: KIND_ID.to_owned(),
                    started_at,
                    duration_ms: start.elapsed().as_millis() as u64,
                    outcome,
                    index: ctx.index,
                },
                None,
            );
        }

        let title = if title_str.is_empty() {
            None
        } else {
            Some(title_str)
        };

        let cost = if cost_str.is_empty() {
            None
        } else {
            match cost_str.parse::<u64>() {
                Ok(n) if n >= 1 => Some(n),
                Ok(_) => {
                    let outcome = SubActionOutcome::Failed("cost must be at least 1".to_owned());
                    return (
                        SubActionTelemetry {
                            kind: KIND_ID.to_owned(),
                            started_at,
                            duration_ms: start.elapsed().as_millis() as u64,
                            outcome,
                            index: ctx.index,
                        },
                        None,
                    );
                }
                Err(_) => {
                    let outcome = SubActionOutcome::Failed(format!(
                        "cost '{cost_str}' is not a valid positive integer"
                    ));
                    return (
                        SubActionTelemetry {
                            kind: KIND_ID.to_owned(),
                            started_at,
                            duration_ms: start.elapsed().as_millis() as u64,
                            outcome,
                            index: ctx.index,
                        },
                        None,
                    );
                }
            }
        };

        let description = if description_str.is_empty() {
            None
        } else {
            Some(description_str)
        };

        if title.is_none() && cost.is_none() && description.is_none() {
            let outcome = SubActionOutcome::Failed(
                "all updatable fields are empty after interpolation; nothing to update".to_owned(),
            );
            return (
                SubActionTelemetry {
                    kind: KIND_ID.to_owned(),
                    started_at,
                    duration_ms: start.elapsed().as_millis() as u64,
                    outcome,
                    index: ctx.index,
                },
                None,
            );
        }

        let outcome = match (self.token_source)().await {
            Err(e) => SubActionOutcome::Failed(format!("token error: {e}")),
            Ok(token) => match self
                .client
                .update(
                    &reward_id,
                    UpdateRewardParams {
                        title,
                        cost,
                        description,
                        background_color: None,
                        is_enabled: None,
                        is_paused: None,
                        is_user_input_required: None,
                        should_redemptions_skip_request_queue: None,
                    },
                    &token,
                )
                .await
            {
                Ok(()) => SubActionOutcome::Success,
                Err(e) => SubActionOutcome::Failed(e.to_string()),
            },
        };

        (
            SubActionTelemetry {
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

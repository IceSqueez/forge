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

use crate::rewards::{CreateRewardParams, KickRewards};

const KIND_ID: &str = "kick.reward.create";

pub struct CreateRewardRunner {
    client: Arc<KickRewards>,
    token_source: Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>,
}

impl CreateRewardRunner {
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
impl SubActionRunner for CreateRewardRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::ChannelPoints
    }

    fn label(&self) -> &str {
        "Create Channel Reward"
    }

    fn summary(&self) -> &str {
        "Creates a new channel point reward on Kick. Requires channel:rewards:write scope."
    }

    fn search_text(&self) -> &str {
        "kick channel reward create points redeem"
    }

    fn icon_name(&self) -> &str {
        "gift"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            ("title".to_owned(), Variant::String(String::new())),
            ("cost".to_owned(), Variant::String(String::new())),
            ("description".to_owned(), Variant::String(String::new())),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "title",
                label: "Reward Title",
                placeholder: "e.g. Hydrate",
            },
            FormField::Text {
                key: "cost",
                label: "Cost (channel points)",
                placeholder: "e.g. 500",
            },
            FormField::Text {
                key: "description",
                label: "Description (optional)",
                placeholder: "Leave empty to omit",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        let title = config
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if title.is_empty() {
            return Err(RegistryError::UnknownKindId(format!(
                "{KIND_ID}: 'title' must be a non-empty string"
            )));
        }

        let cost_raw = config
            .get("cost")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if cost_raw.is_empty() {
            return Err(RegistryError::UnknownKindId(format!(
                "{KIND_ID}: 'cost' must be provided"
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

        let title = ctx.arg_stack.interpolate(raw_title);
        let cost_str = ctx.arg_stack.interpolate(raw_cost);
        let description_str = ctx.arg_stack.interpolate(raw_description);

        if title.is_empty() {
            let outcome = SubActionOutcome::Failed("title is empty after interpolation".to_owned());
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

        let cost = match cost_str.parse::<u64>() {
            Ok(n) if n >= 1 => n,
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
        };

        let description = if description_str.is_empty() {
            None
        } else {
            Some(description_str)
        };

        let outcome = match (self.token_source)().await {
            Err(e) => SubActionOutcome::Failed(format!("token error: {e}")),
            Ok(token) => match self
                .client
                .create(
                    CreateRewardParams {
                        title,
                        cost,
                        description,
                        background_color: None,
                        is_enabled: None,
                        is_user_input_required: None,
                        should_redemptions_skip_request_queue: None,
                    },
                    &token,
                )
                .await
            {
                Ok(_created_id) => SubActionOutcome::Success,
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

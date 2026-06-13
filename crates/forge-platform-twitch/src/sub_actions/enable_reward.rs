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

const KIND_ID: &str = "twitch.channel_points.enable_reward";

pub struct EnableRewardRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl EnableRewardRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }
}

/// PATCH /helix/channel_points/custom_rewards with a single boolean field.
/// Requires channel:manage:redemptions scope. Only the supplied body key is
/// changed; Twitch leaves all other reward fields as-is.
pub(crate) async fn patch_reward_bool(
    transport: &Arc<dyn HelixTransport>,
    identity: &Arc<SelfIdentity>,
    reward_id: &str,
    body_key: &str,
    value: bool,
) -> SubActionOutcome {
    let user_id = match identity.user_id().await {
        Ok(id) => id,
        Err(e) => return SubActionOutcome::Failed(e.to_string()),
    };

    let mut body = serde_json::Map::new();
    body.insert(body_key.to_owned(), value.into());

    let request = HelixRequest::new(HelixMethod::Patch, "/helix/channel_points/custom_rewards")
        .query("broadcaster_id", user_id)
        .query("id", reward_id.to_owned())
        .body(serde_json::Value::Object(body));

    match transport.execute(request).await {
        Ok(_) => SubActionOutcome::Success,
        Err(e) => SubActionOutcome::Failed(e.to_string()),
    }
}

pub(crate) fn default_config() -> SubActionConfig {
    BTreeMap::from([(
        "reward_id".to_owned(),
        Variant::String("%reward.id%".to_owned()),
    )])
}

pub(crate) fn config_fields() -> Vec<FormField> {
    vec![FormField::Text {
        key: "reward_id",
        label: "Reward ID",
        placeholder: "%reward.id%",
    }]
}

pub(crate) fn validate_reward_id(kind_id: &str, config: &SubActionConfig) -> Result<(), RegistryError> {
    match config.get("reward_id") {
        Some(Variant::String(s)) if !s.is_empty() => Ok(()),
        _ => Err(RegistryError::UnknownKindId(format!(
            "{kind_id}: 'reward_id' is required"
        ))),
    }
}

pub(crate) async fn execute_bool_runner(
    transport: &Arc<dyn HelixTransport>,
    identity: &Arc<SelfIdentity>,
    kind_id: &str,
    body_key: &str,
    value: bool,
    config: &SubActionConfig,
    ctx: &RunContext<'_>,
) -> (SubActionTelemetry, Option<ArgStack>) {
    let started_at = OffsetDateTime::now_utc();
    let start = Instant::now();

    let reward_id_template = config
        .get("reward_id")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let reward_id = ctx.arg_stack.interpolate(reward_id_template);

    let outcome = if reward_id.is_empty() {
        SubActionOutcome::Failed("reward_id is required".to_owned())
    } else {
        patch_reward_bool(transport, identity, &reward_id, body_key, value).await
    };

    (
        SubActionTelemetry {
            kind: kind_id.to_owned(),
            started_at,
            duration_ms: start.elapsed().as_millis() as u64,
            outcome,
            index: ctx.index,
        },
        None,
    )
}

#[async_trait]
impl SubActionRunner for EnableRewardRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Twitch
    }

    fn label(&self) -> &str {
        "Enable Channel Point Reward"
    }

    fn summary(&self) -> &str {
        "Enables a custom channel point reward so viewers can redeem it."
    }

    fn search_text(&self) -> &str {
        "twitch channel points custom reward enable on redemption"
    }

    fn icon_name(&self) -> &str {
        "star"
    }

    fn default_config(&self) -> SubActionConfig {
        default_config()
    }

    fn config_fields(&self) -> Vec<FormField> {
        config_fields()
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        validate_reward_id(KIND_ID, config)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        execute_bool_runner(
            &self.transport,
            &self.identity,
            KIND_ID,
            "is_enabled",
            true,
            config,
            ctx,
        )
        .await
    }
}

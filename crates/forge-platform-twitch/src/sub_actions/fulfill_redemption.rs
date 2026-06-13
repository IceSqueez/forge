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

const KIND_ID: &str = "twitch.channel_points.fulfill_redemption";

pub struct FulfillRedemptionRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl FulfillRedemptionRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }
}

pub(crate) fn redemption_default_config() -> SubActionConfig {
    BTreeMap::from([
        (
            "redemption_id".to_owned(),
            Variant::String("%redemption.id%".to_owned()),
        ),
        (
            "reward_id".to_owned(),
            Variant::String("%reward.id%".to_owned()),
        ),
    ])
}

pub(crate) fn redemption_config_fields() -> Vec<FormField> {
    vec![
        FormField::Text {
            key: "redemption_id",
            label: "Redemption ID",
            placeholder: "%redemption.id%",
        },
        FormField::Text {
            key: "reward_id",
            label: "Reward ID",
            placeholder: "%reward.id%",
        },
    ]
}

pub(crate) fn validate_redemption_config(
    kind_id: &str,
    config: &SubActionConfig,
) -> Result<(), RegistryError> {
    match config.get("redemption_id") {
        Some(Variant::String(s)) if !s.is_empty() => {}
        _ => {
            return Err(RegistryError::UnknownKindId(format!(
                "{kind_id}: 'redemption_id' is required"
            )))
        }
    }
    match config.get("reward_id") {
        Some(Variant::String(s)) if !s.is_empty() => Ok(()),
        _ => Err(RegistryError::UnknownKindId(format!(
            "{kind_id}: 'reward_id' is required"
        ))),
    }
}

/// PATCH /helix/channel_points/custom_rewards/redemptions with three query params and
/// status in the body. Twitch requires broadcaster_id + reward_id + id as query params
/// (not body) with the new status as the sole body field.
pub(crate) async fn patch_redemption_status(
    transport: &Arc<dyn HelixTransport>,
    identity: &Arc<SelfIdentity>,
    kind_id: &str,
    redemption_id: &str,
    reward_id: &str,
    status: &str,
) -> SubActionOutcome {
    let user_id = match identity.user_id().await {
        Ok(id) => id,
        Err(e) => return SubActionOutcome::Failed(e.to_string()),
    };

    let mut body = serde_json::Map::new();
    body.insert("status".to_owned(), status.into());

    let request = HelixRequest::new(
        HelixMethod::Patch,
        "/helix/channel_points/custom_rewards/redemptions",
    )
    .query("broadcaster_id", user_id)
    .query("reward_id", reward_id.to_owned())
    .query("id", redemption_id.to_owned())
    .body(serde_json::Value::Object(body));

    match transport.execute(request).await {
        Ok(_) => SubActionOutcome::Success,
        Err(e) => SubActionOutcome::Failed(format!("{kind_id}: {e}")),
    }
}

pub(crate) async fn execute_redemption_runner(
    transport: &Arc<dyn HelixTransport>,
    identity: &Arc<SelfIdentity>,
    kind_id: &str,
    status: &str,
    config: &SubActionConfig,
    ctx: &RunContext<'_>,
) -> (SubActionTelemetry, Option<ArgStack>) {
    let started_at = OffsetDateTime::now_utc();
    let start = Instant::now();

    let redemption_id_template = config
        .get("redemption_id")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let redemption_id = ctx.arg_stack.interpolate(redemption_id_template);

    let reward_id_template = config
        .get("reward_id")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let reward_id = ctx.arg_stack.interpolate(reward_id_template);

    let outcome = if redemption_id.is_empty() {
        SubActionOutcome::Failed("redemption_id is required".to_owned())
    } else if reward_id.is_empty() {
        SubActionOutcome::Failed("reward_id is required".to_owned())
    } else {
        patch_redemption_status(transport, identity, kind_id, &redemption_id, &reward_id, status)
            .await
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
impl SubActionRunner for FulfillRedemptionRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Twitch
    }

    fn label(&self) -> &str {
        "Fulfill Channel Point Redemption"
    }

    fn summary(&self) -> &str {
        "Marks a channel point redemption as fulfilled."
    }

    fn search_text(&self) -> &str {
        "twitch channel points redemption fulfill complete done"
    }

    fn icon_name(&self) -> &str {
        "check"
    }

    fn default_config(&self) -> SubActionConfig {
        redemption_default_config()
    }

    fn config_fields(&self) -> Vec<FormField> {
        redemption_config_fields()
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        validate_redemption_config(KIND_ID, config)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        execute_redemption_runner(
            &self.transport,
            &self.identity,
            KIND_ID,
            "FULFILLED",
            config,
            ctx,
        )
        .await
    }
}

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

const KIND_ID: &str = "twitch.automod.approve_message";

pub struct ApproveAutomodMessageRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl ApproveAutomodMessageRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }
}

pub(crate) fn automod_default_config() -> SubActionConfig {
    BTreeMap::from([(
        "message_id".to_owned(),
        Variant::String("%automod.message_id%".to_owned()),
    )])
}

pub(crate) fn automod_config_fields() -> Vec<FormField> {
    vec![FormField::Text {
        key: "message_id",
        label: "Message ID",
        placeholder: "%automod.message_id%",
    }]
}

pub(crate) fn validate_automod_config(
    kind_id: &str,
    config: &SubActionConfig,
) -> Result<(), RegistryError> {
    match config.get("message_id") {
        Some(Variant::String(s)) if !s.is_empty() => Ok(()),
        _ => Err(RegistryError::UnknownKindId(format!(
            "{kind_id}: 'message_id' is required"
        ))),
    }
}

/// POST /helix/moderation/automod/message to allow or deny a held AutoMod message.
///
/// `user_id` is the MODERATOR's own id (self), not the sender's id — the Twitch API
/// uses it to verify the caller has moderator rights on the channel.
/// `action` must be uppercase "ALLOW" or "DENY" (lowercase is rejected by Twitch).
/// All three fields go in the JSON body, not as query params.
///
/// Reference: https://dev.twitch.tv/docs/api/reference/#manage-held-automod-messages
pub(crate) async fn manage_automod_message(
    transport: &Arc<dyn HelixTransport>,
    identity: &Arc<SelfIdentity>,
    kind_id: &str,
    message_id: &str,
    action: &str,
) -> SubActionOutcome {
    let user_id = match identity.user_id().await {
        Ok(id) => id,
        Err(e) => return SubActionOutcome::Failed(e.to_string()),
    };

    let body = serde_json::json!({
        "user_id": user_id,
        "msg_id": message_id,
        "action": action,
    });

    let request =
        HelixRequest::new(HelixMethod::Post, "/helix/moderation/automod/message").body(body);

    match transport.execute(request).await {
        Ok(_) => SubActionOutcome::Success,
        Err(e) => SubActionOutcome::Failed(format!("{kind_id}: {e}")),
    }
}

pub(crate) async fn execute_automod_runner(
    transport: &Arc<dyn HelixTransport>,
    identity: &Arc<SelfIdentity>,
    kind_id: &str,
    action: &str,
    config: &SubActionConfig,
    ctx: &RunContext<'_>,
) -> (SubActionTelemetry, Option<ArgStack>) {
    let started_at = OffsetDateTime::now_utc();
    let start = Instant::now();

    let message_id_template = config
        .get("message_id")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let message_id = ctx.arg_stack.interpolate(message_id_template);

    let outcome = if message_id.is_empty() {
        SubActionOutcome::Failed("message_id is required".to_owned())
    } else {
        manage_automod_message(transport, identity, kind_id, &message_id, action).await
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
impl SubActionRunner for ApproveAutomodMessageRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Twitch
    }

    fn label(&self) -> &str {
        "Approve AutoMod Message"
    }

    fn summary(&self) -> &str {
        "Allows a message held by AutoMod to appear in chat."
    }

    fn search_text(&self) -> &str {
        "twitch automod approve allow message held moderation"
    }

    fn icon_name(&self) -> &str {
        "check"
    }

    fn default_config(&self) -> SubActionConfig {
        automod_default_config()
    }

    fn config_fields(&self) -> Vec<FormField> {
        automod_config_fields()
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        validate_automod_config(KIND_ID, config)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        execute_automod_runner(
            &self.transport,
            &self.identity,
            KIND_ID,
            "ALLOW",
            config,
            ctx,
        )
        .await
    }
}

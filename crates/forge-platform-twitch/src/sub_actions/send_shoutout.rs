use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use super::identity::{SelfIdentity, resolve_user_id};
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};

const KIND_ID: &str = "twitch.channel.send_shoutout";

pub struct SendShoutoutRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl SendShoutoutRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn shoutout(&self, to_broadcaster_login: &str) -> SubActionOutcome {
        if to_broadcaster_login.is_empty() {
            return SubActionOutcome::Failed(
                "to_broadcaster_login is empty after interpolation".to_owned(),
            );
        }
        let self_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(e.to_string()),
        };
        let to_broadcaster_id =
            match resolve_user_id(self.transport.as_ref(), to_broadcaster_login).await {
                Ok(id) => id,
                Err(e) => return SubActionOutcome::Failed(e.to_string()),
            };
        // Shoutout requires three identity params: from (self), to (resolved), and moderator
        // (self again). Twitch validates that moderator_id has mod privileges in from_broadcaster's
        // channel; using self satisfies this for the broadcaster's own channel.
        let request = HelixRequest::new(HelixMethod::Post, "/helix/chat/shoutouts")
            .query("from_broadcaster_id", self_id.clone())
            .query("to_broadcaster_id", to_broadcaster_id)
            .query("moderator_id", self_id);
        match self.transport.execute(request).await {
            Ok(_) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        }
    }
}

#[async_trait]
impl SubActionRunner for SendShoutoutRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Twitch
    }

    fn label(&self) -> &str {
        "Send Shoutout"
    }

    fn summary(&self) -> &str {
        "Sends a shoutout to another broadcaster in the channel chat."
    }

    fn search_text(&self) -> &str {
        "twitch shoutout so channel raid highlight streamer"
    }

    fn icon_name(&self) -> &str {
        "shoutout"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([(
            "to_broadcaster_login".to_owned(),
            Variant::String(String::new()),
        )])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Text {
            key: "to_broadcaster_login",
            label: "Target Channel",
            placeholder: "%user_login%",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("to_broadcaster_login") {
            Some(Variant::String(s)) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::UnknownKindId(format!(
                "{KIND_ID}: 'to_broadcaster_login' must be a non-empty string"
            ))),
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let login_template = config
            .get("to_broadcaster_login")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let to_broadcaster_login = ctx.arg_stack.interpolate(login_template);

        let outcome = self.shoutout(&to_broadcaster_login).await;

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

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

const KIND_ID: &str = "twitch.channel.start_raid";

pub struct StartRaidRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl StartRaidRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn start_raid(&self, to_broadcaster_login: &str) -> SubActionOutcome {
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
        let request = HelixRequest::new(HelixMethod::Post, "/helix/raids")
            .query("from_broadcaster_id", self_id)
            .query("to_broadcaster_id", to_broadcaster_id);
        match self.transport.execute(request).await {
            Ok(_) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        }
    }
}

#[async_trait]
impl SubActionRunner for StartRaidRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Twitch
    }

    fn label(&self) -> &str {
        "Start Raid"
    }

    fn summary(&self) -> &str {
        "Starts a raid to another broadcaster's channel."
    }

    fn search_text(&self) -> &str {
        "twitch raid start send viewers channel"
    }

    fn icon_name(&self) -> &str {
        "raid"
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

        let outcome = self.start_raid(&to_broadcaster_login).await;

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

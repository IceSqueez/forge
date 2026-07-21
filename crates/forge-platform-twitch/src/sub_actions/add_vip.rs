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

const KIND_ID: &str = "twitch.moderation.add_vip";

pub struct AddVipRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl AddVipRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn apply(&self, target_login: &str) -> SubActionOutcome {
        if target_login.is_empty() {
            return SubActionOutcome::Failed(
                "target_user_login is empty after interpolation".to_owned(),
            );
        }
        let broadcaster_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(e.to_string()),
        };
        let user_id = match resolve_user_id(self.transport.as_ref(), target_login).await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(e.to_string()),
        };
        let request = HelixRequest::new(HelixMethod::Post, "/helix/channels/vips")
            .query("broadcaster_id", broadcaster_id)
            .query("user_id", user_id);
        SubActionOutcome::from_result(&self.transport.execute(request).await)
    }
}

#[async_trait]
impl SubActionRunner for AddVipRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Moderation
    }

    fn label(&self) -> &str {
        "Add VIP"
    }

    fn summary(&self) -> &str {
        "Grants VIP status to a user in the channel."
    }

    fn search_text(&self) -> &str {
        "twitch moderation add vip grant role channel"
    }

    fn icon_name(&self) -> &str {
        "star-plus"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([(
            "target_user_login".to_owned(),
            Variant::String(String::new()),
        )])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Text {
            key: "target_user_login",
            label: "Target Username",
            placeholder: "%user_login%",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("target_user_login") {
            Some(Variant::String(s)) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::InvalidConfig(format!(
                "{KIND_ID}: 'target_user_login' must be a non-empty string"
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

        let login_template = config.str("target_user_login").unwrap_or_default();
        let target_login = ctx.arg_stack.interpolate(login_template);

        let outcome = self.apply(&target_login).await;

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

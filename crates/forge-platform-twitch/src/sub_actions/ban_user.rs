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

const KIND_ID: &str = "twitch.moderation.ban_user";

pub struct BanUserRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl BanUserRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn ban(&self, target_login: &str, reason: &str) -> SubActionOutcome {
        if target_login.is_empty() {
            return SubActionOutcome::Failed(
                "target_user_login is empty after interpolation".to_owned(),
            );
        }
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(e.to_string()),
        };
        let target_user_id = match resolve_user_id(self.transport.as_ref(), target_login).await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(e.to_string()),
        };
        let mut data = serde_json::json!({ "user_id": target_user_id });
        if !reason.is_empty() {
            data["reason"] = serde_json::Value::String(reason.to_owned());
        }
        let request = HelixRequest::new(HelixMethod::Post, "/helix/moderation/bans")
            .query("broadcaster_id", user_id.clone())
            .query("moderator_id", user_id)
            .body(serde_json::json!({ "data": data }));
        match self.transport.execute(request).await {
            Ok(_) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        }
    }
}

#[async_trait]
impl SubActionRunner for BanUserRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Moderation
    }

    fn label(&self) -> &str {
        "Ban User"
    }

    fn summary(&self) -> &str {
        "Permanently bans a user from the channel."
    }

    fn search_text(&self) -> &str {
        "twitch moderation ban user permanent remove"
    }

    fn icon_name(&self) -> &str {
        "ban"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            (
                "target_user_login".to_owned(),
                Variant::String(String::new()),
            ),
            ("reason".to_owned(), Variant::String(String::new())),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "target_user_login",
                label: "Target Username",
                placeholder: "%user_login%",
            },
            FormField::Text {
                key: "reason",
                label: "Reason (optional, max 500 chars)",
                placeholder: "",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("target_user_login") {
            Some(Variant::String(s)) if !s.is_empty() => {}
            _ => {
                return Err(RegistryError::UnknownKindId(format!(
                    "{KIND_ID}: 'target_user_login' must be a non-empty string"
                )));
            }
        }
        if let Some(Variant::String(r)) = config.get("reason")
            && r.chars().count() > 500
        {
            return Err(RegistryError::UnknownKindId(format!(
                "{KIND_ID}: 'reason' must not exceed 500 characters"
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

        let login_template = config
            .get("target_user_login")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let target_login = ctx.arg_stack.interpolate(login_template);
        let reason = config
            .get("reason")
            .and_then(|v| v.as_str())
            .map(|s| ctx.arg_stack.interpolate(s))
            .unwrap_or_default();

        let outcome = self.ban(&target_login, &reason).await;

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

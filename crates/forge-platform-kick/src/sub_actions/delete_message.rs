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

use crate::send::KickSendChat;

const KIND_ID: &str = "kick.chat.delete_message";

pub struct DeleteMessageRunner {
    client: Arc<KickSendChat>,
    token_source: Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>,
}

impl DeleteMessageRunner {
    pub fn new(
        client: Arc<KickSendChat>,
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
impl SubActionRunner for DeleteMessageRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Moderation
    }

    fn label(&self) -> &str {
        "Delete Message"
    }

    fn summary(&self) -> &str {
        "Removes a chat message from the Kick channel by its message id. Requires moderation:chat_message:manage scope."
    }

    fn search_text(&self) -> &str {
        "kick chat delete remove message moderation"
    }

    fn icon_name(&self) -> &str {
        "delete"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([(
            "message_id".to_owned(),
            Variant::String("%chat.message_id%".to_owned()),
        )])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Text {
            key: "message_id",
            label: "Message ID",
            placeholder: "%chat.message_id%",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("message_id") {
            Some(Variant::String(s)) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::UnknownKindId(format!(
                "{KIND_ID}: 'message_id' must be a non-empty string"
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

        let template = config
            .get("message_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let message_id = ctx.arg_stack.interpolate(template);

        let outcome = if message_id.is_empty() {
            SubActionOutcome::Failed("message_id is empty after interpolation".to_owned())
        } else {
            match (self.token_source)().await {
                Err(e) => SubActionOutcome::Failed(format!("token error: {e}")),
                Ok(token) => match self.client.delete(&message_id, &token).await {
                    Ok(()) => SubActionOutcome::Success,
                    Err(e) => SubActionOutcome::Failed(e.to_string()),
                },
            }
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

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

const KIND_ID: &str = "kick.chat.send_message";

pub struct SendMessageRunner {
    client: Arc<KickSendChat>,
    token_source: Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>,
    broadcaster_user_id: u64,
}

impl SendMessageRunner {
    pub fn new(
        client: Arc<KickSendChat>,
        token_source: Arc<
            dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync,
        >,
        broadcaster_user_id: u64,
    ) -> Self {
        Self {
            client,
            token_source,
            broadcaster_user_id,
        }
    }
}

#[async_trait]
impl SubActionRunner for SendMessageRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Chat
    }

    fn label(&self) -> &str {
        "Send Message"
    }

    fn summary(&self) -> &str {
        "Posts a message to the Kick channel chat."
    }

    fn search_text(&self) -> &str {
        "kick chat message send say post"
    }

    fn icon_name(&self) -> &str {
        "chat"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([("message".to_owned(), Variant::String(String::new()))])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::TextArea {
            key: "message",
            label: "Message",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("message") {
            Some(Variant::String(s)) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::UnknownKindId(format!(
                "{KIND_ID}: 'message' must be a non-empty string"
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
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let message = ctx.arg_stack.interpolate(template);

        let outcome = if message.is_empty() {
            SubActionOutcome::Failed("message is empty after interpolation".to_owned())
        } else {
            match (self.token_source)().await {
                Err(e) => SubActionOutcome::Failed(format!("token error: {e}")),
                Ok(token) => match self
                    .client
                    .send(&message, &token, self.broadcaster_user_id)
                    .await
                {
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

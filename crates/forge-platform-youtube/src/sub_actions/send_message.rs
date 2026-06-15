use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::send_chat::YoutubeSendChat;

const KIND_ID: &str = "youtube.chat.send_message";

pub struct SendMessageRunner {
    sender: Arc<YoutubeSendChat>,
}

impl SendMessageRunner {
    pub fn new(sender: Arc<YoutubeSendChat>) -> Self {
        Self { sender }
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
        "Posts a message in the active YouTube live chat."
    }

    fn search_text(&self) -> &str {
        "youtube chat message send say post live"
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
            match self.sender.send(&message).await {
                Ok(()) => SubActionOutcome::Success,
                Err(e) => SubActionOutcome::Failed(e.to_string()),
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

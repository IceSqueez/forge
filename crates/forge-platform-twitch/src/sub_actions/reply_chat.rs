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

const KIND_ID: &str = "twitch.chat.reply";
/// Twitch counts characters, not bytes; multibyte messages must pass at 500 chars.
const MAX_MESSAGE_CHARS: usize = 500;
const DEFAULT_PARENT_TEMPLATE: &str = "%chat.message_id%";

pub struct ReplyChatRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl ReplyChatRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn reply(&self, message: &str, parent_message_id: &str) -> SubActionOutcome {
        if message.is_empty() {
            return SubActionOutcome::Failed("message is empty after interpolation".to_owned());
        }
        if message.chars().count() > MAX_MESSAGE_CHARS {
            return SubActionOutcome::Failed("message exceeds 500-character limit".to_owned());
        }
        if parent_message_id.is_empty() {
            return SubActionOutcome::Failed(
                "parent_message_id is empty after interpolation".to_owned(),
            );
        }
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(e.to_string()),
        };
        let request =
            HelixRequest::new(HelixMethod::Post, "/helix/chat/messages").body(serde_json::json!({
                "broadcaster_id": user_id,
                "sender_id": user_id,
                "message": message,
                "reply_parent_message_id": parent_message_id,
            }));
        match self.transport.execute(request).await {
            Ok(_) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        }
    }
}

#[async_trait]
impl SubActionRunner for ReplyChatRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Chat
    }

    fn label(&self) -> &str {
        "Reply to Message"
    }

    fn summary(&self) -> &str {
        "Sends a reply to a specific chat message."
    }

    fn search_text(&self) -> &str {
        "twitch chat reply respond message thread"
    }

    fn icon_name(&self) -> &str {
        "reply"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            ("message".to_owned(), Variant::String(String::new())),
            (
                "parent_message_id".to_owned(),
                Variant::String(DEFAULT_PARENT_TEMPLATE.to_owned()),
            ),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::TextArea {
                key: "message",
                label: "Message",
            },
            FormField::Text {
                key: "parent_message_id",
                label: "Parent Message ID",
                placeholder: "%chat.message_id%",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("message") {
            Some(Variant::String(s)) if !s.is_empty() => {}
            _ => {
                return Err(RegistryError::UnknownKindId(format!(
                    "{KIND_ID}: 'message' must be a non-empty string"
                )));
            }
        }
        match config.get("parent_message_id") {
            Some(Variant::String(s)) if !s.is_empty() => {}
            _ => {
                return Err(RegistryError::UnknownKindId(format!(
                    "{KIND_ID}: 'parent_message_id' must be a non-empty string"
                )));
            }
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

        let msg_template = config
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let message = ctx.arg_stack.interpolate(msg_template);

        let parent_template = config
            .get("parent_message_id")
            .and_then(|v| v.as_str())
            .unwrap_or(DEFAULT_PARENT_TEMPLATE);
        let parent_message_id = ctx.arg_stack.interpolate(parent_template);

        let outcome = self.reply(&message, &parent_message_id).await;

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

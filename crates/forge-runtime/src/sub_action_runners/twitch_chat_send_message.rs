use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{Event, EventSource};
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_storage::GlobalsRepo;
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

pub struct TwitchChatSendMessageRunner {
    globals: Arc<dyn GlobalsRepo>,
}

impl TwitchChatSendMessageRunner {
    pub fn new(globals: Arc<dyn GlobalsRepo>) -> Self {
        Self { globals }
    }
}

#[async_trait]
impl SubActionRunner for TwitchChatSendMessageRunner {
    fn id(&self) -> &str {
        "twitch.chat.send_message"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Chat
    }

    fn label(&self) -> &str {
        "Send Chat Message"
    }

    fn summary(&self) -> &str {
        "Send a message to a platform chat channel"
    }

    fn search_text(&self) -> &str {
        "send chat message twitch write post"
    }

    fn icon_name(&self) -> &str {
        "message-circle"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("message".to_owned(), Variant::String(String::new()));
        cfg.insert("target".to_owned(), Variant::String("twitch".to_owned()));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::TextArea {
                key: "message",
                label: "Message",
            },
            FormField::Text {
                key: "target",
                label: "Target Platform",
                placeholder: "twitch",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("message").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::UnknownKindId(
                "twitch.chat.send_message: message is required".to_owned(),
            )),
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();

        let message_template = config
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let target_template = config
            .get("target")
            .and_then(|v| v.as_str())
            .unwrap_or("twitch");

        let message = super::interpolate::interpolate_with_globals(
            message_template,
            ctx.arg_stack,
            self.globals.as_ref(),
        )
        .await;
        let target = super::interpolate::interpolate_with_globals(
            target_template,
            ctx.arg_stack,
            self.globals.as_ref(),
        )
        .await;

        ctx.publisher.publish(Event::caused_by(
            EventSource::Core,
            "chat.send.request",
            serde_json::json!({
                "target": target,
                "message": message,
            }),
            ctx.parent_event_id,
        ));

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "twitch.chat.send_message".to_owned(),
                started_at,
                duration_ms,
                outcome: SubActionOutcome::Success,
            },
            None,
        )
    }
}

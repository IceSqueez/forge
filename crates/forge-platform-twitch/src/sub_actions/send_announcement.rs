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

const KIND_ID: &str = "twitch.chat.send_announcement";
/// Twitch counts characters, not bytes; multibyte messages must pass at 500 chars.
const MAX_MESSAGE_CHARS: usize = 500;
const COLORS: &[&str] = &["primary", "blue", "green", "orange", "purple"];
const DEFAULT_COLOR: &str = "primary";

pub struct SendAnnouncementRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl SendAnnouncementRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn send(&self, message: &str, color: &str) -> SubActionOutcome {
        if message.is_empty() {
            return SubActionOutcome::Failed("message is empty after interpolation".to_owned());
        }
        if message.chars().count() > MAX_MESSAGE_CHARS {
            return SubActionOutcome::Failed("message exceeds 500-character limit".to_owned());
        }
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(e.to_string()),
        };
        let request = HelixRequest::new(HelixMethod::Post, "/helix/chat/announcements")
            .query("broadcaster_id", user_id.clone())
            .query("moderator_id", user_id)
            .body(serde_json::json!({ "message": message, "color": color }));
        match self.transport.execute(request).await {
            Ok(_) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        }
    }
}

#[async_trait]
impl SubActionRunner for SendAnnouncementRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Chat
    }

    fn label(&self) -> &str {
        "Send Announcement"
    }

    fn summary(&self) -> &str {
        "Posts a highlighted announcement in the Twitch chat."
    }

    fn search_text(&self) -> &str {
        "twitch chat announcement announce highlight banner notice"
    }

    fn icon_name(&self) -> &str {
        "speakerphone"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            ("message".to_owned(), Variant::String(String::new())),
            (
                "color".to_owned(),
                Variant::String(DEFAULT_COLOR.to_owned()),
            ),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::TextArea {
                key: "message",
                label: "Message",
            },
            FormField::Select {
                key: "color",
                label: "Color",
                options: COLORS,
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
        match config.get("color") {
            None => {}
            Some(Variant::String(c)) if COLORS.contains(&c.as_str()) => {}
            _ => {
                return Err(RegistryError::UnknownKindId(format!(
                    "{KIND_ID}: 'color' must be one of blue, green, orange, purple, primary"
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

        let template = config
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let message = ctx.arg_stack.interpolate(template);
        let color = config
            .get("color")
            .and_then(|v| v.as_str())
            .filter(|c| COLORS.contains(c))
            .unwrap_or(DEFAULT_COLOR);

        let outcome = self.send(&message, color).await;

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

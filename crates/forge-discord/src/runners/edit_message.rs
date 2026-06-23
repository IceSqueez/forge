use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::embed::DiscordEmbed;
use crate::sink::DiscordSink;

pub struct EditMessageRunner {
    sink: Arc<dyn DiscordSink>,
}

impl EditMessageRunner {
    pub fn new(sink: Arc<dyn DiscordSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for EditMessageRunner {
    fn id(&self) -> &str {
        "discord.webhook.update_message"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Discord
    }

    fn label(&self) -> &str {
        "Edit Message"
    }

    fn summary(&self) -> &str {
        "Edits a previously posted Discord webhook message."
    }

    fn search_text(&self) -> &str {
        "discord webhook edit message update"
    }

    fn icon_name(&self) -> &str {
        "pencil"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            ("webhook_name".to_owned(), Variant::String(String::new())),
            ("message_id".to_owned(), Variant::String(String::new())),
            ("content".to_owned(), Variant::String(String::new())),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "webhook_name",
                label: "Webhook",
                placeholder: "alerts",
            },
            FormField::Text {
                key: "message_id",
                label: "Message ID",
                placeholder: "123456789012345678",
            },
            FormField::TextArea {
                key: "content",
                label: "New Content",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        let ok = matches!(config.get("webhook_name"), Some(Variant::String(_)))
            && matches!(config.get("message_id"), Some(Variant::String(_)));
        if ok {
            Ok(())
        } else {
            Err(RegistryError::UnknownKindId(
                "discord.webhook.update_message: 'webhook_name' and 'message_id' must be strings"
                    .to_owned(),
            ))
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let webhook_name = ctx.arg_stack.interpolate(
            config
                .get("webhook_name")
                .and_then(|v| {
                    if let Variant::String(s) = v {
                        Some(s.as_str())
                    } else {
                        None
                    }
                })
                .unwrap_or_default(),
        );
        let message_id = ctx.arg_stack.interpolate(
            config
                .get("message_id")
                .and_then(|v| {
                    if let Variant::String(s) = v {
                        Some(s.as_str())
                    } else {
                        None
                    }
                })
                .unwrap_or_default(),
        );
        let content = config
            .get("content")
            .and_then(|v| {
                if let Variant::String(s) = v {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .map(|raw| ctx.arg_stack.interpolate(raw))
            .filter(|s| !s.is_empty());

        let embed_title = config
            .get("embed_title")
            .and_then(|v| {
                if let Variant::String(s) = v {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .map(|raw| ctx.arg_stack.interpolate(raw))
            .filter(|s| !s.is_empty());

        let has_any = content.is_some() || embed_title.is_some();
        if !has_any {
            return (
                SubActionTelemetry {
                    kind: "discord.webhook.update_message".to_owned(),
                    started_at,
                    duration_ms: start.elapsed().as_millis() as u64,
                    outcome: SubActionOutcome::Failed("edit has no content or embed".to_owned()),
                    index: ctx.index,
                },
                None,
            );
        }

        let embed = embed_title.map(|t| DiscordEmbed {
            title: Some(t),
            ..Default::default()
        });

        let outcome = match self
            .sink
            .edit_message(&webhook_name, &message_id, content.as_deref(), embed)
            .await
        {
            Ok(()) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        };

        (
            SubActionTelemetry {
                kind: "discord.webhook.update_message".to_owned(),
                started_at,
                duration_ms: start.elapsed().as_millis() as u64,
                outcome,
                index: ctx.index,
            },
            None,
        )
    }
}

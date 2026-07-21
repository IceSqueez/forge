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

use crate::sink::DiscordSink;

pub struct PostTextRunner {
    sink: Arc<dyn DiscordSink>,
}

impl PostTextRunner {
    pub fn new(sink: Arc<dyn DiscordSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for PostTextRunner {
    fn id(&self) -> &str {
        "discord.webhook.send_message"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Discord
    }

    fn label(&self) -> &str {
        "Post Text"
    }

    fn summary(&self) -> &str {
        "Posts a text message to a Discord webhook."
    }

    fn search_text(&self) -> &str {
        "discord webhook post message text send"
    }

    fn icon_name(&self) -> &str {
        "message"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            ("webhook_name".to_owned(), Variant::String(String::new())),
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
            FormField::TextArea {
                key: "content",
                label: "Message",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        let ok = matches!(config.get("webhook_name"), Some(Variant::String(_)))
            && matches!(config.get("content"), Some(Variant::String(_)));
        if ok {
            Ok(())
        } else {
            Err(RegistryError::InvalidConfig(
                "discord.webhook.send_message: 'webhook_name' and 'content' must be strings"
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

        let webhook_name = ctx
            .arg_stack
            .interpolate(config.str("webhook_name").unwrap_or_default());
        let content = ctx
            .arg_stack
            .interpolate(config.str("content").unwrap_or_default());

        if content.trim().is_empty() {
            return (
                SubActionTelemetry {
                    args_in: ::std::collections::BTreeMap::new(),
                    produced: ::std::collections::BTreeMap::new(),
                    kind: "discord.webhook.send_message".to_owned(),
                    started_at,
                    duration_ms: start.elapsed().as_millis() as u64,
                    outcome: SubActionOutcome::Failed("content is empty".to_owned()),
                    index: ctx.index,
                },
                None,
            );
        }

        let outcome =
            SubActionOutcome::from_result(&self.sink.post_text(&webhook_name, &content).await);

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                kind: "discord.webhook.send_message".to_owned(),
                started_at,
                duration_ms: start.elapsed().as_millis() as u64,
                outcome,
                index: ctx.index,
            },
            None,
        )
    }
}

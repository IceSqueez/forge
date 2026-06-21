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

pub struct PostEmbedRunner {
    sink: Arc<dyn DiscordSink>,
}

impl PostEmbedRunner {
    pub fn new(sink: Arc<dyn DiscordSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for PostEmbedRunner {
    fn id(&self) -> &str {
        "discord.webhook.send_embed"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Discord
    }

    fn label(&self) -> &str {
        "Post Embed"
    }

    fn summary(&self) -> &str {
        "Posts a rich embed message to a Discord webhook."
    }

    fn search_text(&self) -> &str {
        "discord webhook post embed rich card"
    }

    fn icon_name(&self) -> &str {
        "layout-cards"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            ("webhook_name".to_owned(), Variant::String(String::new())),
            ("embed_title".to_owned(), Variant::String(String::new())),
            (
                "embed_description".to_owned(),
                Variant::String(String::new()),
            ),
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
                key: "embed_title",
                label: "Title",
                placeholder: "Stream is live",
            },
            FormField::TextArea {
                key: "embed_description",
                label: "Description",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        if matches!(config.get("webhook_name"), Some(Variant::String(_))) {
            Ok(())
        } else {
            Err(RegistryError::UnknownKindId(
                "discord.webhook.send_embed: 'webhook_name' must be a string".to_owned(),
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
        let title = interp_opt(config, ctx, "embed_title");
        let description = interp_opt(config, ctx, "embed_description");
        let footer_text = interp_opt(config, ctx, "embed_footer_text");
        let author_name = interp_opt(config, ctx, "embed_author_name");
        let thumbnail_url = interp_opt(config, ctx, "embed_thumbnail_url");
        let image_url = interp_opt(config, ctx, "embed_image_url");

        let has_content = title.is_some() || description.is_some();
        if !has_content {
            return (
                SubActionTelemetry {
                    kind: "discord.webhook.send_embed".to_owned(),
                    started_at,
                    duration_ms: start.elapsed().as_millis() as u64,
                    outcome: SubActionOutcome::Failed("embed has no content".to_owned()),
                    index: ctx.index,
                },
                None,
            );
        }

        let embed = DiscordEmbed {
            title,
            description,
            footer_text,
            author_name,
            thumbnail_url,
            image_url,
            ..Default::default()
        };

        let outcome = match embed.validate() {
            Err(e) => SubActionOutcome::Failed(e.to_string()),
            Ok(()) => match self.sink.post_embed(&webhook_name, embed).await {
                Ok(_) => SubActionOutcome::Success,
                Err(e) => SubActionOutcome::Failed(e.to_string()),
            },
        };

        (
            SubActionTelemetry {
                kind: "discord.webhook.send_embed".to_owned(),
                started_at,
                duration_ms: start.elapsed().as_millis() as u64,
                outcome,
                index: ctx.index,
            },
            None,
        )
    }
}

fn interp_opt(config: &SubActionConfig, ctx: &RunContext<'_>, key: &str) -> Option<String> {
    let raw = config.get(key).and_then(|v| {
        if let Variant::String(s) = v {
            Some(s.as_str())
        } else {
            None
        }
    })?;
    let s = ctx.arg_stack.interpolate(raw);
    if s.is_empty() { None } else { Some(s) }
}

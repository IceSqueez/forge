use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigReason, FormField, RegistryError, RunContext, SubActionCategory, SubActionConfigExt,
    SubActionRunner,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use super::mention_fields::{mention_default_entries, mention_form_fields, mention_policy};
use crate::embed::DiscordEmbed;
use crate::sink::DiscordSink;
use crate::vocabulary::sub_action_ids;

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
        sub_action_ids::WEBHOOK_UPDATE_MESSAGE
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
        let mut config = BTreeMap::from([
            ("webhook_name".to_owned(), Variant::String(String::new())),
            ("message_id".to_owned(), Variant::String(String::new())),
            ("content".to_owned(), Variant::String(String::new())),
        ]);
        config.extend(mention_default_entries());
        config
    }

    fn config_fields(&self) -> Vec<FormField> {
        let mut fields = vec![
            FormField::Text {
                key: "webhook_name",
                label: "Webhook",
                placeholder: "alerts",
            },
            FormField::Text {
                key: "message_id",
                label: "Message ID",
                placeholder: "%discord.message_id%",
            },
            FormField::TextArea {
                key: "content",
                label: "New Content",
            },
        ];
        fields.extend(mention_form_fields());
        fields
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        if !matches!(config.get("webhook_name"), Some(Variant::String(_))) {
            return Err(RegistryError::invalid_field(
                "webhook_name",
                ConfigReason::Required,
            ));
        }
        if !matches!(config.get("message_id"), Some(Variant::String(_))) {
            return Err(RegistryError::invalid_field(
                "message_id",
                ConfigReason::Required,
            ));
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

        let webhook_name = ctx
            .arg_stack
            .interpolate(config.str("webhook_name").unwrap_or_default());
        let message_id = ctx
            .arg_stack
            .interpolate(config.str("message_id").unwrap_or_default());
        let content = config
            .str("content")
            .map(|raw| ctx.arg_stack.interpolate(raw))
            .filter(|s| !s.is_empty());

        let embed_title = config
            .str("embed_title")
            .map(|raw| ctx.arg_stack.interpolate(raw))
            .filter(|s| !s.is_empty());

        let has_any = content.is_some() || embed_title.is_some();
        if !has_any {
            return (
                SubActionTelemetry {
                    args_in: ::std::collections::BTreeMap::new(),
                    produced: ::std::collections::BTreeMap::new(),
                    kind: sub_action_ids::WEBHOOK_UPDATE_MESSAGE.to_owned(),
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

        let outcome = SubActionOutcome::from_result(
            &self
                .sink
                .edit_message(
                    &webhook_name,
                    &message_id,
                    content.as_deref(),
                    embed,
                    mention_policy(config),
                )
                .await,
        );

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                kind: sub_action_ids::WEBHOOK_UPDATE_MESSAGE.to_owned(),
                started_at,
                duration_ms: start.elapsed().as_millis() as u64,
                outcome,
                index: ctx.index,
            },
            None,
        )
    }
}

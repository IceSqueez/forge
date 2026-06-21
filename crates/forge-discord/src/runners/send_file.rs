use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::sink::DiscordSink;

pub struct SendFileRunner {
    sink: Arc<dyn DiscordSink>,
}

impl SendFileRunner {
    pub fn new(sink: Arc<dyn DiscordSink>) -> Self {
        Self { sink }
    }
}

fn interpolated_string(config: &SubActionConfig, ctx: &RunContext<'_>, key: &str) -> String {
    ctx.arg_stack.interpolate(
        config
            .get(key)
            .and_then(|v| {
                if let Variant::String(s) = v {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .unwrap_or_default(),
    )
}

#[async_trait]
impl SubActionRunner for SendFileRunner {
    fn id(&self) -> &str {
        "discord.webhook.send_file"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Discord
    }

    fn label(&self) -> &str {
        "Send File Attachment"
    }

    fn summary(&self) -> &str {
        "Uploads a file from disk to a Discord webhook with an optional caption."
    }

    fn search_text(&self) -> &str {
        "discord webhook send file attachment upload"
    }

    fn icon_name(&self) -> &str {
        "paperclip"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            ("webhook_name".to_owned(), Variant::String(String::new())),
            ("content".to_owned(), Variant::String(String::new())),
            ("file_path".to_owned(), Variant::String(String::new())),
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
                label: "Caption",
            },
            FormField::Text {
                key: "file_path",
                label: "File Path",
                placeholder: "/home/user/clip.png",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        let ok = matches!(config.get("webhook_name"), Some(Variant::String(_)))
            && matches!(config.get("file_path"), Some(Variant::String(_)));
        if ok {
            Ok(())
        } else {
            Err(RegistryError::UnknownKindId(
                "discord.webhook.send_file: 'webhook_name' and 'file_path' must be strings"
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

        let webhook_name = interpolated_string(config, ctx, "webhook_name");
        let file_path = interpolated_string(config, ctx, "file_path");
        let caption = {
            let raw = interpolated_string(config, ctx, "content");
            if raw.trim().is_empty() {
                None
            } else {
                Some(raw)
            }
        };

        let fail = |msg: String, start: Instant| {
            (
                SubActionTelemetry {
                    kind: "discord.webhook.send_file".to_owned(),
                    started_at,
                    duration_ms: start.elapsed().as_millis() as u64,
                    outcome: SubActionOutcome::Failed(msg),
                    index: ctx.index,
                },
                None,
            )
        };

        if file_path.trim().is_empty() {
            return fail("file_path is empty".to_owned(), start);
        }

        let file_name = match Path::new(&file_path).file_name().and_then(|n| n.to_str()) {
            Some(name) => name.to_owned(),
            None => return fail(format!("cannot derive file name from {file_path}"), start),
        };

        let file_bytes = match tokio::fs::read(&file_path).await {
            Ok(bytes) => bytes,
            Err(e) => return fail(format!("cannot read file: {e}"), start),
        };

        let outcome = match self
            .sink
            .send_file(&webhook_name, caption.as_deref(), &file_name, &file_bytes)
            .await
        {
            Ok(_) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        };

        (
            SubActionTelemetry {
                kind: "discord.webhook.send_file".to_owned(),
                started_at,
                duration_ms: start.elapsed().as_millis() as u64,
                outcome,
                index: ctx.index,
            },
            None,
        )
    }
}

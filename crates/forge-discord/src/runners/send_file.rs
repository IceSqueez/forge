use std::collections::BTreeMap;
use std::path::Path;
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

pub struct SendFileRunner {
    sink: Arc<dyn DiscordSink>,
}

impl SendFileRunner {
    pub fn new(sink: Arc<dyn DiscordSink>) -> Self {
        Self { sink }
    }
}

fn interpolated_string(config: &SubActionConfig, ctx: &RunContext<'_>, key: &str) -> String {
    ctx.arg_stack
        .interpolate(config.str(key).unwrap_or_default())
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
            Err(RegistryError::InvalidConfig(
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
                    args_in: ::std::collections::BTreeMap::new(),
                    produced: ::std::collections::BTreeMap::new(),
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

        let outcome = SubActionOutcome::from_result(
            &self
                .sink
                .send_file(&webhook_name, caption.as_deref(), &file_name, &file_bytes)
                .await,
        );

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::embed::DiscordEmbed;
    use crate::error::DiscordError;
    use forge_events::{Event, EventPublisher};
    use forge_types::EventId;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Captured {
        webhook_name: String,
        content: Option<String>,
        file_name: String,
        file_bytes: Vec<u8>,
    }

    struct CapturingSink {
        captured: Mutex<Option<Captured>>,
        should_fail: bool,
    }

    impl CapturingSink {
        fn ok() -> Arc<Self> {
            Arc::new(Self {
                captured: Mutex::new(None),
                should_fail: false,
            })
        }

        fn failing() -> Arc<Self> {
            Arc::new(Self {
                captured: Mutex::new(None),
                should_fail: true,
            })
        }

        fn captured(&self) -> Option<Captured> {
            self.captured.lock().unwrap().take()
        }
    }

    #[async_trait]
    impl DiscordSink for CapturingSink {
        async fn post_text(&self, _: &str, _: &str) -> Result<String, DiscordError> {
            Ok(String::new())
        }
        async fn post_embed(&self, _: &str, _: DiscordEmbed) -> Result<String, DiscordError> {
            Ok(String::new())
        }
        async fn edit_message(
            &self,
            _: &str,
            _: &str,
            _: Option<&str>,
            _: Option<DiscordEmbed>,
        ) -> Result<(), DiscordError> {
            Ok(())
        }
        async fn send_file(
            &self,
            webhook_name: &str,
            content: Option<&str>,
            file_name: &str,
            file_bytes: &[u8],
        ) -> Result<String, DiscordError> {
            *self.captured.lock().unwrap() = Some(Captured {
                webhook_name: webhook_name.to_owned(),
                content: content.map(str::to_owned),
                file_name: file_name.to_owned(),
                file_bytes: file_bytes.to_vec(),
            });
            if self.should_fail {
                Err(DiscordError::WebhookNotFound {
                    name: webhook_name.to_owned(),
                })
            } else {
                Ok("msg-1".to_owned())
            }
        }
        async fn delete_message(&self, _: &str, _: &str) -> Result<(), DiscordError> {
            Ok(())
        }
    }

    struct NoopPublisher;
    impl EventPublisher for NoopPublisher {
        fn publish(&self, _: Event) {}
    }

    fn config(file_path: &str, caption: &str) -> SubActionConfig {
        BTreeMap::from([
            (
                "webhook_name".to_owned(),
                Variant::String("alerts".to_owned()),
            ),
            ("content".to_owned(), Variant::String(caption.to_owned())),
            (
                "file_path".to_owned(),
                Variant::String(file_path.to_owned()),
            ),
        ])
    }

    fn ctx<'a>(stack: &'a ArgStack, publisher: &'a NoopPublisher) -> RunContext<'a> {
        RunContext::leaf(stack, 0, EventId::new(), publisher)
    }

    fn temp_file(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("forge-discord-test-{name}"));
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[tokio::test]
    async fn empty_file_path_fails_without_calling_sink() {
        let sink = CapturingSink::ok();
        let runner = SendFileRunner::new(Arc::clone(&sink) as Arc<dyn DiscordSink>);
        let stack = ArgStack::new();
        let publisher = NoopPublisher;
        let (telemetry, _) = runner
            .execute(&config("", "cap"), &ctx(&stack, &publisher))
            .await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(sink.captured().is_none(), "sink must not run on empty path");
    }

    #[tokio::test]
    async fn missing_file_fails_without_calling_sink() {
        let sink = CapturingSink::ok();
        let runner = SendFileRunner::new(Arc::clone(&sink) as Arc<dyn DiscordSink>);
        let stack = ArgStack::new();
        let publisher = NoopPublisher;
        let missing = std::env::temp_dir().join("forge-discord-test-does-not-exist-xyz.png");
        let cfg = config(missing.to_str().unwrap(), "cap");
        let (telemetry, _) = runner.execute(&cfg, &ctx(&stack, &publisher)).await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(
            sink.captured().is_none(),
            "sink must not run on unreadable file"
        );
    }

    #[tokio::test]
    async fn reads_file_and_forwards_bytes_basename_and_no_caption_when_empty() {
        let sink = CapturingSink::ok();
        let runner = SendFileRunner::new(Arc::clone(&sink) as Arc<dyn DiscordSink>);
        let stack = ArgStack::new();
        let publisher = NoopPublisher;
        let path = temp_file("clip-a.png", &[0xDE, 0xAD, 0xBE, 0xEF]);
        let cfg = config(path.to_str().unwrap(), "   ");
        let (telemetry, _) = runner.execute(&cfg, &ctx(&stack, &publisher)).await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
        let c = sink.captured().expect("sink must be called");
        assert_eq!(c.file_bytes, vec![0xDE, 0xAD, 0xBE, 0xEF]);
        assert_eq!(c.file_name, "forge-discord-test-clip-a.png");
        assert_eq!(c.webhook_name, "alerts");
        assert!(c.content.is_none(), "blank caption must become None");
        std::fs::remove_file(&path).ok();
    }

    #[tokio::test]
    async fn non_empty_caption_is_forwarded_as_some() {
        let sink = CapturingSink::ok();
        let runner = SendFileRunner::new(Arc::clone(&sink) as Arc<dyn DiscordSink>);
        let stack = ArgStack::new();
        let publisher = NoopPublisher;
        let path = temp_file("clip-b.png", &[1, 2, 3]);
        let cfg = config(path.to_str().unwrap(), "look at this");
        let (_telemetry, _) = runner.execute(&cfg, &ctx(&stack, &publisher)).await;
        let c = sink.captured().expect("sink must be called");
        assert_eq!(c.content.as_deref(), Some("look at this"));
        std::fs::remove_file(&path).ok();
    }

    #[tokio::test]
    async fn sink_error_yields_failed_outcome() {
        let sink = CapturingSink::failing();
        let runner = SendFileRunner::new(Arc::clone(&sink) as Arc<dyn DiscordSink>);
        let stack = ArgStack::new();
        let publisher = NoopPublisher;
        let path = temp_file("clip-c.png", &[9]);
        let cfg = config(path.to_str().unwrap(), "cap");
        let (telemetry, _) = runner.execute(&cfg, &ctx(&stack, &publisher)).await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        std::fs::remove_file(&path).ok();
    }
}

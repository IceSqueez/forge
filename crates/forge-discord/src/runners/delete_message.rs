use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::sink::DiscordSink;

pub struct DeleteMessageRunner {
    sink: Arc<dyn DiscordSink>,
}

impl DeleteMessageRunner {
    pub fn new(sink: Arc<dyn DiscordSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for DeleteMessageRunner {
    fn id(&self) -> &str {
        "discord.webhook.delete_message"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Discord
    }

    fn label(&self) -> &str {
        "Delete Webhook Message"
    }

    fn summary(&self) -> &str {
        "Deletes a previously posted Discord webhook message."
    }

    fn search_text(&self) -> &str {
        "discord webhook delete remove message"
    }

    fn icon_name(&self) -> &str {
        "trash"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            ("webhook_name".to_owned(), Variant::String(String::new())),
            ("message_id".to_owned(), Variant::String(String::new())),
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
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        let ok = matches!(config.get("webhook_name"), Some(Variant::String(_)))
            && matches!(config.get("message_id"), Some(Variant::String(_)));
        if ok {
            Ok(())
        } else {
            Err(RegistryError::UnknownKindId(
                "discord.webhook.delete_message: 'webhook_name' and 'message_id' must be strings"
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

        if message_id.trim().is_empty() {
            return (
                SubActionTelemetry {
                    kind: "discord.webhook.delete_message".to_owned(),
                    started_at,
                    duration_ms: start.elapsed().as_millis() as u64,
                    outcome: SubActionOutcome::Failed("message_id is empty".to_owned()),
                    index: ctx.index,
                },
                None,
            );
        }

        let outcome = match self.sink.delete_message(&webhook_name, &message_id).await {
            Ok(()) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        };

        (
            SubActionTelemetry {
                kind: "discord.webhook.delete_message".to_owned(),
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
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::embed::DiscordEmbed;
    use crate::error::DiscordError;
    use forge_events::{Event, EventPublisher};
    use forge_types::EventId;
    use std::sync::Mutex;

    struct CapturingSink {
        deleted_id: Mutex<Option<String>>,
        should_fail: bool,
    }

    impl CapturingSink {
        fn ok() -> Arc<Self> {
            Arc::new(Self {
                deleted_id: Mutex::new(None),
                should_fail: false,
            })
        }

        fn failing() -> Arc<Self> {
            Arc::new(Self {
                deleted_id: Mutex::new(None),
                should_fail: true,
            })
        }

        fn deleted_id(&self) -> Option<String> {
            self.deleted_id.lock().unwrap().take()
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
            _: &str,
            _: Option<&str>,
            _: &str,
            _: &[u8],
        ) -> Result<String, DiscordError> {
            Ok(String::new())
        }
        async fn delete_message(
            &self,
            _webhook_name: &str,
            message_id: &str,
        ) -> Result<(), DiscordError> {
            *self.deleted_id.lock().unwrap() = Some(message_id.to_owned());
            if self.should_fail {
                Err(DiscordError::BadResponse {
                    status: 404,
                    body: "unknown message".to_owned(),
                })
            } else {
                Ok(())
            }
        }
    }

    struct NoopPublisher;
    impl EventPublisher for NoopPublisher {
        fn publish(&self, _: Event) {}
    }

    fn config(message_id: &str) -> SubActionConfig {
        BTreeMap::from([
            (
                "webhook_name".to_owned(),
                Variant::String("alerts".to_owned()),
            ),
            (
                "message_id".to_owned(),
                Variant::String(message_id.to_owned()),
            ),
        ])
    }

    fn ctx<'a>(stack: &'a ArgStack, publisher: &'a NoopPublisher) -> RunContext<'a> {
        RunContext {
            arg_stack: stack,
            index: 0,
            parent_event_id: EventId::new(),
            publisher,
        }
    }

    #[tokio::test]
    async fn empty_message_id_fails_without_calling_sink() {
        let sink = CapturingSink::ok();
        let runner = DeleteMessageRunner::new(Arc::clone(&sink) as Arc<dyn DiscordSink>);
        let stack = ArgStack::new();
        let publisher = NoopPublisher;
        let (telemetry, _) = runner.execute(&config(""), &ctx(&stack, &publisher)).await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(sink.deleted_id().is_none(), "sink must not run on empty id");
    }

    #[tokio::test]
    async fn interpolated_message_id_reaches_sink() {
        let sink = CapturingSink::ok();
        let runner = DeleteMessageRunner::new(Arc::clone(&sink) as Arc<dyn DiscordSink>);
        let stack = ArgStack::new().set("msg".to_owned(), Variant::String("987654321".to_owned()));
        let publisher = NoopPublisher;
        let (telemetry, _) = runner
            .execute(&config("%msg%"), &ctx(&stack, &publisher))
            .await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
        assert_eq!(sink.deleted_id().as_deref(), Some("987654321"));
    }

    #[tokio::test]
    async fn sink_error_yields_failed_outcome() {
        let sink = CapturingSink::failing();
        let runner = DeleteMessageRunner::new(Arc::clone(&sink) as Arc<dyn DiscordSink>);
        let stack = ArgStack::new();
        let publisher = NoopPublisher;
        let (telemetry, _) = runner
            .execute(&config("123"), &ctx(&stack, &publisher))
            .await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
    }
}

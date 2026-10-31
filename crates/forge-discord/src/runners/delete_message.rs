use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::SubActionConfig;
use forge_registry::{
    ConfigProblem, ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner,
    TypedField,
};
use forge_types::{ArgStack, SubActionTelemetry};

use super::fields::{MESSAGE_ID, WEBHOOK};
use super::step_failure;
use crate::sink::DiscordSink;
use crate::vocabulary::sub_action_ids;

const KIND_ID: &str = sub_action_ids::WEBHOOK_DELETE_MESSAGE;

const CONFIG: ConfigSpec = ConfigSpec::new(&[&WEBHOOK, &MESSAGE_ID]);

fn read_config(
    config: &SubActionConfig,
    args: &ArgStack,
) -> Result<(String, String), ConfigProblem> {
    Ok((WEBHOOK.read(config, args)?, MESSAGE_ID.read(config, args)?))
}

pub(crate) struct DeleteMessageRunner {
    sink: Arc<dyn DiscordSink>,
}

impl DeleteMessageRunner {
    pub(crate) fn new(sink: Arc<dyn DiscordSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for DeleteMessageRunner {
    fn id(&self) -> &str {
        KIND_ID
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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, KIND_ID);
        let (webhook, message_id) = match read_config(config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let outcome = step_failure::outcome(&self.sink.delete_message(&webhook, &message_id).await);
        (timer.finish(outcome), None)
    }
}

#[cfg(test)]
mod tests {
    use forge_events::test_support::NullPublisher;
    use std::collections::BTreeMap;

    use forge_types::Variant;

    use forge_types::SubActionOutcome;

    use super::*;
    use crate::embed::DiscordEmbed;
    use crate::error::DiscordError;
    use crate::mention::MentionPolicy;

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
        async fn post_text(
            &self,
            _: &str,
            _: &str,
            _: MentionPolicy,
        ) -> Result<String, DiscordError> {
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
            _: MentionPolicy,
        ) -> Result<(), DiscordError> {
            Ok(())
        }
        async fn send_file(
            &self,
            _: &str,
            _: Option<&str>,
            _: &str,
            _: &[u8],
            _: MentionPolicy,
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

    fn ctx<'a>(stack: &'a ArgStack, publisher: &'a NullPublisher) -> RunContext<'a> {
        RunContext::leaf(stack, 0, EventId::new(), publisher)
    }

    #[tokio::test]
    async fn empty_message_id_fails_without_calling_sink() {
        let sink = CapturingSink::ok();
        let runner = DeleteMessageRunner::new(Arc::clone(&sink) as Arc<dyn DiscordSink>);
        let stack = ArgStack::new();
        let publisher = NullPublisher;
        let (telemetry, _) = runner.execute(&config(""), &ctx(&stack, &publisher)).await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(sink.deleted_id().is_none(), "sink must not run on empty id");
    }

    #[tokio::test]
    async fn interpolated_message_id_reaches_sink() {
        let sink = CapturingSink::ok();
        let runner = DeleteMessageRunner::new(Arc::clone(&sink) as Arc<dyn DiscordSink>);
        let stack = ArgStack::new().set("msg".to_owned(), Variant::String("987654321".to_owned()));
        let publisher = NullPublisher;
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
        let publisher = NullPublisher;
        let (telemetry, _) = runner
            .execute(&config("123"), &ctx(&stack, &publisher))
            .await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
    }
}

use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{
    FormField, RegistryError, RunContext, StepTimer, SubActionCategory, SubActionConfigExt,
    SubActionRunner,
};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};

use crate::speak_dispatcher::SpeakDispatcher;

pub struct SpeakRunner {
    speak: Arc<dyn SpeakDispatcher>,
}

impl SpeakRunner {
    pub fn new(speak: Arc<dyn SpeakDispatcher>) -> Self {
        Self { speak }
    }
}

#[async_trait]
impl SubActionRunner for SpeakRunner {
    fn id(&self) -> &str {
        "tts.speak.text"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Tts
    }

    fn label(&self) -> &str {
        "Speak Text"
    }

    fn summary(&self) -> &str {
        "Send text to the TTS speak queue with an optional voice alias override"
    }

    fn search_text(&self) -> &str {
        "speak tts text voice alias queue"
    }

    fn icon_name(&self) -> &str {
        "message-circle"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("text".to_owned(), Variant::String(String::new()));
        cfg.insert("wait_for_completion".to_owned(), Variant::Bool(true));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::TextArea {
                key: "text",
                label: "Text",
            },
            FormField::Optional {
                key: "voice_alias",
                label: "Voice alias",
                inner: Box::new(FormField::Text {
                    key: "voice_alias",
                    label: "Voice alias",
                    placeholder: "e.g. piper/en_US-amy-medium",
                }),
            },
            FormField::Toggle {
                key: "wait_for_completion",
                label: "Wait for completion",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.str_nonempty("text") {
            Some(_) => Ok(()),
            None => Err(RegistryError::InvalidConfig(
                "tts.speak.text: text is required".to_owned(),
            )),
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, "tts.speak.text");

        let raw_text = config.str("text").unwrap_or_default();
        let text = ctx.arg_stack.interpolate(raw_text);

        let voice_alias = config.str("voice_alias").map(|s| s.to_owned());

        let is_reward = ctx.arg_stack.get("reward.id").is_some();
        let wait_for_completion = config.bool("wait_for_completion").unwrap_or(true);
        let dispatch_result = if wait_for_completion {
            self.speak
                .speak_and_wait(text, voice_alias, is_reward, ctx.cancel.clone())
                .await
        } else if is_reward {
            self.speak.speak_reward_sourced(text, voice_alias).await
        } else {
            self.speak.speak(text, voice_alias).await
        };

        (
            timer.finish(SubActionOutcome::from_result(&dispatch_result)),
            None,
        )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use async_trait::async_trait;
    use forge_types::{EventId, SubActionOutcome};

    use super::*;
    use crate::speak_dispatcher::{SpeakDispatchError, SpeakDispatcher};
    use forge_events::{Event, EventPublisher};

    struct NullPublisher;

    impl EventPublisher for NullPublisher {
        fn publish(&self, _event: Event) {}
    }

    struct OkSpeaker;

    #[async_trait]
    impl SpeakDispatcher for OkSpeaker {
        async fn speak(
            &self,
            _text: String,
            _voice_id_override: Option<String>,
        ) -> Result<(), SpeakDispatchError> {
            Ok(())
        }
    }

    struct FailSpeaker;

    #[async_trait]
    impl SpeakDispatcher for FailSpeaker {
        async fn speak(
            &self,
            _text: String,
            _voice_id_override: Option<String>,
        ) -> Result<(), SpeakDispatchError> {
            Err(SpeakDispatchError::Dispatch("queue full".to_owned()))
        }
    }

    fn make_ctx(stack: &ArgStack) -> RunContext<'_> {
        RunContext::leaf(stack, 0, EventId::new(), &NullPublisher)
    }

    fn config_with_text(text: &str) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("text".to_owned(), Variant::String(text.to_owned()));
        cfg
    }

    #[tokio::test]
    async fn success_path() {
        let runner = SpeakRunner::new(Arc::new(OkSpeaker));
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let (telemetry, updated) = runner.execute(&config_with_text("Hello chat!"), &ctx).await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
        assert!(updated.is_none());
    }

    #[tokio::test]
    async fn failure_path() {
        let runner = SpeakRunner::new(Arc::new(FailSpeaker));
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let (telemetry, _) = runner.execute(&config_with_text("Hello!"), &ctx).await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
    }

    #[tokio::test]
    async fn empty_text_is_forwarded() {
        let runner = SpeakRunner::new(Arc::new(OkSpeaker));
        let cfg = runner.default_config();
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let (telemetry, _) = runner.execute(&cfg, &ctx).await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
    }

    #[tokio::test]
    async fn arg_stack_interpolation_applied() {
        use std::sync::{Arc, Mutex};

        struct CapturingSpeaker {
            captured: Arc<Mutex<String>>,
        }

        #[async_trait]
        impl SpeakDispatcher for CapturingSpeaker {
            async fn speak(
                &self,
                text: String,
                _voice_id_override: Option<String>,
            ) -> Result<(), SpeakDispatchError> {
                *self.captured.lock().unwrap() = text;
                Ok(())
            }
        }

        let captured: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
        let runner = SpeakRunner::new(Arc::new(CapturingSpeaker {
            captured: Arc::clone(&captured),
        }));

        let stack = ArgStack::new().set("user".to_owned(), Variant::String("Alice".to_owned()));
        let ctx = make_ctx(&stack);

        runner
            .execute(&config_with_text("Welcome %user%!"), &ctx)
            .await;

        assert_eq!(*captured.lock().unwrap(), "Welcome Alice!");
    }

    #[test]
    fn validate_config_rejects_missing_text() {
        let runner = SpeakRunner::new(Arc::new(OkSpeaker));
        assert!(runner.validate_config(&SubActionConfig::new()).is_err());
    }

    #[test]
    fn validate_config_accepts_nonempty_text() {
        let runner = SpeakRunner::new(Arc::new(OkSpeaker));
        assert!(runner.validate_config(&config_with_text("hello")).is_ok());
    }
}

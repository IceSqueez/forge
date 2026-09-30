use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{
    FormField, RegistryError, RunContext, StepTimer, SubActionCategory, SubActionConfigExt,
    SubActionRunner,
};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};

use crate::speak_dispatcher::{SpeakDispatcher, SpeechOrigin};

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
        let origin = SpeechOrigin::from_args(ctx.arg_stack, Some(ctx.parent_event_id));
        let wait_for_completion = config.bool("wait_for_completion").unwrap_or(true);
        let dispatch_result = if wait_for_completion {
            self.speak
                .speak_and_wait(text, voice_alias, is_reward, origin, ctx.cancel.clone())
                .await
        } else if is_reward {
            self.speak
                .speak_reward_sourced(text, voice_alias, origin)
                .await
        } else {
            self.speak.speak(text, voice_alias, origin).await
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
    use std::sync::Mutex;

    use async_trait::async_trait;
    use forge_types::{EventId, SubActionOutcome};

    use super::*;
    use crate::speak_dispatcher::{SpeakDispatchError, SpeakDispatcher, SpeakingViewer};
    use forge_events::{Event, EventPublisher};

    struct NullPublisher;

    impl EventPublisher for NullPublisher {
        fn publish(&self, _event: Event) {}
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Path {
        Speak,
        Reward,
    }

    #[derive(Debug, Clone)]
    struct Heard {
        text: String,
        path: Path,
        origin: SpeechOrigin,
    }

    #[derive(Default)]
    struct Capturing {
        heard: Mutex<Vec<Heard>>,
    }

    impl Capturing {
        fn only(&self) -> Heard {
            let heard = self.heard.lock().unwrap();
            assert_eq!(heard.len(), 1, "expected exactly one dispatch: {heard:?}");
            heard[0].clone()
        }
    }

    #[async_trait]
    impl SpeakDispatcher for Capturing {
        async fn speak(
            &self,
            text: String,
            _voice_id_override: Option<String>,
            origin: SpeechOrigin,
        ) -> Result<(), SpeakDispatchError> {
            self.heard.lock().unwrap().push(Heard {
                text,
                path: Path::Speak,
                origin,
            });
            Ok(())
        }

        async fn speak_reward_sourced(
            &self,
            text: String,
            _voice_id_override: Option<String>,
            origin: SpeechOrigin,
        ) -> Result<(), SpeakDispatchError> {
            self.heard.lock().unwrap().push(Heard {
                text,
                path: Path::Reward,
                origin,
            });
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
            _origin: SpeechOrigin,
        ) -> Result<(), SpeakDispatchError> {
            Err(SpeakDispatchError::Dispatch("queue full".to_owned()))
        }
    }

    fn make_ctx(stack: &ArgStack) -> RunContext<'_> {
        RunContext::leaf(stack, 0, EventId::new(), &NullPublisher)
    }

    fn config(text: &str, wait: bool) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("text".to_owned(), Variant::String(text.to_owned()));
        cfg.insert("wait_for_completion".to_owned(), Variant::Bool(wait));
        cfg
    }

    fn chat_stack() -> ArgStack {
        ArgStack::new()
            .set(
                "user_id".to_owned(),
                Variant::String("141981764".to_owned()),
            )
            .set(
                "user_name".to_owned(),
                Variant::String("NovaFox".to_owned()),
            )
            .set(
                "user_login".to_owned(),
                Variant::String("novafox".to_owned()),
            )
            .set(
                "user_platform".to_owned(),
                Variant::String("twitch".to_owned()),
            )
    }

    fn nova() -> SpeakingViewer {
        SpeakingViewer {
            platform: "twitch".to_owned(),
            id: "141981764".to_owned(),
            name: "NovaFox".to_owned(),
        }
    }

    #[tokio::test]
    async fn a_chat_triggered_speak_carries_the_viewer_and_the_triggering_event() {
        let speaker = Arc::new(Capturing::default());
        let runner = SpeakRunner::new(speaker.clone());
        let stack = chat_stack();
        let ctx = make_ctx(&stack);

        let (telemetry, _) = runner.execute(&config("hello", false), &ctx).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
        assert_eq!(
            speaker.only().origin,
            SpeechOrigin {
                viewer: Some(nova()),
                caused_by: Some(ctx.parent_event_id),
            }
        );
    }

    #[tokio::test]
    async fn every_dispatch_path_forwards_the_viewer() {
        for (wait, reward, path) in [
            (true, false, Path::Speak),
            (false, false, Path::Speak),
            (true, true, Path::Reward),
            (false, true, Path::Reward),
        ] {
            let speaker = Arc::new(Capturing::default());
            let runner = SpeakRunner::new(speaker.clone());
            let mut stack = chat_stack();
            if reward {
                stack = stack.set("reward.id".to_owned(), Variant::String("r-1".to_owned()));
            }
            let ctx = make_ctx(&stack);

            runner.execute(&config("hello", wait), &ctx).await;

            let heard = speaker.only();
            assert_eq!(heard.path, path, "wait={wait} reward={reward}");
            assert_eq!(
                heard.origin.viewer,
                Some(nova()),
                "wait={wait} reward={reward} lost the viewer"
            );
        }
    }

    #[tokio::test]
    async fn a_speak_without_a_triggering_viewer_carries_no_viewer() {
        let speaker = Arc::new(Capturing::default());
        let runner = SpeakRunner::new(speaker.clone());
        let stack = ArgStack::new()
            .set(
                "timer.name".to_owned(),
                Variant::String("hydrate".to_owned()),
            )
            .set("user".to_owned(), Variant::String("NovaFox".to_owned()));
        let ctx = make_ctx(&stack);

        runner.execute(&config("drink water", false), &ctx).await;

        assert_eq!(speaker.only().origin.viewer, None);
    }

    #[tokio::test]
    async fn a_dispatch_error_fails_the_step() {
        let runner = SpeakRunner::new(Arc::new(FailSpeaker));
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);

        let (telemetry, _) = runner.execute(&config("Hello!", false), &ctx).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
    }

    #[tokio::test]
    async fn text_is_interpolated_from_the_arg_stack() {
        let speaker = Arc::new(Capturing::default());
        let runner = SpeakRunner::new(speaker.clone());
        let stack = ArgStack::new().set("user".to_owned(), Variant::String("Alice".to_owned()));
        let ctx = make_ctx(&stack);

        runner
            .execute(&config("Welcome %user%!", false), &ctx)
            .await;

        assert_eq!(speaker.only().text, "Welcome Alice!");
    }

    #[test]
    fn validate_config_rejects_missing_text() {
        let runner = SpeakRunner::new(Arc::new(FailSpeaker));
        assert!(runner.validate_config(&SubActionConfig::new()).is_err());
    }

    #[test]
    fn validate_config_accepts_nonempty_text() {
        let runner = SpeakRunner::new(Arc::new(FailSpeaker));
        assert!(runner.validate_config(&config("hello", true)).is_ok());
    }
}

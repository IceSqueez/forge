use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{
    ENGINE_VOICE_OPTIONS_KEY, FormField, RegistryError, RunContext, StepTimer, SubActionCategory,
    SubActionConfigExt, SubActionRunner,
};
use forge_types::{
    ArgStack, PlatformId, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant,
};

use crate::engine_voice_ref::EngineVoiceRef;
use crate::speak_dispatcher::{SpeakDispatcher, SpeechOrigin};
use crate::twitch_emote_lexicon::TwitchEmoteLexicon;

const VOICE_KEY: &str = "voice";

pub struct SpeakRunner {
    speak: Arc<dyn SpeakDispatcher>,
    reward_emotes: TwitchEmoteLexicon,
}

impl SpeakRunner {
    pub fn new(speak: Arc<dyn SpeakDispatcher>) -> Self {
        Self {
            speak,
            reward_emotes: TwitchEmoteLexicon::default(),
        }
    }

    pub fn with_reward_emotes(mut self, lexicon: TwitchEmoteLexicon) -> Self {
        self.reward_emotes = lexicon;
        self
    }

    async fn installed_voice(&self, config: &SubActionConfig) -> Result<Option<String>, String> {
        let Some(stored) = config.str_nonempty(VOICE_KEY) else {
            return Ok(None);
        };
        EngineVoiceRef::installed(stored, &self.speak.get_available_voices().await)
            .map(|voice| Some(voice.to_string()))
            .map_err(|reason| format!("tts.speak.text: {reason} - pick a voice again"))
    }

    fn add_learned_reward_emotes(&self, origin: &mut SpeechOrigin, text: &str) {
        let from_twitch = origin
            .viewer
            .as_ref()
            .is_some_and(|viewer| viewer.platform == PlatformId::Twitch.as_str());
        if from_twitch {
            origin
                .message_emotes
                .extend(self.reward_emotes.codes_in(text));
        }
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
        "Send text to the TTS speak queue, optionally with a chosen engine voice"
    }

    fn search_text(&self) -> &str {
        "speak tts text voice engine queue"
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
                key: VOICE_KEY,
                label: "Voice (leave empty for the default voice)",
                inner: Box::new(FormField::DynamicSelect {
                    key: VOICE_KEY,
                    label: "Voice",
                    options_key: ENGINE_VOICE_OPTIONS_KEY,
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

        let voice = match self.installed_voice(config).await {
            Ok(voice) => voice,
            Err(reason) => return (timer.finish(SubActionOutcome::Failed(reason)), None),
        };

        let is_reward = ctx.arg_stack.get("reward.id").is_some();
        let mut origin = SpeechOrigin::from_args(ctx.arg_stack, Some(ctx.parent_event_id));
        if is_reward {
            self.add_learned_reward_emotes(&mut origin, &text);
        }
        let wait_for_completion = config.bool("wait_for_completion").unwrap_or(true);
        let dispatch_result = if wait_for_completion {
            self.speak
                .speak_and_wait(text, voice, is_reward, origin, ctx.cancel.clone())
                .await
        } else if is_reward {
            self.speak.speak_reward_sourced(text, voice, origin).await
        } else {
            self.speak.speak(text, voice, origin).await
        };

        (
            timer.finish(SubActionOutcome::from_result(&dispatch_result)),
            None,
        )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
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
                ..Default::default()
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

    fn lexicon_knowing_lul() -> TwitchEmoteLexicon {
        let lexicon = TwitchEmoteLexicon::default();
        lexicon.learn_from(&Event::new(
            forge_events::EventSource::Twitch,
            "twitch.channel.chat.message",
            serde_json::json!({
                forge_types::ChatPayload::KEY: {
                    "platform_msg_id": "m-1",
                    "author": "NovaFox",
                    "author_color": null,
                    "segments": [{ "type": "emote", "id": "425618", "name": "LUL" }],
                    "badges": [],
                    "is_event": false,
                    "event_detail": null,
                }
            }),
        ));
        lexicon
    }

    #[tokio::test]
    async fn learned_twitch_emotes_reach_only_twitch_reward_speech() {
        let strings = |codes: &[&str]| -> Vec<Variant> {
            codes
                .iter()
                .map(|code| Variant::String((*code).to_owned()))
                .collect()
        };
        for (label, platform, reward, message_emotes, text, expected) in [
            ("twitch reward", "twitch", true, None, "LUL hi", vec!["LUL"]),
            (
                "twitch reward without codes",
                "twitch",
                true,
                None,
                "hi there",
                vec![],
            ),
            ("kick reward", "kick", true, None, "LUL hi", vec![]),
            (
                "twitch chat",
                "twitch",
                false,
                Some(strings(&["Kappa"])),
                "LUL Kappa hi",
                vec!["Kappa"],
            ),
        ] {
            let speaker = Arc::new(Capturing::default());
            let runner =
                SpeakRunner::new(speaker.clone()).with_reward_emotes(lexicon_knowing_lul());
            let mut stack = chat_stack().set(
                "user_platform".to_owned(),
                Variant::String(platform.to_owned()),
            );
            if reward {
                stack = stack.set("reward.id".to_owned(), Variant::String("r-1".to_owned()));
            }
            if let Some(codes) = message_emotes {
                stack = stack.set("message_emotes".to_owned(), Variant::Array(codes));
            }
            let ctx = make_ctx(&stack);

            runner.execute(&config(text, false), &ctx).await;

            assert_eq!(speaker.only().origin.message_emotes, expected, "{label}");
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

    #[derive(Default)]
    struct VoicedSpeaker {
        voices: Vec<crate::speak_dispatcher::VoiceDescriptor>,
        dispatched: Mutex<Vec<(Path, Option<String>)>>,
    }

    impl VoicedSpeaker {
        fn with_installed(engine_id: &str, voice_id: &str) -> Self {
            Self {
                voices: vec![crate::speak_dispatcher::VoiceDescriptor {
                    id: voice_id.to_owned(),
                    name: "Amy".to_owned(),
                    locale: "en-US".to_owned(),
                    engine_id: engine_id.to_owned(),
                }],
                dispatched: Mutex::new(Vec::new()),
            }
        }

        fn dispatched(&self) -> Vec<(Path, Option<String>)> {
            self.dispatched.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl SpeakDispatcher for VoicedSpeaker {
        async fn speak(
            &self,
            _text: String,
            voice: Option<String>,
            _origin: SpeechOrigin,
        ) -> Result<(), SpeakDispatchError> {
            self.dispatched.lock().unwrap().push((Path::Speak, voice));
            Ok(())
        }

        async fn speak_reward_sourced(
            &self,
            _text: String,
            voice: Option<String>,
            _origin: SpeechOrigin,
        ) -> Result<(), SpeakDispatchError> {
            self.dispatched.lock().unwrap().push((Path::Reward, voice));
            Ok(())
        }

        async fn get_available_voices(&self) -> Vec<crate::speak_dispatcher::VoiceDescriptor> {
            self.voices.clone()
        }
    }

    fn voiced(text: &str, wait: bool, voice: &str) -> SubActionConfig {
        let mut cfg = config(text, wait);
        cfg.insert(VOICE_KEY.to_owned(), Variant::String(voice.to_owned()));
        cfg
    }

    #[tokio::test]
    async fn a_step_without_a_chosen_voice_speaks_with_the_default_voice() {
        for step in [config("hello", false), voiced("hello", false, "")] {
            let speaker = Arc::new(VoicedSpeaker::with_installed("piper", "amy"));
            let runner = SpeakRunner::new(speaker.clone());
            let stack = ArgStack::new();

            let (telemetry, _) = runner.execute(&step, &make_ctx(&stack)).await;

            assert!(
                matches!(telemetry.outcome, SubActionOutcome::Success)
                    && speaker.dispatched() == vec![(Path::Speak, None)],
                "{step:?} -> {:?} / {:?}",
                telemetry.outcome,
                speaker.dispatched()
            );
        }
    }

    #[tokio::test]
    async fn an_installed_voice_is_dispatched_on_every_speak_path() {
        for (wait, reward, path) in [
            (true, false, Path::Speak),
            (false, false, Path::Speak),
            (true, true, Path::Reward),
            (false, true, Path::Reward),
        ] {
            let speaker = Arc::new(VoicedSpeaker::with_installed("piper", "en/amy"));
            let runner = SpeakRunner::new(speaker.clone());
            let mut stack = ArgStack::new();
            if reward {
                stack = stack.set("reward.id".to_owned(), Variant::String("r-1".to_owned()));
            }

            runner
                .execute(&voiced("hello", wait, "piper/en/amy"), &make_ctx(&stack))
                .await;

            assert_eq!(
                speaker.dispatched(),
                vec![(path, Some("piper/en/amy".to_owned()))],
                "wait={wait} reward={reward}"
            );
        }
    }

    #[tokio::test]
    async fn an_unusable_voice_fails_the_step_naming_it_and_speaks_nothing() {
        for (stored, names) in [
            ("amy", "\"amy\" is not an engine voice"),
            (
                "piper/ghost",
                "voice \"ghost\" of engine \"piper\" is not installed",
            ),
            (
                "sapi/amy",
                "voice \"amy\" of engine \"sapi\" is not installed",
            ),
        ] {
            let speaker = Arc::new(VoicedSpeaker::with_installed("piper", "amy"));
            let runner = SpeakRunner::new(speaker.clone());
            let stack = ArgStack::new();

            let (telemetry, _) = runner
                .execute(&voiced("hello", false, stored), &make_ctx(&stack))
                .await;

            match telemetry.outcome {
                SubActionOutcome::Failed(reason) => assert!(
                    reason.contains(names) && reason.ends_with("pick a voice again"),
                    "{stored:?} failed with {reason:?}"
                ),
                other => panic!("{stored:?} must fail the step, got {other:?}"),
            }
            assert!(speaker.dispatched().is_empty(), "{stored:?} was spoken");
        }
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

use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use forge_storage::TtsTriggerSettings;

use crate::speak_dispatcher::SpeakDispatcher;
use crate::tts_trigger_settings::TtsTriggerSettingsHandle;

pub struct SpeakRunner {
    speak: Arc<dyn SpeakDispatcher>,
    trigger_settings: TtsTriggerSettingsHandle,
}

impl SpeakRunner {
    pub fn new(
        speak: Arc<dyn SpeakDispatcher>,
        trigger_settings: TtsTriggerSettingsHandle,
    ) -> Self {
        Self {
            speak,
            trigger_settings,
        }
    }
}

/// Origin category a speak fires from, inferred from the arg-stack keys the
/// triggering platform event placed. Selects which `TtsTriggerSettings` toggle
/// gates the speech. A stack carrying none of these keys (e.g. a script-driven
/// speak) has no gating category and always speaks.
enum SpeakOrigin {
    Command,
    ChannelPoints,
    Bits,
    Sub,
}

impl SpeakOrigin {
    fn is_enabled(&self, settings: &TtsTriggerSettings) -> bool {
        match self {
            SpeakOrigin::Command => settings.command_enabled,
            SpeakOrigin::ChannelPoints => settings.channel_points_enabled,
            SpeakOrigin::Bits => settings.bits_enabled,
            SpeakOrigin::Sub => settings.sub_messages_enabled,
        }
    }

    fn disabled_reason(&self) -> &'static str {
        match self {
            SpeakOrigin::Command => "command-sourced TTS is disabled",
            SpeakOrigin::ChannelPoints => "channel-point-sourced TTS is disabled",
            SpeakOrigin::Bits => "bits-sourced TTS is disabled",
            SpeakOrigin::Sub => "subscription-sourced TTS is disabled",
        }
    }
}

/// Reward redemptions carry `reward.id`; cheers add `cheer.bits` over the base
/// chat args; subscriptions carry `sub_tier`; plain chat/command messages carry
/// `message_text`. Reward/cheer are checked first because they also carry the
/// base chat keys.
fn classify_origin(arg_stack: &ArgStack) -> Option<SpeakOrigin> {
    if arg_stack.get("reward.id").is_some() {
        Some(SpeakOrigin::ChannelPoints)
    } else if arg_stack.get("cheer.bits").is_some() {
        Some(SpeakOrigin::Bits)
    } else if arg_stack.get("sub_tier").is_some() {
        Some(SpeakOrigin::Sub)
    } else if arg_stack.get("message_text").is_some() {
        Some(SpeakOrigin::Command)
    } else {
        None
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
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("text").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::UnknownKindId(
                "tts.speak.text: text is required".to_owned(),
            )),
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();

        let raw_text = config
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let text = ctx.arg_stack.interpolate(raw_text);

        let voice_alias = config
            .get("voice_alias")
            .and_then(|v| v.as_str())
            .map(|s| s.to_owned());

        let settings = self.trigger_settings.load();
        if let Some(origin) = classify_origin(ctx.arg_stack)
            && !origin.is_enabled(&settings)
        {
            let duration_ms = (OffsetDateTime::now_utc() - started_at)
                .whole_milliseconds()
                .max(0) as u64;
            return (
                SubActionTelemetry {
                    index: ctx.index,
                    kind: "tts.speak.text".to_owned(),
                    started_at,
                    duration_ms,
                    outcome: SubActionOutcome::Skipped(origin.disabled_reason().to_owned()),
                },
                None,
            );
        }

        let outcome = match self.speak.speak(text, voice_alias).await {
            Ok(()) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        };

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "tts.speak.text".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            None,
        )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

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

    /// Records whether the dispatcher was reached, so gate tests can assert a
    /// gated-off speak never touches the queue.
    struct RecordingSpeaker {
        called: Arc<AtomicBool>,
    }

    #[async_trait]
    impl SpeakDispatcher for RecordingSpeaker {
        async fn speak(
            &self,
            _text: String,
            _voice_id_override: Option<String>,
        ) -> Result<(), SpeakDispatchError> {
            self.called.store(true, Ordering::SeqCst);
            Ok(())
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
        let runner = SpeakRunner::new(
            Arc::new(OkSpeaker),
            TtsTriggerSettingsHandle::new(TtsTriggerSettings::default()),
        );
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let (telemetry, updated) = runner.execute(&config_with_text("Hello chat!"), &ctx).await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
        assert!(updated.is_none());
    }

    #[tokio::test]
    async fn failure_path() {
        let runner = SpeakRunner::new(
            Arc::new(FailSpeaker),
            TtsTriggerSettingsHandle::new(TtsTriggerSettings::default()),
        );
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let (telemetry, _) = runner.execute(&config_with_text("Hello!"), &ctx).await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
    }

    #[tokio::test]
    async fn empty_text_is_forwarded() {
        let runner = SpeakRunner::new(
            Arc::new(OkSpeaker),
            TtsTriggerSettingsHandle::new(TtsTriggerSettings::default()),
        );
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
        let runner = SpeakRunner::new(
            Arc::new(CapturingSpeaker {
                captured: Arc::clone(&captured),
            }),
            TtsTriggerSettingsHandle::new(TtsTriggerSettings::default()),
        );

        let stack = ArgStack::new().set("user".to_owned(), Variant::String("Alice".to_owned()));
        let ctx = make_ctx(&stack);

        runner
            .execute(&config_with_text("Welcome %user%!"), &ctx)
            .await;

        assert_eq!(*captured.lock().unwrap(), "Welcome Alice!");
    }

    #[test]
    fn validate_config_rejects_missing_text() {
        let runner = SpeakRunner::new(
            Arc::new(OkSpeaker),
            TtsTriggerSettingsHandle::new(TtsTriggerSettings::default()),
        );
        assert!(runner.validate_config(&SubActionConfig::new()).is_err());
    }

    #[test]
    fn validate_config_accepts_nonempty_text() {
        let runner = SpeakRunner::new(
            Arc::new(OkSpeaker),
            TtsTriggerSettingsHandle::new(TtsTriggerSettings::default()),
        );
        assert!(runner.validate_config(&config_with_text("hello")).is_ok());
    }

    #[test]
    fn classify_origin_prefers_specific_source_over_base_chat_keys() {
        // Reward/cheer/sub arg-stacks ALSO carry the base chat `message_text`;
        // the classifier must pick the specific source, not fall through to
        // Command. Locks the precedence chain reward > bits > sub > command, and
        // None when no source key is present. A reversed check order (Command
        // first) would misclassify every reward/cheer/sub speak.
        fn stack(keys: &[&str]) -> ArgStack {
            let mut s = ArgStack::new();
            for k in keys {
                s = s.set((*k).to_owned(), Variant::Int(1));
            }
            s
        }
        fn tag(origin: Option<SpeakOrigin>) -> &'static str {
            match origin {
                Some(SpeakOrigin::ChannelPoints) => "channel_points",
                Some(SpeakOrigin::Bits) => "bits",
                Some(SpeakOrigin::Sub) => "sub",
                Some(SpeakOrigin::Command) => "command",
                None => "none",
            }
        }
        let cases: &[(&[&str], &str)] = &[
            (&["reward.id", "message_text"], "channel_points"),
            (&["reward.id", "cheer.bits"], "channel_points"),
            (&["cheer.bits", "message_text"], "bits"),
            (&["cheer.bits", "sub_tier"], "bits"),
            (&["sub_tier", "message_text"], "sub"),
            (&["message_text"], "command"),
            (&[], "none"),
        ];
        for (keys, expected) in cases {
            assert_eq!(
                tag(classify_origin(&stack(keys))),
                *expected,
                "keys={keys:?}"
            );
        }
    }

    #[test]
    fn is_enabled_reads_only_the_matching_toggle_per_origin() {
        // Guards the origin->field mapping against a copy-paste swap. One-hot-true
        // rows catch a read misdirected to another (false) field; one-cold rows
        // (self false, rest true) catch an always-true short-circuit.
        fn settings(
            command: bool,
            channel_points: bool,
            bits: bool,
            sub: bool,
        ) -> TtsTriggerSettings {
            TtsTriggerSettings {
                command_enabled: command,
                channel_points_enabled: channel_points,
                bits_enabled: bits,
                sub_messages_enabled: sub,
                ..TtsTriggerSettings::default()
            }
        }
        let cases = [
            (
                SpeakOrigin::Command,
                settings(true, false, false, false),
                true,
            ),
            (
                SpeakOrigin::ChannelPoints,
                settings(false, true, false, false),
                true,
            ),
            (SpeakOrigin::Bits, settings(false, false, true, false), true),
            (SpeakOrigin::Sub, settings(false, false, false, true), true),
            (
                SpeakOrigin::Command,
                settings(false, true, true, true),
                false,
            ),
            (
                SpeakOrigin::ChannelPoints,
                settings(true, false, true, true),
                false,
            ),
            (SpeakOrigin::Bits, settings(true, true, false, true), false),
            (SpeakOrigin::Sub, settings(true, true, true, false), false),
        ];
        for (i, (origin, settings, expected)) in cases.into_iter().enumerate() {
            assert_eq!(origin.is_enabled(&settings), expected, "case {i}");
        }
    }

    #[tokio::test]
    async fn speak_gated_off_by_source_toggle_is_skipped_and_not_dispatched() {
        let called = Arc::new(AtomicBool::new(false));
        let settings = TtsTriggerSettings {
            bits_enabled: false,
            ..TtsTriggerSettings::default()
        };
        let runner = SpeakRunner::new(
            Arc::new(RecordingSpeaker {
                called: Arc::clone(&called),
            }),
            TtsTriggerSettingsHandle::new(settings),
        );
        let stack = ArgStack::new().set("cheer.bits".to_owned(), Variant::Int(100));
        let ctx = make_ctx(&stack);
        let (telemetry, _) = runner.execute(&config_with_text("Thanks!"), &ctx).await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Skipped(_)));
        assert!(
            !called.load(Ordering::SeqCst),
            "dispatcher must not be reached when the source toggle is off"
        );
    }

    #[tokio::test]
    async fn speak_dispatched_when_source_toggle_is_on() {
        let called = Arc::new(AtomicBool::new(false));
        // Default leaves bits_enabled = true.
        let runner = SpeakRunner::new(
            Arc::new(RecordingSpeaker {
                called: Arc::clone(&called),
            }),
            TtsTriggerSettingsHandle::new(TtsTriggerSettings::default()),
        );
        let stack = ArgStack::new().set("cheer.bits".to_owned(), Variant::Int(100));
        let ctx = make_ctx(&stack);
        let (telemetry, _) = runner.execute(&config_with_text("Thanks!"), &ctx).await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
        assert!(called.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn speak_without_source_keys_is_dispatched_even_with_all_toggles_off() {
        let called = Arc::new(AtomicBool::new(false));
        let settings = TtsTriggerSettings {
            command_enabled: false,
            channel_points_enabled: false,
            bits_enabled: false,
            sub_messages_enabled: false,
            ..TtsTriggerSettings::default()
        };
        let runner = SpeakRunner::new(
            Arc::new(RecordingSpeaker {
                called: Arc::clone(&called),
            }),
            TtsTriggerSettingsHandle::new(settings),
        );
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let (telemetry, _) = runner
            .execute(&config_with_text("script speak"), &ctx)
            .await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
        assert!(called.load(Ordering::SeqCst));
    }
}

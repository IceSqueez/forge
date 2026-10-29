mod alias_set;
mod alias_switch;
mod queue_clear;
mod queue_pause;
mod queue_resume;
mod queue_skip;
mod speak;
mod speak_stop;
mod speak_with_engine;

pub use alias_set::AliasSetRunner;
pub use alias_switch::AliasSwitchRunner;
pub use queue_clear::QueueClearRunner;
pub use queue_pause::QueuePauseRunner;
pub use queue_resume::QueueResumeRunner;
pub use queue_skip::QueueSkipRunner;
pub use speak::{SPEAK_KIND_ID, SpeakRunner};
pub use speak_stop::SpeakStopRunner;
pub use speak_with_engine::SpeakWithEngineRunner;

use std::sync::Arc;

use forge_registry::{
    ConfigProblem, ConfigSpec, DependentSelectField, DynamicSelectField, RegistryError,
    SubActionRegistry, TextEditor, TextField, ToggleField, TypedField,
};
use forge_types::{ArgStack, SubActionConfig};

use crate::speak_dispatcher::SpeakDispatcher;
use crate::twitch_emote_lexicon::TwitchEmoteLexicon;

const ENGINE_ID_KEY: &str = "engine_id";
pub const ENGINE_OPTIONS_KEY: &str = "tts.engine_ids";
pub const VOICE_OPTIONS_PREFIX: &str = "tts.voices";
pub const SPEECH_TEXT_KEY: &str = "text";

const ALIAS_NAME: TextField = TextField {
    key: "alias_name",
    label: "Alias name",
    editor: TextEditor::Line {
        placeholder: "e.g. %user% or viewer id",
    },
    default: "",
    required: true,
};

const ENGINE: DynamicSelectField<String> =
    DynamicSelectField::new(ENGINE_ID_KEY, "Engine ID", ENGINE_OPTIONS_KEY);

const VOICE: DependentSelectField = DependentSelectField {
    key: "voice_id",
    label: "Voice ID",
    options_prefix: VOICE_OPTIONS_PREFIX,
    depends_on: ENGINE_ID_KEY,
};

const ALIAS_SPEC: ConfigSpec = ConfigSpec::new(&[&ALIAS_NAME, &ENGINE, &VOICE]);

const SPEECH_TEXT: TextField = TextField {
    key: SPEECH_TEXT_KEY,
    label: "Text",
    editor: TextEditor::Area,
    default: "",
    required: true,
};

const WAIT_FOR_COMPLETION: ToggleField = ToggleField {
    key: "wait_for_completion",
    label: "Wait for completion",
    default: true,
};

struct AliasTarget {
    alias_name: String,
    engine_id: String,
    voice_id: String,
}

fn read_alias(config: &SubActionConfig, args: &ArgStack) -> Result<AliasTarget, ConfigProblem> {
    Ok(AliasTarget {
        alias_name: ALIAS_NAME.read(config, args)?,
        engine_id: ENGINE.read(config, args)?,
        voice_id: VOICE.read(config, args)?,
    })
}

pub fn register_speech_sub_actions(
    reg: &mut SubActionRegistry,
    speak: Arc<dyn SpeakDispatcher>,
    reward_emotes: TwitchEmoteLexicon,
) -> Result<(), RegistryError> {
    reg.register(Box::new(
        SpeakRunner::new(Arc::clone(&speak)).with_reward_emotes(reward_emotes),
    ))?;
    reg.register(Box::new(SpeakWithEngineRunner::new(Arc::clone(&speak))))?;
    reg.register(Box::new(SpeakStopRunner::new(Arc::clone(&speak))))?;
    reg.register(Box::new(QueuePauseRunner::new(Arc::clone(&speak))))?;
    reg.register(Box::new(QueueResumeRunner::new(Arc::clone(&speak))))?;
    reg.register(Box::new(QueueClearRunner::new(Arc::clone(&speak))))?;
    reg.register(Box::new(QueueSkipRunner::new(Arc::clone(&speak))))?;
    reg.register(Box::new(AliasSetRunner::new(Arc::clone(&speak))))?;
    reg.register(Box::new(AliasSwitchRunner::new(speak)))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use forge_events::{Event, EventPublisher};
    use forge_registry::{FormField, RunContext, SubActionRunner};
    use forge_types::{ArgStack, EventId, SubActionConfig, SubActionOutcome, Variant};

    use super::*;
    use crate::speak_dispatcher::{
        SpeakDispatchError, SpeakDispatcher, SpeakingViewer, SpeechOrigin,
    };

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum DispatchCall {
        SpeakWithEngine {
            text: String,
            engine_id: String,
            origin: SpeechOrigin,
        },
        StopCurrent,
        Pause,
        Resume,
        SkipCurrent,
        ClearKeepCurrent,
        AliasSet {
            viewer_id: String,
            viewer_name: String,
            engine_id: String,
            voice_id: String,
        },
        AliasSwitch {
            viewer_id: String,
            engine_id: String,
            voice_id: String,
        },
    }

    struct RecordingDispatcher {
        calls: Mutex<Vec<DispatchCall>>,
        fail: bool,
    }

    impl RecordingDispatcher {
        fn ok() -> Arc<Self> {
            Arc::new(Self {
                calls: Mutex::new(Vec::new()),
                fail: false,
            })
        }

        fn failing() -> Arc<Self> {
            Arc::new(Self {
                calls: Mutex::new(Vec::new()),
                fail: true,
            })
        }

        fn record(&self, call: DispatchCall) -> Result<(), SpeakDispatchError> {
            self.calls.lock().unwrap().push(call);
            if self.fail {
                Err(SpeakDispatchError::Dispatch("boom".to_owned()))
            } else {
                Ok(())
            }
        }

        fn calls(&self) -> Vec<DispatchCall> {
            self.calls.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl SpeakDispatcher for RecordingDispatcher {
        async fn speak(
            &self,
            _text: String,
            _voice_id_override: Option<String>,
            _origin: SpeechOrigin,
        ) -> Result<(), SpeakDispatchError> {
            Ok(())
        }

        async fn speak_with_engine(
            &self,
            text: String,
            engine_id: String,
            origin: SpeechOrigin,
        ) -> Result<(), SpeakDispatchError> {
            self.record(DispatchCall::SpeakWithEngine {
                text,
                engine_id,
                origin,
            })
        }

        async fn stop_current(&self) -> Result<(), SpeakDispatchError> {
            self.record(DispatchCall::StopCurrent)
        }

        async fn pause(&self) -> Result<(), SpeakDispatchError> {
            self.record(DispatchCall::Pause)
        }

        async fn resume(&self) -> Result<(), SpeakDispatchError> {
            self.record(DispatchCall::Resume)
        }

        async fn skip_current(&self) -> Result<(), SpeakDispatchError> {
            self.record(DispatchCall::SkipCurrent)
        }

        async fn clear_keep_current(&self) -> Result<(), SpeakDispatchError> {
            self.record(DispatchCall::ClearKeepCurrent)
        }

        async fn alias_set(
            &self,
            viewer_id: String,
            viewer_name: String,
            engine_id: String,
            voice_id: String,
        ) -> Result<(), SpeakDispatchError> {
            self.record(DispatchCall::AliasSet {
                viewer_id,
                viewer_name,
                engine_id,
                voice_id,
            })
        }

        async fn alias_switch(
            &self,
            viewer_id: String,
            engine_id: String,
            voice_id: String,
        ) -> Result<(), SpeakDispatchError> {
            self.record(DispatchCall::AliasSwitch {
                viewer_id,
                engine_id,
                voice_id,
            })
        }
    }

    struct NullPublisher;

    impl EventPublisher for NullPublisher {
        fn publish(&self, _event: Event) {}
    }

    fn make_ctx(stack: &ArgStack) -> RunContext<'_> {
        RunContext::leaf(stack, 0, EventId::new(), &NullPublisher)
    }

    fn config(pairs: &[(&str, Variant)]) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        for (k, v) in pairs {
            cfg.insert((*k).to_owned(), v.clone());
        }
        cfg
    }

    #[tokio::test]
    async fn each_control_runner_forwards_to_its_dispatcher_method() {
        let disp = RecordingDispatcher::ok();
        let cases: Vec<(Box<dyn SubActionRunner>, DispatchCall)> = vec![
            (
                Box::new(SpeakStopRunner::new(disp.clone())),
                DispatchCall::StopCurrent,
            ),
            (
                Box::new(QueuePauseRunner::new(disp.clone())),
                DispatchCall::Pause,
            ),
            (
                Box::new(QueueResumeRunner::new(disp.clone())),
                DispatchCall::Resume,
            ),
            (
                Box::new(QueueSkipRunner::new(disp.clone())),
                DispatchCall::SkipCurrent,
            ),
        ];

        let stack = ArgStack::new();
        for (runner, expected) in cases {
            let before = disp.calls().len();
            let ctx = make_ctx(&stack);
            let (telemetry, updated) = runner.execute(&runner.default_config(), &ctx).await;

            assert!(
                matches!(telemetry.outcome, SubActionOutcome::Success),
                "{} should succeed",
                runner.id()
            );
            assert!(
                updated.is_none(),
                "{} must not emit output vars",
                runner.id()
            );

            let after = disp.calls();
            assert_eq!(
                after.len(),
                before + 1,
                "{} should make exactly one dispatch call",
                runner.id()
            );
            assert_eq!(
                after[before],
                expected,
                "{} forwarded to the wrong dispatcher method",
                runner.id()
            );
        }
    }

    #[tokio::test]
    async fn speak_with_engine_marshals_text_engine_and_the_chatting_viewer() {
        let disp = RecordingDispatcher::ok();
        let runner = SpeakWithEngineRunner::new(disp.clone());
        let stack = ArgStack::new()
            .set("user".to_owned(), Variant::String("Bob".to_owned()))
            .set("user_id".to_owned(), Variant::String("4411".to_owned()))
            .set("user_name".to_owned(), Variant::String("Bob".to_owned()))
            .set(
                "user_platform".to_owned(),
                Variant::String("kick".to_owned()),
            );
        let ctx = make_ctx(&stack);

        let cfg = config(&[
            ("text", Variant::String("Hi %user%".to_owned())),
            ("engine_id", Variant::String("azure".to_owned())),
        ]);
        let (telemetry, updated) = runner.execute(&cfg, &ctx).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
        assert!(updated.is_none());
        assert_eq!(
            disp.calls(),
            vec![DispatchCall::SpeakWithEngine {
                text: "Hi Bob".to_owned(),
                engine_id: "azure".to_owned(),
                origin: SpeechOrigin {
                    viewer: Some(SpeakingViewer {
                        platform: "kick".to_owned(),
                        id: "4411".to_owned(),
                        name: "Bob".to_owned(),
                    }),
                    caused_by: Some(ctx.parent_event_id),
                    ..Default::default()
                },
            }]
        );
    }

    #[tokio::test]
    async fn alias_set_double_maps_alias_name_to_viewer_id_and_name() {
        let disp = RecordingDispatcher::ok();
        let runner = AliasSetRunner::new(disp.clone());
        let stack = ArgStack::new().set("user".to_owned(), Variant::String("Carol".to_owned()));
        let ctx = make_ctx(&stack);

        let cfg = config(&[
            ("alias_name", Variant::String("%user%".to_owned())),
            ("engine_id", Variant::String("piper".to_owned())),
            ("voice_id", Variant::String("en_US-amy-medium".to_owned())),
        ]);
        let (telemetry, updated) = runner.execute(&cfg, &ctx).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
        assert!(updated.is_none());
        assert_eq!(
            disp.calls(),
            vec![DispatchCall::AliasSet {
                viewer_id: "Carol".to_owned(),
                viewer_name: "Carol".to_owned(),
                engine_id: "piper".to_owned(),
                voice_id: "en_US-amy-medium".to_owned(),
            }]
        );
    }

    #[tokio::test]
    async fn alias_switch_maps_alias_name_to_viewer_id_with_engine_and_voice() {
        let disp = RecordingDispatcher::ok();
        let runner = AliasSwitchRunner::new(disp.clone());
        let stack = ArgStack::new().set("user".to_owned(), Variant::String("Dave".to_owned()));
        let ctx = make_ctx(&stack);

        let cfg = config(&[
            ("alias_name", Variant::String("%user%".to_owned())),
            ("engine_id", Variant::String("piper".to_owned())),
            ("voice_id", Variant::String("en_GB-alan-low".to_owned())),
        ]);
        let (telemetry, updated) = runner.execute(&cfg, &ctx).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
        assert!(updated.is_none());
        assert_eq!(
            disp.calls(),
            vec![DispatchCall::AliasSwitch {
                viewer_id: "Dave".to_owned(),
                engine_id: "piper".to_owned(),
                voice_id: "en_GB-alan-low".to_owned(),
            }]
        );
    }

    #[tokio::test]
    async fn alias_runners_key_the_alias_by_the_triggering_platform() {
        let disp = RecordingDispatcher::ok();
        let stack = ArgStack::new()
            .set("user".to_owned(), Variant::String("Erin".to_owned()))
            .set(
                "user_platform".to_owned(),
                Variant::String("twitch".to_owned()),
            );
        let cfg = config(&[
            ("alias_name", Variant::String("%user%".to_owned())),
            ("engine_id", Variant::String("piper".to_owned())),
            ("voice_id", Variant::String("en_US-amy-medium".to_owned())),
        ]);
        let runners: [Box<dyn SubActionRunner>; 2] = [
            Box::new(AliasSetRunner::new(disp.clone())),
            Box::new(AliasSwitchRunner::new(disp.clone())),
        ];

        for runner in runners {
            let ctx = make_ctx(&stack);
            runner.execute(&cfg, &ctx).await;
        }

        assert_eq!(
            disp.calls(),
            vec![
                DispatchCall::AliasSet {
                    viewer_id: "twitch:Erin".to_owned(),
                    viewer_name: "Erin".to_owned(),
                    engine_id: "piper".to_owned(),
                    voice_id: "en_US-amy-medium".to_owned(),
                },
                DispatchCall::AliasSwitch {
                    viewer_id: "twitch:Erin".to_owned(),
                    engine_id: "piper".to_owned(),
                    voice_id: "en_US-amy-medium".to_owned(),
                },
            ]
        );
    }

    #[tokio::test]
    async fn queue_clear_keep_current_makes_single_clear_call() {
        let disp = RecordingDispatcher::ok();
        let runner = QueueClearRunner::new(disp.clone());
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);

        let (telemetry, _) = runner.execute(&runner.default_config(), &ctx).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
        assert_eq!(disp.calls(), vec![DispatchCall::ClearKeepCurrent]);
    }

    #[tokio::test]
    async fn queue_clear_without_keep_current_stops_then_clears_in_order() {
        let disp = RecordingDispatcher::ok();
        let runner = QueueClearRunner::new(disp.clone());
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);

        let cfg = config(&[("keep_current", Variant::Bool(false))]);
        let (telemetry, _) = runner.execute(&cfg, &ctx).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
        assert_eq!(
            disp.calls(),
            vec![DispatchCall::StopCurrent, DispatchCall::ClearKeepCurrent]
        );
    }

    #[tokio::test]
    async fn queue_clear_stop_failure_short_circuits_before_clear() {
        let disp = RecordingDispatcher::failing();
        let runner = QueueClearRunner::new(disp.clone());
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);

        let cfg = config(&[("keep_current", Variant::Bool(false))]);
        let (telemetry, _) = runner.execute(&cfg, &ctx).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert_eq!(disp.calls(), vec![DispatchCall::StopCurrent]);
    }

    #[tokio::test]
    async fn speak_with_engine_dispatch_error_yields_failed_with_message() {
        let disp = RecordingDispatcher::failing();
        let runner = SpeakWithEngineRunner::new(disp.clone());
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);

        let cfg = config(&[
            ("text", Variant::String("hi".to_owned())),
            ("engine_id", Variant::String("piper".to_owned())),
        ]);
        let (telemetry, _) = runner.execute(&cfg, &ctx).await;

        assert!(
            matches!(&telemetry.outcome, SubActionOutcome::Failed(msg) if msg == "boom"),
            "expected Failed(\"boom\"), got {:?}",
            telemetry.outcome
        );
    }

    #[tokio::test]
    async fn queue_pause_dispatch_error_yields_failed() {
        let disp = RecordingDispatcher::failing();
        let runner = QueuePauseRunner::new(disp.clone());
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);

        let (telemetry, _) = runner.execute(&runner.default_config(), &ctx).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
    }

    fn speech_registry() -> forge_registry::SubActionRegistry {
        let mut reg = forge_registry::SubActionRegistry::new();
        register_speech_sub_actions(
            &mut reg,
            RecordingDispatcher::ok(),
            crate::TwitchEmoteLexicon::default(),
        )
        .unwrap();
        reg
    }

    #[test]
    fn a_dependent_field_names_a_sibling_of_the_same_schema_as_its_source() {
        let reg = speech_registry();
        let mut checked = 0;

        for runner in reg.all() {
            let fields = runner.config_fields();
            let keys: Vec<&str> = fields.iter().map(FormField::key).collect();
            for field in &fields {
                let FormField::DependentSelect {
                    key, depends_on, ..
                } = field
                else {
                    continue;
                };
                assert!(
                    keys.contains(depends_on),
                    "{}: field {key:?} depends on {depends_on:?}, which no field of its own schema declares",
                    runner.id()
                );
                assert!(
                    runner.default_config().contains_key(*depends_on),
                    "{}: {depends_on:?} drives {key:?} but has no default slot to read a value from",
                    runner.id()
                );
                checked += 1;
            }
        }

        assert!(
            checked >= 2,
            "expected the alias runners to declare dependent fields, found {checked}"
        );
    }

    #[test]
    fn alias_runners_pin_the_option_map_keys_the_voice_dropdown_is_fed_under() {
        let reg = speech_registry();

        for kind_id in ["tts.alias.set", "tts.alias.switch_active"] {
            let runner = reg
                .get(kind_id)
                .unwrap_or_else(|| panic!("{kind_id} is not registered"));
            let fields = runner.config_fields();

            let engine = fields
                .iter()
                .find(|f| f.key() == "engine_id")
                .unwrap_or_else(|| panic!("{kind_id} declares no engine_id field"));
            assert!(
                matches!(
                    engine,
                    FormField::DynamicSelect { options_key, .. } if *options_key == "tts.engine_ids"
                ),
                "{kind_id}: engine_id must read the engine list under \"tts.engine_ids\", got {engine:?}"
            );

            let voice = fields
                .iter()
                .find(|f| f.key() == "voice_id")
                .unwrap_or_else(|| panic!("{kind_id} declares no voice_id field"));
            assert!(
                matches!(
                    voice,
                    FormField::DependentSelect { options_prefix, depends_on, .. }
                        if *options_prefix == "tts.voices" && *depends_on == "engine_id"
                ),
                "{kind_id}: voice_id must read \"tts.voices.<engine_id>\", got {voice:?}"
            );
        }
    }
}

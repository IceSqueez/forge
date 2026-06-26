mod alias_set;
mod alias_switch;
mod play_sound;
mod queue_clear;
mod queue_pause;
mod queue_resume;
mod queue_skip;
mod set_master_volume;
mod speak;
mod speak_stop;
mod speak_with_engine;
mod stop_all_sounds;
mod stop_sound;

pub use alias_set::AliasSetRunner;
pub use alias_switch::AliasSwitchRunner;
pub use play_sound::PlaySoundRunner;
pub use queue_clear::QueueClearRunner;
pub use queue_pause::QueuePauseRunner;
pub use queue_resume::QueueResumeRunner;
pub use queue_skip::QueueSkipRunner;
pub use set_master_volume::SetMasterVolumeRunner;
pub use speak::SpeakRunner;
pub use speak_stop::SpeakStopRunner;
pub use speak_with_engine::SpeakWithEngineRunner;
pub use stop_all_sounds::StopAllSoundsRunner;
pub use stop_sound::StopSoundRunner;

use std::sync::Arc;

use forge_registry::{RegistryError, SubActionRegistry};

use crate::sound_player::SoundPlayer;
use crate::speak_dispatcher::SpeakDispatcher;

pub fn register_audio_sub_actions(
    reg: &mut SubActionRegistry,
    sound_player: Arc<dyn SoundPlayer>,
    speak: Arc<dyn SpeakDispatcher>,
) -> Result<(), RegistryError> {
    reg.register(Box::new(PlaySoundRunner::new(Arc::clone(&sound_player))))?;
    reg.register(Box::new(StopSoundRunner::new(Arc::clone(&sound_player))))?;
    reg.register(Box::new(StopAllSoundsRunner::new(Arc::clone(
        &sound_player,
    ))))?;
    reg.register(Box::new(SetMasterVolumeRunner::new(sound_player)))?;
    reg.register(Box::new(SpeakRunner::new(Arc::clone(&speak))))?;
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
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use forge_events::{Event, EventPublisher};
    use forge_registry::{RunContext, SubActionRunner};
    use forge_types::{ArgStack, EventId, SubActionConfig, SubActionOutcome, Variant};

    use super::*;
    use crate::speak_dispatcher::{SpeakDispatchError, SpeakDispatcher};

    /// One recorded dispatcher invocation. The variant pins WHICH method ran;
    /// the fields pin the marshaled arguments. A runner wired to the wrong
    /// dispatcher method records the wrong variant and fails the assertion.
    #[derive(Debug, Clone, PartialEq, Eq)]
    enum DispatchCall {
        SpeakWithEngine {
            text: String,
            engine_id: String,
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

    /// Capturing test double for `SpeakDispatcher`. Records every call in order;
    /// when `fail` is set, each method records THEN returns a dispatch error so
    /// the runner's error path can be exercised without real TTS.
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
        ) -> Result<(), SpeakDispatchError> {
            // Not exercised by these runners; SpeakRunner has its own tests.
            Ok(())
        }

        async fn speak_with_engine(
            &self,
            text: String,
            engine_id: String,
        ) -> Result<(), SpeakDispatchError> {
            self.record(DispatchCall::SpeakWithEngine { text, engine_id })
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
        RunContext {
            arg_stack: stack,
            index: 0,
            parent_event_id: EventId::new(),
            publisher: &NullPublisher,
        }
    }

    fn config(pairs: &[(&str, Variant)]) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        for (k, v) in pairs {
            cfg.insert((*k).to_owned(), v.clone());
        }
        cfg
    }

    /// The four argument-less control runners each forward to exactly ONE
    /// dispatcher method. Asserting the recorded variant is swap-resistant: a
    /// runner mistakenly wired to `resume` instead of `pause` records `Resume`
    /// and fails here. Also pins success outcome + no output ArgStack.
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
    async fn speak_with_engine_marshals_interpolated_text_and_engine() {
        let disp = RecordingDispatcher::ok();
        let runner = SpeakWithEngineRunner::new(disp.clone());
        let stack = ArgStack::new().set("user".to_owned(), Variant::String("Bob".to_owned()));
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
            }]
        );
    }

    /// `alias_set` exposes one `alias_name` field that the runner double-maps to
    /// BOTH viewer_id (identity key) and viewer_name (display), alongside engine
    /// and voice. Interpolation applies before the split.
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

    /// `alias_switch` is a single-map (viewer_id only, no viewer_name) — the
    /// contrast with `alias_set` is load-bearing.
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
    async fn queue_clear_keep_current_makes_single_clear_call() {
        let disp = RecordingDispatcher::ok();
        let runner = QueueClearRunner::new(disp.clone());
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);

        // default_config sets keep_current = true.
        let (telemetry, _) = runner.execute(&runner.default_config(), &ctx).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
        assert_eq!(disp.calls(), vec![DispatchCall::ClearKeepCurrent]);
    }

    /// With keep_current = false the runner must stop the in-flight item FIRST,
    /// then clear pending — the order is the contract.
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

    /// When the leading stop_current fails, clear_keep_current must NOT run.
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
}

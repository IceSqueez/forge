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
    use forge_types::{
        ArgStack, ClipId, EventId, OutputDevice, SubActionConfig, SubActionOutcome, Variant,
    };

    use super::*;
    use crate::sound_player::{SoundPlayer, SoundPlayerError};
    use crate::speak_dispatcher::{SpeakDispatchError, SpeakDispatcher};

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

    #[derive(Debug, Clone, PartialEq)]
    enum SoundCall {
        Play(ClipId),
        Stop(ClipId),
        StopAll,
        SetMasterVolume(f32),
    }

    struct RecordingSoundPlayer {
        calls: Mutex<Vec<SoundCall>>,
        fail: bool,
    }

    impl RecordingSoundPlayer {
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

        fn record(&self, call: SoundCall) -> Result<(), SoundPlayerError> {
            self.calls.lock().unwrap().push(call);
            if self.fail {
                Err(SoundPlayerError::Play("boom".to_owned()))
            } else {
                Ok(())
            }
        }

        fn calls(&self) -> Vec<SoundCall> {
            self.calls.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl SoundPlayer for RecordingSoundPlayer {
        async fn play(
            &self,
            clip_id: ClipId,
            _override_device: Option<OutputDevice>,
        ) -> Result<(), SoundPlayerError> {
            self.record(SoundCall::Play(clip_id))
        }

        async fn stop(&self, clip_id: ClipId) -> Result<(), SoundPlayerError> {
            self.record(SoundCall::Stop(clip_id))
        }

        async fn stop_all(&self) -> Result<(), SoundPlayerError> {
            self.record(SoundCall::StopAll)
        }

        async fn set_master_volume(&self, gain: f32) -> Result<(), SoundPlayerError> {
            self.record(SoundCall::SetMasterVolume(gain))
        }
    }

    fn clip_config(clip_id: &str) -> SubActionConfig {
        config(&[("clip_id", Variant::String(clip_id.to_owned()))])
    }

    #[tokio::test]
    async fn stop_sound_forwards_resolved_clip_id_to_player_stop() {
        let player = RecordingSoundPlayer::ok();
        let runner = StopSoundRunner::new(player.clone());
        let clip_id = ClipId::new();
        let stack = ArgStack::new().set("clip".to_owned(), Variant::String(clip_id.to_string()));
        let ctx = make_ctx(&stack);

        let (telemetry, updated) = runner.execute(&clip_config("%clip%"), &ctx).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
        assert!(updated.is_none());
        assert_eq!(player.calls(), vec![SoundCall::Stop(clip_id)]);
    }

    #[tokio::test]
    async fn stop_sound_with_empty_clip_id_routes_to_stop_all() {
        let player = RecordingSoundPlayer::ok();
        let runner = StopSoundRunner::new(player.clone());
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);

        let (telemetry, _) = runner.execute(&runner.default_config(), &ctx).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
        assert_eq!(player.calls(), vec![SoundCall::StopAll]);
    }

    #[tokio::test]
    async fn stop_sound_with_invalid_clip_id_fails_without_touching_player() {
        let player = RecordingSoundPlayer::ok();
        let runner = StopSoundRunner::new(player.clone());
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);

        let (telemetry, _) = runner.execute(&clip_config("not-a-ulid"), &ctx).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(player.calls().is_empty());
    }

    #[tokio::test]
    async fn stop_all_sounds_forwards_to_player_stop_all() {
        let player = RecordingSoundPlayer::ok();
        let runner = StopAllSoundsRunner::new(player.clone());
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);

        let (telemetry, updated) = runner.execute(&runner.default_config(), &ctx).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
        assert!(updated.is_none());
        assert_eq!(player.calls(), vec![SoundCall::StopAll]);
    }

    #[tokio::test]
    async fn sound_control_runners_surface_player_error_as_failed() {
        let player = RecordingSoundPlayer::failing();
        let valid_clip = ClipId::new().to_string();
        let runners: Vec<(Box<dyn SubActionRunner>, SubActionConfig)> = vec![
            (
                Box::new(StopSoundRunner::new(player.clone())),
                clip_config(&valid_clip),
            ),
            (
                Box::new(StopAllSoundsRunner::new(player.clone())),
                SubActionConfig::new(),
            ),
            (
                Box::new(SetMasterVolumeRunner::new(player.clone())),
                config(&[("volume_db", Variant::Float(0.0))]),
            ),
        ];

        let stack = ArgStack::new();
        for (runner, cfg) in runners {
            let ctx = make_ctx(&stack);
            let (telemetry, _) = runner.execute(&cfg, &ctx).await;
            assert!(
                matches!(telemetry.outcome, SubActionOutcome::Failed(_)),
                "{} must surface player error as Failed",
                runner.id()
            );
        }
    }

    #[tokio::test]
    async fn set_master_volume_converts_db_to_linear_gain_with_clamping() {
        let cases: Vec<(SubActionConfig, f32)> = vec![
            (config(&[("volume_db", Variant::Int(0))]), 1.0),
            (config(&[("volume_db", Variant::Float(-6.0))]), 0.501_187),
            (config(&[("volume_db", Variant::Int(6))]), 1.995_262),
            (config(&[("volume_db", Variant::Float(-60.0))]), 0.031_623),
            (config(&[("volume_db", Variant::Int(30))]), 1.995_262),
            (SubActionConfig::new(), 1.0),
        ];

        let stack = ArgStack::new();
        for (cfg, expected) in cases {
            let player = RecordingSoundPlayer::ok();
            let runner = SetMasterVolumeRunner::new(player.clone());
            let ctx = make_ctx(&stack);
            let (telemetry, _) = runner.execute(&cfg, &ctx).await;

            assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
            let calls = player.calls();
            assert!(
                matches!(calls.as_slice(), [SoundCall::SetMasterVolume(g)] if (*g - expected).abs() < 1e-4),
                "expected one SetMasterVolume(~{expected}), got {calls:?}"
            );
        }
    }
}

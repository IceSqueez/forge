#![allow(clippy::unwrap_used)]
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use forge_events::{Event, EventPublisher};
use forge_registry::{FormField, RunContext, SubActionRegistry, SubActionRunner};
use forge_soundboard::{
    PlaySoundRunner, SetMasterVolumeRunner, SoundboardControl, SoundboardError,
    StopAllSoundsRunner, StopSoundRunner, register_soundboard_sub_actions,
};
use forge_types::{
    ArgStack, ClipId, EventId, OutputDevice, SubActionConfig, SubActionOutcome, Variant,
};

const PLAY: &str = "soundboard.sound.play";
const STOP: &str = "soundboard.sound.stop";
const STOP_ALL: &str = "soundboard.sound.stop_all";
const SET_MASTER_VOLUME: &str = "soundboard.volume.set_master";
const CLIP_OPTIONS: &str = "soundboard.clip_ids";
const CONTROL_FAILURE: &str = "output device vanished";
const GAIN_TOLERANCE: f32 = 1e-4;

#[derive(Debug, Clone, PartialEq)]
enum ControlCall {
    Play(ClipId),
    Stop(ClipId),
    StopAll,
    SetMasterVolume(f32),
}

struct RecordingControl {
    calls: Mutex<Vec<ControlCall>>,
    fail: bool,
}

impl RecordingControl {
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

    fn record(&self, call: ControlCall) -> Result<(), SoundboardError> {
        self.calls.lock().unwrap().push(call);
        if self.fail {
            Err(SoundboardError::Storage(CONTROL_FAILURE.to_owned()))
        } else {
            Ok(())
        }
    }

    fn calls(&self) -> Vec<ControlCall> {
        self.calls.lock().unwrap().clone()
    }
}

#[async_trait]
impl SoundboardControl for RecordingControl {
    async fn play(
        &self,
        clip_id: ClipId,
        _output_device_override: Option<OutputDevice>,
    ) -> Result<(), SoundboardError> {
        self.record(ControlCall::Play(clip_id))
    }

    async fn stop(&self, clip_id: ClipId) -> Result<(), SoundboardError> {
        self.record(ControlCall::Stop(clip_id))
    }

    async fn stop_all(&self) -> Result<(), SoundboardError> {
        self.record(ControlCall::StopAll)
    }

    async fn set_master_volume(&self, gain: f32) -> Result<(), SoundboardError> {
        self.record(ControlCall::SetMasterVolume(gain))
    }
}

struct NullPublisher;

impl EventPublisher for NullPublisher {
    fn publish(&self, _event: Event) {}
}

fn make_ctx(stack: &ArgStack) -> RunContext<'_> {
    RunContext::leaf(stack, 0, EventId::new(), &NullPublisher)
}

fn clip_config(clip_id: &str) -> SubActionConfig {
    let mut cfg = SubActionConfig::new();
    cfg.insert("clip_id".to_owned(), Variant::String(clip_id.to_owned()));
    cfg
}

fn volume_config(volume_db: Variant) -> SubActionConfig {
    let mut cfg = SubActionConfig::new();
    cfg.insert("volume_db".to_owned(), volume_db);
    cfg
}

fn clip_on_stack(clip: &str) -> ArgStack {
    ArgStack::new().set("clip".to_owned(), Variant::String(clip.to_owned()))
}

fn sound_registry(control: Arc<dyn SoundboardControl>) -> SubActionRegistry {
    let mut reg = SubActionRegistry::new();
    register_soundboard_sub_actions(&mut reg, control).unwrap();
    reg
}

async fn applied_gain(cfg: &SubActionConfig) -> f32 {
    let control = RecordingControl::ok();
    let runner = SetMasterVolumeRunner::new(control.clone());
    let stack = ArgStack::new();

    let (telemetry, _) = runner.execute(cfg, &make_ctx(&stack)).await;

    assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
    let calls = control.calls();
    let gains: Vec<f32> = calls
        .iter()
        .filter_map(|call| match call {
            ControlCall::SetMasterVolume(gain) => Some(*gain),
            _ => None,
        })
        .collect();
    assert!(
        calls.len() == 1 && gains.len() == 1,
        "expected one SetMasterVolume call, got {calls:?}"
    );
    gains.into_iter().next().unwrap()
}

const INVALID_CLIP_IDS: [&str; 3] = [
    "not-a-ulid-a-viewer-typed-this",
    "with\"quote",
    " 01ARZ3NDEKTSV4RRFFQ69G5FAV",
];

#[test]
fn registration_adds_exactly_the_four_sound_runners_owned_by_no_integration() {
    let reg = sound_registry(RecordingControl::ok());

    let ids: BTreeSet<&str> = reg.all().map(|runner| runner.id()).collect();

    assert_eq!(
        ids,
        BTreeSet::from([PLAY, STOP, STOP_ALL, SET_MASTER_VOLUME])
    );
    for id in ids {
        assert!(
            reg.owning_integration(id).is_none(),
            "{id} must stay usable without any integration enabled"
        );
    }
}

#[tokio::test]
async fn every_sound_runner_reports_telemetry_under_its_own_id_and_produces_no_variables() {
    let reg = sound_registry(RecordingControl::ok());
    let stack = ArgStack::new();

    for runner in reg.all() {
        let (telemetry, updated) = runner
            .execute(&runner.default_config(), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.kind, runner.id());
        assert!(updated.is_none(), "{} produced variables", runner.id());
    }
}

#[test]
fn every_sound_form_field_has_a_default_slot_and_every_default_has_a_field() {
    let reg = sound_registry(RecordingControl::ok());

    for runner in reg.all() {
        let field_keys: BTreeSet<&str> =
            runner.config_fields().iter().map(FormField::key).collect();
        let default_keys: BTreeSet<String> = runner.default_config().into_keys().collect();
        let default_keys: BTreeSet<&str> = default_keys.iter().map(String::as_str).collect();

        assert_eq!(field_keys, default_keys, "{}", runner.id());
    }
}

#[test]
fn clip_pickers_read_the_soundboard_clip_list() {
    let reg = sound_registry(RecordingControl::ok());

    for id in [PLAY, STOP] {
        let fields = reg.get(id).unwrap().config_fields();
        assert!(
            matches!(
                fields.as_slice(),
                [FormField::DynamicSelect { key: "clip_id", options_key, .. }] if *options_key == CLIP_OPTIONS
            ),
            "{id}: {fields:?}"
        );
    }
}

#[test]
fn play_validation_requires_a_non_empty_clip() {
    let runner = PlaySoundRunner::new(RecordingControl::ok());

    for (case, cfg, accepted) in [
        ("missing", SubActionConfig::new(), false),
        ("empty", clip_config(""), false),
        ("chosen", clip_config(&ClipId::new().to_string()), true),
    ] {
        assert_eq!(runner.validate_config(&cfg).is_ok(), accepted, "{case}");
    }
}

#[tokio::test]
async fn play_plays_the_clip_its_template_resolves_to() {
    let control = RecordingControl::ok();
    let runner = PlaySoundRunner::new(control.clone());
    let clip_id = ClipId::new();
    let stack = clip_on_stack(&clip_id.to_string());

    let (telemetry, _) = runner
        .execute(&clip_config("%clip%"), &make_ctx(&stack))
        .await;

    assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
    assert_eq!(control.calls(), vec![ControlCall::Play(clip_id)]);
}

#[tokio::test]
async fn play_with_no_resolved_clip_fails_without_playing() {
    let cases = [
        ("default config", clip_config(""), ArgStack::new()),
        ("missing key", SubActionConfig::new(), ArgStack::new()),
        (
            "template resolving to blank",
            clip_config("%clip%"),
            clip_on_stack(""),
        ),
    ];

    for (case, cfg, stack) in cases {
        let control = RecordingControl::ok();
        let runner = PlaySoundRunner::new(control.clone());

        let (telemetry, _) = runner.execute(&cfg, &make_ctx(&stack)).await;

        assert!(
            matches!(telemetry.outcome, SubActionOutcome::Failed(_)),
            "{case}"
        );
        assert!(control.calls().is_empty(), "{case}");
    }
}

#[tokio::test]
async fn play_and_stop_reject_an_invalid_clip_id_without_touching_the_player_or_echoing_it() {
    for bad in INVALID_CLIP_IDS {
        let control = RecordingControl::ok();
        let runners: [Box<dyn SubActionRunner>; 2] = [
            Box::new(PlaySoundRunner::new(control.clone())),
            Box::new(StopSoundRunner::new(control.clone())),
        ];

        for runner in runners {
            let stack = ArgStack::new();
            let (telemetry, _) = runner.execute(&clip_config(bad), &make_ctx(&stack)).await;

            let SubActionOutcome::Failed(message) = telemetry.outcome else {
                panic!(
                    "{}: expected Failed for {bad:?}, got {:?}",
                    runner.id(),
                    telemetry.outcome
                );
            };
            assert!(
                !message.contains(bad),
                "{} echoed {bad:?}: {message}",
                runner.id()
            );
        }
        assert!(control.calls().is_empty(), "{bad:?}");
    }
}

#[tokio::test]
async fn stop_stops_the_clip_its_template_resolves_to() {
    let control = RecordingControl::ok();
    let runner = StopSoundRunner::new(control.clone());
    let clip_id = ClipId::new();
    let stack = clip_on_stack(&clip_id.to_string());

    let (telemetry, _) = runner
        .execute(&clip_config("%clip%"), &make_ctx(&stack))
        .await;

    assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
    assert_eq!(control.calls(), vec![ControlCall::Stop(clip_id)]);
}

#[tokio::test]
async fn stop_with_no_resolved_clip_stops_everything() {
    let cases = [
        ("default config", clip_config(""), ArgStack::new()),
        ("missing key", SubActionConfig::new(), ArgStack::new()),
        (
            "template resolving to blank",
            clip_config("%clip%"),
            clip_on_stack(""),
        ),
    ];

    for (case, cfg, stack) in cases {
        let control = RecordingControl::ok();
        let runner = StopSoundRunner::new(control.clone());

        let (telemetry, _) = runner.execute(&cfg, &make_ctx(&stack)).await;

        assert!(
            matches!(telemetry.outcome, SubActionOutcome::Success),
            "{case}"
        );
        assert_eq!(control.calls(), vec![ControlCall::StopAll], "{case}");
    }
}

#[tokio::test]
async fn stop_all_stops_everything() {
    let control = RecordingControl::ok();
    let runner = StopAllSoundsRunner::new(control.clone());
    let stack = ArgStack::new();

    let (telemetry, _) = runner
        .execute(&SubActionConfig::new(), &make_ctx(&stack))
        .await;

    assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
    assert_eq!(control.calls(), vec![ControlCall::StopAll]);
}

#[tokio::test]
async fn every_sound_runner_fails_with_the_control_error_text() {
    let control = RecordingControl::failing();
    let valid_clip = ClipId::new().to_string();
    let runners: Vec<(Box<dyn SubActionRunner>, SubActionConfig)> = vec![
        (
            Box::new(PlaySoundRunner::new(control.clone())),
            clip_config(&valid_clip),
        ),
        (
            Box::new(StopSoundRunner::new(control.clone())),
            clip_config(&valid_clip),
        ),
        (
            Box::new(StopSoundRunner::new(control.clone())),
            clip_config(""),
        ),
        (
            Box::new(StopAllSoundsRunner::new(control.clone())),
            SubActionConfig::new(),
        ),
        (
            Box::new(SetMasterVolumeRunner::new(control.clone())),
            volume_config(Variant::Float(0.0)),
        ),
    ];
    let expected = SoundboardError::Storage(CONTROL_FAILURE.to_owned()).to_string();

    let stack = ArgStack::new();
    for (runner, cfg) in runners {
        let (telemetry, _) = runner.execute(&cfg, &make_ctx(&stack)).await;

        assert!(
            matches!(&telemetry.outcome, SubActionOutcome::Failed(message) if *message == expected),
            "{}: {:?}",
            runner.id(),
            telemetry.outcome
        );
    }
}

#[tokio::test]
async fn master_volume_converts_decibels_to_linear_gain() {
    let cases = [
        ("unity", volume_config(Variant::Int(0)), 1.0),
        (
            "half amplitude",
            volume_config(Variant::Float(-6.0)),
            0.501_187,
        ),
        ("floor", volume_config(Variant::Float(-30.0)), 0.031_623),
        ("above ceiling", volume_config(Variant::Int(6)), 1.0),
        (
            "below floor",
            volume_config(Variant::Float(-60.0)),
            0.031_623,
        ),
        ("missing field", SubActionConfig::new(), 1.0),
        (
            "non-numeric",
            volume_config(Variant::String("loud".to_owned())),
            1.0,
        ),
    ];

    for (case, cfg, expected) in cases {
        let gain = applied_gain(&cfg).await;

        assert!(
            (gain - expected).abs() < GAIN_TOLERANCE,
            "{case}: expected ~{expected}, got {gain}"
        );
    }
}

#[tokio::test]
async fn master_volume_clamps_exactly_at_the_bounds_its_form_offers() {
    let runner = SetMasterVolumeRunner::new(RecordingControl::ok());
    let (min, max) = match runner.config_fields().as_slice() {
        [
            FormField::Integer {
                key: "volume_db",
                min,
                max,
                ..
            },
        ] => (*min, *max),
        other => panic!("unexpected volume form: {other:?}"),
    };
    let gain_at = |db: i64| applied_gain_owned(volume_config(Variant::Int(db)));

    let (below_min, at_min, above_min) = (
        gain_at(min - 1).await,
        gain_at(min).await,
        gain_at(min + 1).await,
    );
    let (below_max, at_max, above_max) = (
        gain_at(max - 1).await,
        gain_at(max).await,
        gain_at(max + 1).await,
    );

    assert!(
        (below_min - at_min).abs() < GAIN_TOLERANCE && above_min > at_min,
        "floor must sit at the form minimum {min}: {below_min} {at_min} {above_min}"
    );
    assert!(
        (above_max - at_max).abs() < GAIN_TOLERANCE && below_max < at_max,
        "ceiling must sit at the form maximum {max}: {below_max} {at_max} {above_max}"
    );
}

async fn applied_gain_owned(cfg: SubActionConfig) -> f32 {
    applied_gain(&cfg).await
}

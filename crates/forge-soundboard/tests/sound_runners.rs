#![allow(clippy::unwrap_used)]
use forge_events::test_support::NullPublisher;
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use forge_registry::{
    ConfigProblem, ConfigReason, FormField, RegistryError, RunContext, SubActionRegistry,
    SubActionRunner, ValueBounds,
};
use forge_soundboard::{
    PlaySoundRunner, SetMasterVolumeRunner, SoundboardControl, SoundboardError,
    StopAllSoundsRunner, StopSoundRunner, register_soundboard_sub_actions,
};
use forge_types::{
    ArgStack, ClipId, EventId, MessageCode, MessageRef, OutputDevice, SubActionConfig,
    SubActionOutcome, UpstreamDetail, Variant,
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
            Err(SoundboardError::Storage(
                forge_storage::StorageError::Connection {
                    reason: CONTROL_FAILURE.to_owned(),
                },
            ))
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

async fn applied_gain(cfg: &SubActionConfig, stack: &ArgStack) -> f32 {
    let control = RecordingControl::ok();
    let runner = SetMasterVolumeRunner::new(control.clone());

    let (telemetry, _) = runner.execute(cfg, &make_ctx(stack)).await;

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

const INVALID_CLIP_IDS: [&str; 4] = [
    "not-a-ulid-a-viewer-typed-this",
    "with\"quote",
    " 01ARZ3NDEKTSV4RRFFQ69G5FAV",
    "  ",
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
fn clip_validation_requires_a_clip_for_play_and_reads_none_as_all_for_stop() {
    let play = PlaySoundRunner::new(RecordingControl::ok());
    let stop = StopSoundRunner::new(RecordingControl::ok());
    let chosen = ClipId::new().to_string();
    let shape = forge_registry::invalid_id();
    let cases: [(&dyn SubActionRunner, SubActionConfig, Option<ConfigReason>); 10] = [
        (&play, SubActionConfig::new(), Some(ConfigReason::Required)),
        (&play, clip_config(""), Some(ConfigReason::Required)),
        (&play, clip_config("  "), Some(ConfigReason::Required)),
        (&play, clip_config("not-a-ulid"), Some(shape.clone())),
        (&play, clip_config(&chosen), None),
        (&stop, SubActionConfig::new(), None),
        (&stop, clip_config(""), None),
        (&stop, clip_config("  "), Some(shape.clone())),
        (&stop, clip_config("not-a-ulid"), Some(shape)),
        (&stop, clip_config(&chosen), None),
    ];

    for (runner, cfg, expected) in cases {
        let problem = runner
            .validate_config(&cfg)
            .err()
            .map(RegistryError::into_config_problem);

        assert_eq!(
            problem,
            expected.map(|reason| ConfigProblem::field("clip_id", reason)),
            "{} {cfg:?}",
            runner.id()
        );
    }
}

#[tokio::test]
async fn stop_with_a_template_resolving_to_whitespace_fails_instead_of_stopping_everything() {
    let control = RecordingControl::ok();
    let runner = StopSoundRunner::new(control.clone());
    let stack = clip_on_stack("   ");

    let (telemetry, _) = runner
        .execute(&clip_config("%clip%"), &make_ctx(&stack))
        .await;

    assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
    assert!(control.calls().is_empty());
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
                !message.code_or_text().contains(bad),
                "{} echoed {bad:?}: {message:?}",
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
            volume_config(Variant::Int(-6)),
        ),
    ];
    let expected = SubActionOutcome::Failed(
        MessageRef::new(MessageCode::from_static("soundboard", "storage_failed"))
            .with_detail(UpstreamDetail::from_redacted(
                forge_storage::StorageError::Connection {
                    reason: CONTROL_FAILURE.to_owned(),
                }
                .to_string(),
            ))
            .into(),
    );

    let stack = ArgStack::new();
    for (runner, cfg) in runners {
        let (telemetry, _) = runner.execute(&cfg, &make_ctx(&stack)).await;

        assert!(
            telemetry.outcome == expected,
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
        ("half amplitude", volume_config(Variant::Int(-6)), 0.501_187),
        ("floor", volume_config(Variant::Int(-30)), 0.031_623),
        ("missing", SubActionConfig::new(), 1.0),
        (
            "numeric text",
            volume_config(Variant::String("-6".to_owned())),
            0.501_187,
        ),
        (
            "whole float saved by an older version",
            volume_config(Variant::Float(-6.0)),
            0.501_187,
        ),
        (
            "whole float zero saved by an older version",
            volume_config(Variant::Float(0.0)),
            1.0,
        ),
        (
            "whole float floor saved by an older version",
            volume_config(Variant::Float(-30.0)),
            0.031_623,
        ),
    ];

    for (case, cfg, expected) in cases {
        let gain = applied_gain(&cfg, &ArgStack::new()).await;

        assert!(
            (gain - expected).abs() < GAIN_TOLERANCE,
            "{case}: expected ~{expected}, got {gain}"
        );
    }
}

#[tokio::test]
async fn master_volume_resolves_a_template_volume_at_run_time() {
    let cfg = volume_config(Variant::String("%db%".to_owned()));
    let stack = ArgStack::new().set("db".to_owned(), Variant::Int(-6));
    let runner = SetMasterVolumeRunner::new(RecordingControl::ok());

    assert!(runner.validate_config(&cfg).is_ok());
    let gain = applied_gain(&cfg, &stack).await;
    assert!(
        (gain - 0.501_187).abs() < GAIN_TOLERANCE,
        "expected ~0.501187, got {gain}"
    );
}

fn volume_form_bounds() -> Option<(i64, i64)> {
    let runner = SetMasterVolumeRunner::new(RecordingControl::ok());
    match runner.config_fields().as_slice() {
        [
            FormField::Integer {
                key: "volume_db",
                min,
                max,
                ..
            },
        ] => Some((*min, *max)),
        _ => None,
    }
}

#[tokio::test]
async fn master_volume_accepts_exactly_the_bounds_its_form_offers() {
    let (min, max) = volume_form_bounds().unwrap();

    let at_min = applied_gain(&volume_config(Variant::Int(min)), &ArgStack::new()).await;
    let at_max = applied_gain(&volume_config(Variant::Int(max)), &ArgStack::new()).await;

    assert!(
        (at_min - 0.031_623).abs() < GAIN_TOLERANCE && (at_max - 1.0).abs() < GAIN_TOLERANCE,
        "form bounds {min}..{max} applied {at_min}..{at_max}"
    );
}

#[tokio::test]
async fn master_volume_rejects_an_invalid_volume_without_touching_the_player() {
    let (min, max) = volume_form_bounds().unwrap();
    let out_of_range = ConfigReason::OutOfRange(ValueBounds::between(min, max));
    let template_stack = ArgStack::new().set("db".to_owned(), Variant::Int(min - 1));
    let cases = [
        (
            "one below the floor",
            volume_config(Variant::Int(min - 1)),
            out_of_range.clone(),
        ),
        (
            "one above the ceiling",
            volume_config(Variant::Int(max + 1)),
            out_of_range.clone(),
        ),
        (
            "smallest integer",
            volume_config(Variant::Int(i64::MIN)),
            out_of_range.clone(),
        ),
        (
            "largest integer",
            volume_config(Variant::Int(i64::MAX)),
            out_of_range.clone(),
        ),
        (
            "whole float below the floor",
            volume_config(Variant::Float(-31.0)),
            out_of_range.clone(),
        ),
        (
            "numeric text above the ceiling",
            volume_config(Variant::String("1".to_owned())),
            out_of_range.clone(),
        ),
        (
            "template resolving below the floor",
            volume_config(Variant::String("%db%".to_owned())),
            out_of_range,
        ),
        (
            "fractional float",
            volume_config(Variant::Float(-6.5)),
            ConfigReason::NotANumber,
        ),
        (
            "not-a-number float",
            volume_config(Variant::Float(f64::NAN)),
            ConfigReason::NotANumber,
        ),
        (
            "infinite float",
            volume_config(Variant::Float(f64::NEG_INFINITY)),
            ConfigReason::NotANumber,
        ),
        (
            "text",
            volume_config(Variant::String("loud".to_owned())),
            ConfigReason::NotANumber,
        ),
        (
            "bool",
            volume_config(Variant::Bool(true)),
            ConfigReason::NotANumber,
        ),
        (
            "array",
            volume_config(Variant::Array(vec![Variant::Int(-6)])),
            ConfigReason::NotANumber,
        ),
    ];

    for (case, cfg, reason) in cases {
        let control = RecordingControl::ok();
        let runner = SetMasterVolumeRunner::new(control.clone());
        let problem = ConfigProblem::field("volume_db", reason);

        let (telemetry, _) = runner.execute(&cfg, &make_ctx(&template_stack)).await;

        assert_eq!(
            telemetry.outcome,
            SubActionOutcome::Failed(
                problem
                    .message(|keys| forge_types::MessageArg::StepFields {
                        kind: "soundboard.volume.set_master".to_owned(),
                        keys: keys.to_vec(),
                    })
                    .into()
            ),
            "{case}"
        );
        assert!(control.calls().is_empty(), "{case}: {:?}", control.calls());
    }
}

#[test]
fn master_volume_validation_rejects_a_literal_invalid_volume_and_defers_templates() {
    let runner = SetMasterVolumeRunner::new(RecordingControl::ok());
    let cases = [
        (volume_config(Variant::Int(-31)), false),
        (volume_config(Variant::Float(-6.5)), false),
        (volume_config(Variant::Float(-6.0)), true),
        (volume_config(Variant::String("%db%".to_owned())), true),
    ];

    for (cfg, accepted) in cases {
        assert_eq!(runner.validate_config(&cfg).is_ok(), accepted, "{cfg:?}");
    }
}

const STORED_DEFAULT_CONFIGS: [(&str, &str); 4] = [
    (PLAY, r#"{"clip_id":{"type":"string","value":""}}"#),
    (STOP, r#"{"clip_id":{"type":"string","value":""}}"#),
    (STOP_ALL, r#"{}"#),
    (
        SET_MASTER_VOLUME,
        r#"{"volume_db":{"type":"int","value":0}}"#,
    ),
];

#[test]
fn every_runner_seeds_the_same_stored_default_config_as_before_declared_fields() {
    let reg = sound_registry(RecordingControl::ok());

    for (id, stored) in STORED_DEFAULT_CONFIGS {
        let expected: serde_json::Value = serde_json::from_str(stored).unwrap();
        let runner = reg.get(id).unwrap();

        assert_eq!(
            serde_json::to_value(runner.default_config()).unwrap(),
            expected,
            "{id}"
        );
    }
}

#[test]
fn every_default_config_validates_except_play_which_waits_for_a_clip() {
    let reg = sound_registry(RecordingControl::ok());

    let failing: Vec<(String, ConfigProblem)> = reg
        .all()
        .filter_map(|runner| {
            let error = runner.validate_config(&runner.default_config()).err()?;
            Some((runner.id().to_owned(), error.into_config_problem()))
        })
        .collect();

    assert_eq!(
        failing,
        vec![(
            PLAY.to_owned(),
            ConfigProblem::field("clip_id", ConfigReason::Required)
        )]
    );
}

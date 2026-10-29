use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{
    ConfigSpec, IntegerEditor, IntegerField, RunContext, StepTimer, SubActionCategory,
    SubActionRunner, TypedField, ValueBounds,
};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry};

use crate::control::SoundboardControl;
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::VOLUME_SET_MASTER;
const MIN_VOLUME_DB: i64 = -30;
const MAX_VOLUME_DB: i64 = 0;
const UNITY_GAIN_DB: i64 = 0;
const DECADE_BASE: f64 = 10.0;
const AMPLITUDE_DB_PER_DECADE: f64 = 20.0;

const VOLUME_DB: IntegerField = IntegerField {
    key: config_keys::VOLUME_DB,
    label: "Volume (dB)",
    editor: IntegerEditor::Number,
    default: UNITY_GAIN_DB,
    bounds: ValueBounds {
        min: Some(MIN_VOLUME_DB),
        max: Some(MAX_VOLUME_DB),
    },
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&VOLUME_DB]);

fn linear_gain(volume_db: i64) -> f32 {
    DECADE_BASE.powf(volume_db as f64 / AMPLITUDE_DB_PER_DECADE) as f32
}

pub struct SetMasterVolumeRunner {
    control: Arc<dyn SoundboardControl>,
}

impl SetMasterVolumeRunner {
    pub fn new(control: Arc<dyn SoundboardControl>) -> Self {
        Self { control }
    }
}

#[async_trait]
impl SubActionRunner for SetMasterVolumeRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Audio
    }

    fn label(&self) -> &str {
        "Set Master Volume"
    }

    fn summary(&self) -> &str {
        "Set the soundboard master volume applied to every clip"
    }

    fn search_text(&self) -> &str {
        "set master volume soundboard gain decibel db level"
    }

    fn icon_name(&self) -> &str {
        "volume"
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
        let volume_db = match VOLUME_DB.read(config, ctx.arg_stack) {
            Ok(volume_db) => volume_db,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let outcome = SubActionOutcome::from_result(
            &self.control.set_master_volume(linear_gain(volume_db)).await,
        );
        (timer.finish(outcome), None)
    }
}

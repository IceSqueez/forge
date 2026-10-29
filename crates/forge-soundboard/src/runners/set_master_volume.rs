use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{
    ConfigReason, FormField, RegistryError, RunContext, StepTimer, SubActionCategory,
    SubActionRunner,
};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};

use crate::control::SoundboardControl;
use crate::vocabulary::sub_action_ids;

const VOLUME_DB_KEY: &str = "volume_db";
const MIN_VOLUME_DB: i32 = -30;
const MAX_VOLUME_DB: i32 = 0;
const UNITY_GAIN_DB: i32 = 0;
const DECADE_BASE: f64 = 10.0;
const AMPLITUDE_DB_PER_DECADE: f64 = 20.0;

fn read_volume_db(config: &SubActionConfig) -> Result<f64, RegistryError> {
    match config.get(VOLUME_DB_KEY) {
        Some(Variant::Int(db)) => {
            let saturated =
                i32::try_from(*db).unwrap_or(if db.is_negative() { i32::MIN } else { i32::MAX });
            Ok(f64::from(saturated.clamp(MIN_VOLUME_DB, MAX_VOLUME_DB)))
        }
        Some(Variant::Float(db)) => {
            Ok(db.clamp(f64::from(MIN_VOLUME_DB), f64::from(MAX_VOLUME_DB)))
        }
        Some(_) => Err(RegistryError::invalid_field(
            VOLUME_DB_KEY,
            ConfigReason::NotANumber,
        )),
        None => Err(RegistryError::invalid_field(
            VOLUME_DB_KEY,
            ConfigReason::Required,
        )),
    }
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
        sub_action_ids::VOLUME_SET_MASTER
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

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert(
            VOLUME_DB_KEY.to_owned(),
            Variant::Int(i64::from(UNITY_GAIN_DB)),
        );
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Integer {
            key: VOLUME_DB_KEY,
            label: "Volume (dB)",
            min: i64::from(MIN_VOLUME_DB),
            max: i64::from(MAX_VOLUME_DB),
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        read_volume_db(config).map(|_| ())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, sub_action_ids::VOLUME_SET_MASTER);
        let volume_db = match read_volume_db(config) {
            Ok(volume_db) => volume_db,
            Err(e) => return (timer.failed(e.to_string()), None),
        };
        let gain = DECADE_BASE.powf(volume_db / AMPLITUDE_DB_PER_DECADE) as f32;

        let outcome = match self.control.set_master_volume(gain).await {
            Ok(()) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        };
        (timer.finish(outcome), None)
    }
}

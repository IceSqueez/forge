use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::control::SoundboardControl;

const MIN_VOLUME_DB: f64 = -30.0;
const MAX_VOLUME_DB: f64 = 0.0;
const UNITY_GAIN_DB: f64 = 0.0;
const AMPLITUDE_DB_PER_DECADE: f64 = 20.0;

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
        "soundboard.volume.set_master"
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
        cfg.insert("volume_db".to_owned(), Variant::Float(UNITY_GAIN_DB));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Integer {
            key: "volume_db",
            label: "Volume (dB)",
            min: MIN_VOLUME_DB as i64,
            max: MAX_VOLUME_DB as i64,
        }]
    }

    fn validate_config(&self, _config: &SubActionConfig) -> Result<(), RegistryError> {
        Ok(())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();

        let volume_db: f64 = config
            .get("volume_db")
            .and_then(|v| v.as_float().or_else(|| v.as_int().map(|i| i as f64)))
            .unwrap_or(UNITY_GAIN_DB)
            .clamp(MIN_VOLUME_DB, MAX_VOLUME_DB);
        let gain = 10.0_f64.powf(volume_db / AMPLITUDE_DB_PER_DECADE) as f32;

        let outcome = match self.control.set_master_volume(gain).await {
            Ok(()) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        };

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                index: ctx.index,
                kind: "soundboard.volume.set_master".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            None,
        )
    }
}

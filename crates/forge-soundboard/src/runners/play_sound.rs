use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{
    ConfigReason, FormField, RegistryError, RunContext, StepTimer, SubActionCategory,
    SubActionConfigExt, SubActionRunner,
};
use forge_types::{
    ArgStack, ClipId, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant,
};

use crate::control::SoundboardControl;

pub struct PlaySoundRunner {
    control: Arc<dyn SoundboardControl>,
}

impl PlaySoundRunner {
    pub fn new(control: Arc<dyn SoundboardControl>) -> Self {
        Self { control }
    }
}

#[async_trait]
impl SubActionRunner for PlaySoundRunner {
    fn id(&self) -> &str {
        "soundboard.sound.play"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Audio
    }

    fn label(&self) -> &str {
        "Play Sound"
    }

    fn summary(&self) -> &str {
        "Play a soundboard clip on the configured output device"
    }

    fn search_text(&self) -> &str {
        "play sound clip audio soundboard"
    }

    fn icon_name(&self) -> &str {
        "volume"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("clip_id".to_owned(), Variant::String(String::new()));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::DynamicSelect {
            key: "clip_id",
            label: "Clip",
            options_key: "soundboard.clip_ids",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.str_nonempty("clip_id") {
            Some(_) => Ok(()),
            None => Err(RegistryError::invalid_field(
                "clip_id",
                ConfigReason::Required,
            )),
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, "soundboard.sound.play");

        let clip_id_str = config.str("clip_id").unwrap_or_default();
        let interpolated = ctx.arg_stack.interpolate(clip_id_str);

        let parse_result: Result<ClipId, _> = serde_json::from_str(&format!("\"{interpolated}\""));

        let outcome = match parse_result {
            Err(_) => SubActionOutcome::Failed("soundboard.sound.play: invalid clip_id".to_owned()),
            Ok(clip_id) => SubActionOutcome::from_result(&self.control.play(clip_id, None).await),
        };

        (timer.finish(outcome), None)
    }
}

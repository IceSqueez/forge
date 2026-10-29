use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry};
use time::OffsetDateTime;

use crate::control::SoundboardControl;
use crate::vocabulary::sub_action_ids;

pub struct StopAllSoundsRunner {
    control: Arc<dyn SoundboardControl>,
}

impl StopAllSoundsRunner {
    pub fn new(control: Arc<dyn SoundboardControl>) -> Self {
        Self { control }
    }
}

#[async_trait]
impl SubActionRunner for StopAllSoundsRunner {
    fn id(&self) -> &str {
        sub_action_ids::SOUND_STOP_ALL
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Audio
    }

    fn label(&self) -> &str {
        "Stop All Sounds"
    }

    fn summary(&self) -> &str {
        "Stop every soundboard clip currently playing"
    }

    fn search_text(&self) -> &str {
        "stop all sounds clips audio soundboard silence"
    }

    fn icon_name(&self) -> &str {
        "stop-circle"
    }

    fn default_config(&self) -> SubActionConfig {
        SubActionConfig::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![]
    }

    fn validate_config(&self, _config: &SubActionConfig) -> Result<(), RegistryError> {
        Ok(())
    }

    async fn execute(
        &self,
        _config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();

        let outcome = match self.control.stop_all().await {
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
                kind: sub_action_ids::SOUND_STOP_ALL.to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            None,
        )
    }
}

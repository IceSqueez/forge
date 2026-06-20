use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};
use time::OffsetDateTime;

use crate::ObsSink;

pub struct ReplaySaveRunner {
    sink: Arc<dyn ObsSink>,
}

impl ReplaySaveRunner {
    pub fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for ReplaySaveRunner {
    fn id(&self) -> &str {
        "obs.replay.save"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Save Replay Buffer"
    }

    fn summary(&self) -> &str {
        "Saves the current contents of the OBS replay buffer to disk."
    }

    fn search_text(&self) -> &str {
        "obs replay buffer save clip export"
    }

    fn icon_name(&self) -> &str {
        "device-floppy"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::new()
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
        let start = Instant::now();

        let outcome = match self.sink.save_replay_buffer().await {
            Ok(()) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        };

        (
            SubActionTelemetry {
                kind: "obs.replay.save".to_owned(),
                started_at,
                duration_ms: start.elapsed().as_millis() as u64,
                outcome,
                index: ctx.index,
            },
            None,
        )
    }
}

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    FormField, RegistryError, RunContext, SubActionCategory, SubActionConfigExt, SubActionRunner,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::ObsSink;

pub struct MediaRestartRunner {
    sink: Arc<dyn ObsSink>,
}

impl MediaRestartRunner {
    pub fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for MediaRestartRunner {
    fn id(&self) -> &str {
        "obs.media.restart"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Restart Media Source"
    }

    fn summary(&self) -> &str {
        "Restarts playback of an OBS media input from the beginning."
    }

    fn search_text(&self) -> &str {
        "obs media restart replay video source input playback"
    }

    fn icon_name(&self) -> &str {
        "player-track-prev"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([("source".to_owned(), Variant::String(String::new()))])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::DynamicSelect {
            key: "source",
            label: "Media Input",
            options_key: "obs.input_names",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        if matches!(config.get("source"), Some(Variant::String(_))) {
            Ok(())
        } else {
            Err(RegistryError::InvalidConfig(
                "obs.media.restart: 'source' must be a string".to_owned(),
            ))
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let raw_source = config.str("source").unwrap_or_default();
        let source = ctx.arg_stack.interpolate(raw_source);

        let outcome = SubActionOutcome::from_result(&self.sink.restart_media_input(&source).await);

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                kind: "obs.media.restart".to_owned(),
                started_at,
                duration_ms: start.elapsed().as_millis() as u64,
                outcome,
                index: ctx.index,
            },
            None,
        )
    }
}

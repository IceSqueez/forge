use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::ObsSink;

pub struct StreamSetActiveRunner {
    sink: Arc<dyn ObsSink>,
}

impl StreamSetActiveRunner {
    pub fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for StreamSetActiveRunner {
    fn id(&self) -> &str {
        "obs.stream.set_active"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Set Stream Active"
    }

    fn summary(&self) -> &str {
        "Starts or stops the OBS stream output to an explicit state."
    }

    fn search_text(&self) -> &str {
        "obs stream start stop set on off active broadcast"
    }

    fn icon_name(&self) -> &str {
        "broadcast"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([("on".to_owned(), Variant::Bool(true))])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Toggle {
            key: "on",
            label: "Active",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("on") {
            Some(Variant::Bool(_)) => Ok(()),
            _ => Err(RegistryError::InvalidConfig(
                "obs.stream.set_active: 'on' must be a bool".to_owned(),
            )),
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let on = matches!(config.get("on"), Some(Variant::Bool(true)));

        let outcome = if on {
            SubActionOutcome::from_result(&self.sink.start_stream().await)
        } else {
            SubActionOutcome::from_result(&self.sink.stop_stream().await)
        };

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                kind: "obs.stream.set_active".to_owned(),
                started_at,
                duration_ms: start.elapsed().as_millis() as u64,
                outcome,
                index: ctx.index,
            },
            None,
        )
    }
}

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::ObsSink;

pub struct QueryStreamStatusRunner {
    sink: Arc<dyn ObsSink>,
}

impl QueryStreamStatusRunner {
    pub fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for QueryStreamStatusRunner {
    fn id(&self) -> &str {
        "obs.stream.get_status"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Get Streaming Status"
    }

    fn summary(&self) -> &str {
        "Queries OBS for the current stream output state and elapsed duration."
    }

    fn search_text(&self) -> &str {
        "obs stream streaming get status active duration live"
    }

    fn icon_name(&self) -> &str {
        "wifi"
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

        match self.sink.get_stream_status().await {
            Ok(variant) => {
                let mut stack = ArgStack::new();
                if let Variant::Object(ref map) = variant {
                    if let Some(v) = map.get("is_active") {
                        stack = stack.set("obs.stream.is_active".to_owned(), v.clone());
                    }
                    if let Some(v) = map.get("duration_ms") {
                        stack = stack.set("obs.stream.duration_ms".to_owned(), v.clone());
                    }
                }
                (
                    SubActionTelemetry {
                        kind: "obs.stream.get_status".to_owned(),
                        started_at,
                        duration_ms: start.elapsed().as_millis() as u64,
                        outcome: SubActionOutcome::Success,
                        index: ctx.index,
                    },
                    Some(stack),
                )
            }
            Err(e) => (
                SubActionTelemetry {
                    kind: "obs.stream.get_status".to_owned(),
                    started_at,
                    duration_ms: start.elapsed().as_millis() as u64,
                    outcome: SubActionOutcome::Failed(e.to_string()),
                    index: ctx.index,
                },
                None,
            ),
        }
    }
}

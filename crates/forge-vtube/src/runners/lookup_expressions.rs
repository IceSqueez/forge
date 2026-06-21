use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::sink::VTubeSink;

pub struct LookupExpressionsRunner {
    sink: Arc<dyn VTubeSink>,
}

impl LookupExpressionsRunner {
    pub fn new(sink: Arc<dyn VTubeSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for LookupExpressionsRunner {
    fn id(&self) -> &str {
        "vtube.lookup.expressions"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::VTube
    }

    fn label(&self) -> &str {
        "Get Expressions"
    }

    fn summary(&self) -> &str {
        "Queries VTube Studio for the expressions in the current model."
    }

    fn search_text(&self) -> &str {
        "vtube get expressions names active state lookup vts"
    }

    fn icon_name(&self) -> &str {
        "smile"
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

        match self.sink.get_expressions().await {
            Ok(variant) => {
                let mut stack = ArgStack::new();
                if let Variant::Object(ref map) = variant {
                    if let Some(v) = map.get("names") {
                        stack = stack.set("vtube.expressions.names".to_owned(), v.clone());
                    }
                    if let Some(v) = map.get("active") {
                        stack = stack.set("vtube.expressions.active".to_owned(), v.clone());
                    }
                    if let Some(v) = map.get("count") {
                        stack = stack.set("vtube.expressions.count".to_owned(), v.clone());
                    }
                }
                (
                    SubActionTelemetry {
                        kind: "vtube.lookup.expressions".to_owned(),
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
                    kind: "vtube.lookup.expressions".to_owned(),
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

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionTelemetry};
use time::OffsetDateTime;

use super::step_failure;
use crate::ObsSink;
use crate::vocabulary::sub_action_ids;

const KIND_ID: &str = sub_action_ids::STREAMING_STOP;

pub struct StreamStopRunner {
    sink: Arc<dyn ObsSink>,
}

impl StreamStopRunner {
    pub fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for StreamStopRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Stop Stream"
    }

    fn summary(&self) -> &str {
        "Stops OBS streaming output."
    }

    fn search_text(&self) -> &str {
        "obs stream stop streaming end live broadcast"
    }

    fn icon_name(&self) -> &str {
        "player-stop"
    }

    async fn execute(
        &self,
        _config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let outcome = step_failure::outcome(&self.sink.stop_stream().await);

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                kind: KIND_ID.to_owned(),
                started_at,
                duration_ms: start.elapsed().as_millis() as u64,
                outcome,
                index: ctx.index,
            },
            None,
        )
    }
}

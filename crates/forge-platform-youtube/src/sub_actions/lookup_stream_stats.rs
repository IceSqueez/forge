use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::stream_stats::YoutubeStreamStats;

const KIND_ID: &str = "youtube.lookup.stream_stats";

pub struct LookupStreamStatsRunner {
    stats: Arc<YoutubeStreamStats>,
}

impl LookupStreamStatsRunner {
    pub fn new(stats: Arc<YoutubeStreamStats>) -> Self {
        Self { stats }
    }
}

#[async_trait]
impl SubActionRunner for LookupStreamStatsRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::YouTube
    }

    fn label(&self) -> &str {
        "Stream Stats"
    }

    fn summary(&self) -> &str {
        "Fetches live-streaming details for the active YouTube broadcast."
    }

    fn search_text(&self) -> &str {
        "youtube stream stats concurrent viewers start time broadcast live"
    }

    fn icon_name(&self) -> &str {
        "chart-line"
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

        match self.stats.fetch().await {
            Ok(Variant::Object(map)) => {
                let mut stack = ctx.arg_stack.clone();
                for (field, key) in [
                    ("concurrent_viewers", "youtube.stream.concurrent_viewers"),
                    ("actual_start_time", "youtube.stream.actual_start_time"),
                    (
                        "scheduled_start_time",
                        "youtube.stream.scheduled_start_time",
                    ),
                    ("live_chat_id", "youtube.stream.live_chat_id"),
                ] {
                    if let Some(v) = map.get(field) {
                        stack = stack.set(key.to_owned(), v.clone());
                    }
                }
                (
                    SubActionTelemetry {
                        args_in: ::std::collections::BTreeMap::new(),
                        produced: ::std::collections::BTreeMap::new(),
                        kind: KIND_ID.to_owned(),
                        started_at,
                        duration_ms: start.elapsed().as_millis() as u64,
                        outcome: SubActionOutcome::Success,
                        index: ctx.index,
                    },
                    Some(stack),
                )
            }
            Ok(_) => (
                SubActionTelemetry {
                    args_in: ::std::collections::BTreeMap::new(),
                    produced: ::std::collections::BTreeMap::new(),
                    kind: KIND_ID.to_owned(),
                    started_at,
                    duration_ms: start.elapsed().as_millis() as u64,
                    outcome: SubActionOutcome::Failed(
                        "stream stats returned an unexpected shape".to_owned(),
                    ),
                    index: ctx.index,
                },
                None,
            ),
            Err(e) => (
                SubActionTelemetry {
                    args_in: ::std::collections::BTreeMap::new(),
                    produced: ::std::collections::BTreeMap::new(),
                    kind: KIND_ID.to_owned(),
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

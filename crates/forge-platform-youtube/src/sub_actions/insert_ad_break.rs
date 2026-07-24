use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::ad_break::YoutubeAdBreak;

const KIND_ID: &str = "youtube.stream.insert_ad_break";
const MIN_DURATION_SECS: i64 = 1;
const MAX_DURATION_SECS: i64 = 3_600;

pub struct InsertAdBreakRunner {
    ad_break: Arc<YoutubeAdBreak>,
}

impl InsertAdBreakRunner {
    pub fn new(ad_break: Arc<YoutubeAdBreak>) -> Self {
        Self { ad_break }
    }
}

#[async_trait]
impl SubActionRunner for InsertAdBreakRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::YouTube
    }

    fn label(&self) -> &str {
        "Insert Ad Break"
    }

    fn summary(&self) -> &str {
        "Triggers a mid-roll ad cuepoint on the active YouTube broadcast."
    }

    fn search_text(&self) -> &str {
        "youtube ad break cuepoint mid-roll monetization broadcast live"
    }

    fn icon_name(&self) -> &str {
        "player-skip-forward"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([("duration_secs".to_owned(), Variant::Int(30))])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Integer {
            key: "duration_secs",
            label: "Length (seconds)",
            min: MIN_DURATION_SECS,
            max: MAX_DURATION_SECS,
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("duration_secs") {
            Some(Variant::Int(n)) if (MIN_DURATION_SECS..=MAX_DURATION_SECS).contains(n) => Ok(()),
            _ => Err(RegistryError::InvalidConfig(format!(
                "{KIND_ID}: 'duration_secs' must be {MIN_DURATION_SECS}..={MAX_DURATION_SECS}"
            ))),
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let duration_secs = config
            .get("duration_secs")
            .and_then(|v| match v {
                Variant::Int(n) => u32::try_from(*n).ok(),
                _ => None,
            })
            .unwrap_or(30);

        let outcome =
            SubActionOutcome::from_result(&self.ad_break.insert_cuepoint(duration_secs).await);

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

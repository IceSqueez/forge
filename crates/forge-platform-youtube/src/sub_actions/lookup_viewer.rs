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

use crate::channel_lookup::YoutubeChannelLookup;

const KIND_ID: &str = "youtube.lookup.viewer";

pub struct LookupViewerRunner {
    lookup: Arc<YoutubeChannelLookup>,
}

impl LookupViewerRunner {
    pub fn new(lookup: Arc<YoutubeChannelLookup>) -> Self {
        Self { lookup }
    }
}

#[async_trait]
impl SubActionRunner for LookupViewerRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::YouTube
    }

    fn label(&self) -> &str {
        "Lookup Viewer"
    }

    fn summary(&self) -> &str {
        "Looks up a YouTube channel by handle or channel id."
    }

    fn search_text(&self) -> &str {
        "youtube lookup viewer channel handle subscriber stats"
    }

    fn icon_name(&self) -> &str {
        "user"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([("identifier".to_owned(), Variant::String(String::new()))])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Text {
            key: "identifier",
            label: "Channel Handle or ID",
            placeholder: "@handle or UC...",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("identifier") {
            Some(Variant::String(s)) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::InvalidConfig(format!(
                "{KIND_ID}: 'identifier' must be a non-empty string"
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

        let template = config.str("identifier").unwrap_or_default();
        let identifier = ctx.arg_stack.interpolate(template);

        if identifier.is_empty() {
            return (
                SubActionTelemetry {
                    args_in: ::std::collections::BTreeMap::new(),
                    produced: ::std::collections::BTreeMap::new(),
                    kind: KIND_ID.to_owned(),
                    started_at,
                    duration_ms: start.elapsed().as_millis() as u64,
                    outcome: SubActionOutcome::Failed(
                        "identifier is empty after interpolation".to_owned(),
                    ),
                    index: ctx.index,
                },
                None,
            );
        }

        match self.lookup.lookup(&identifier).await {
            Ok(Variant::Object(map)) => {
                let mut stack = ctx.arg_stack.clone();
                for (field, key) in [
                    ("channel_id", "youtube.viewer.channel_id"),
                    ("title", "youtube.viewer.title"),
                    ("subscriber_count", "youtube.viewer.subscriber_count"),
                    ("view_count", "youtube.viewer.view_count"),
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
                        "channel lookup returned an unexpected shape".to_owned(),
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

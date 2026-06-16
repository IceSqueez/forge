use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::stream_metadata::YoutubeStreamMetadata;

const KIND_ID: &str = "youtube.stream.update_title";

pub struct UpdateTitleRunner {
    metadata: Arc<YoutubeStreamMetadata>,
}

impl UpdateTitleRunner {
    pub fn new(metadata: Arc<YoutubeStreamMetadata>) -> Self {
        Self { metadata }
    }
}

#[async_trait]
impl SubActionRunner for UpdateTitleRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::YouTube
    }

    fn label(&self) -> &str {
        "Update Stream Title"
    }

    fn summary(&self) -> &str {
        "Sets the title of the active YouTube broadcast."
    }

    fn search_text(&self) -> &str {
        "youtube stream broadcast title rename metadata"
    }

    fn icon_name(&self) -> &str {
        "edit"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([("title".to_owned(), Variant::String(String::new()))])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Text {
            key: "title",
            label: "Title",
            placeholder: "New stream title",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("title") {
            Some(Variant::String(s)) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::UnknownKindId(format!(
                "{KIND_ID}: 'title' must be a non-empty string"
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

        let template = config
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let title = ctx.arg_stack.interpolate(template);

        let outcome = if title.is_empty() {
            SubActionOutcome::Failed("title is empty after interpolation".to_owned())
        } else {
            match self.metadata.set_title(&title).await {
                Ok(()) => SubActionOutcome::Success,
                Err(e) => SubActionOutcome::Failed(e.to_string()),
            }
        };

        (
            SubActionTelemetry {
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

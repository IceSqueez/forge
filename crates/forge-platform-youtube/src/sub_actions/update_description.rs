use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::stream_metadata::YoutubeStreamMetadata;

const KIND_ID: &str = "youtube.stream.update_description";

pub struct UpdateDescriptionRunner {
    metadata: Arc<YoutubeStreamMetadata>,
}

impl UpdateDescriptionRunner {
    pub fn new(metadata: Arc<YoutubeStreamMetadata>) -> Self {
        Self { metadata }
    }
}

#[async_trait]
impl SubActionRunner for UpdateDescriptionRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::YouTube
    }

    fn label(&self) -> &str {
        "Update Stream Description"
    }

    fn summary(&self) -> &str {
        "Sets the description of the active YouTube broadcast."
    }

    fn search_text(&self) -> &str {
        "youtube stream broadcast description metadata"
    }

    fn icon_name(&self) -> &str {
        "edit"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([("description".to_owned(), Variant::String(String::new()))])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::TextArea {
            key: "description",
            label: "Description",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("description") {
            Some(Variant::String(s)) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::UnknownKindId(format!(
                "{KIND_ID}: 'description' must be a non-empty string"
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
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let description = ctx.arg_stack.interpolate(template);

        let outcome = if description.is_empty() {
            SubActionOutcome::Failed("description is empty after interpolation".to_owned())
        } else {
            match self.metadata.set_description(&description).await {
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

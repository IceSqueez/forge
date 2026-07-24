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

use crate::thumbnail::YoutubeThumbnail;

const KIND_ID: &str = "youtube.stream.set_thumbnail";

pub struct SetThumbnailRunner {
    thumbnail: Arc<YoutubeThumbnail>,
}

impl SetThumbnailRunner {
    pub fn new(thumbnail: Arc<YoutubeThumbnail>) -> Self {
        Self { thumbnail }
    }
}

#[async_trait]
impl SubActionRunner for SetThumbnailRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::YouTube
    }

    fn label(&self) -> &str {
        "Set Thumbnail"
    }

    fn summary(&self) -> &str {
        "Uploads a custom thumbnail image for the active YouTube broadcast."
    }

    fn search_text(&self) -> &str {
        "youtube thumbnail image upload photo broadcast video"
    }

    fn icon_name(&self) -> &str {
        "photo"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([("image_path".to_owned(), Variant::String(String::new()))])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Text {
            key: "image_path",
            label: "Image Path",
            placeholder: "~/thumb.png",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("image_path") {
            Some(Variant::String(s)) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::InvalidConfig(format!(
                "{KIND_ID}: 'image_path' must be a non-empty string"
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

        let template = config.str("image_path").unwrap_or_default();
        let image_path = ctx.arg_stack.interpolate(template);

        let outcome = if image_path.is_empty() {
            SubActionOutcome::Failed("image_path is empty after interpolation".to_owned())
        } else {
            SubActionOutcome::from_result(&self.thumbnail.set(&image_path).await)
        };

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

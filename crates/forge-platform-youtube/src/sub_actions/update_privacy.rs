use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::stream_metadata::YoutubeStreamMetadata;

const KIND_ID: &str = "youtube.stream.update_privacy";

const PRIVACY_OPTIONS: &[&str] = &["public", "unlisted", "private"];
const DEFAULT_PRIVACY: &str = "public";

pub struct UpdatePrivacyRunner {
    metadata: Arc<YoutubeStreamMetadata>,
}

impl UpdatePrivacyRunner {
    pub fn new(metadata: Arc<YoutubeStreamMetadata>) -> Self {
        Self { metadata }
    }
}

#[async_trait]
impl SubActionRunner for UpdatePrivacyRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::YouTube
    }

    fn label(&self) -> &str {
        "Update Stream Privacy"
    }

    fn summary(&self) -> &str {
        "Sets the privacy status of the active YouTube broadcast."
    }

    fn search_text(&self) -> &str {
        "youtube stream broadcast privacy public unlisted private visibility metadata"
    }

    fn icon_name(&self) -> &str {
        "eye"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([(
            "privacy_status".to_owned(),
            Variant::String(DEFAULT_PRIVACY.to_owned()),
        )])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Select {
            key: "privacy_status",
            label: "Privacy",
            options: PRIVACY_OPTIONS,
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("privacy_status") {
            Some(Variant::String(s)) if PRIVACY_OPTIONS.contains(&s.as_str()) => Ok(()),
            _ => Err(RegistryError::UnknownKindId(format!(
                "{KIND_ID}: 'privacy_status' must be one of public, unlisted, private"
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
            .get("privacy_status")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let privacy_status = ctx.arg_stack.interpolate(template);

        let outcome = if privacy_status.is_empty() {
            SubActionOutcome::Failed("privacy_status is empty after interpolation".to_owned())
        } else {
            match self.metadata.set_privacy(&privacy_status).await {
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

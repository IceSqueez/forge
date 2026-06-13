use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use super::identity::SelfIdentity;
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};

const KIND_ID: &str = "twitch.channel.update_title";
const MAX_TITLE_CHARS: usize = 140;

pub struct UpdateTitleRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl UpdateTitleRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn apply(&self, title: &str) -> SubActionOutcome {
        if title.is_empty() {
            return SubActionOutcome::Failed("title is empty after interpolation".to_owned());
        }
        if title.chars().count() > MAX_TITLE_CHARS {
            return SubActionOutcome::Failed("title exceeds 140-character limit".to_owned());
        }
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(e.to_string()),
        };
        // PATCH /helix/channels returns 204 No Content on success; Value::Null from transport.
        let request = HelixRequest::new(HelixMethod::Patch, "/helix/channels")
            .query("broadcaster_id", user_id)
            .body(serde_json::json!({ "title": title }));
        match self.transport.execute(request).await {
            Ok(_) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        }
    }
}

#[async_trait]
impl SubActionRunner for UpdateTitleRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Twitch
    }

    fn label(&self) -> &str {
        "Update Stream Title"
    }

    fn summary(&self) -> &str {
        "Updates the broadcaster's stream title."
    }

    fn search_text(&self) -> &str {
        "twitch channel title stream update broadcast"
    }

    fn icon_name(&self) -> &str {
        "pencil"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([("title".to_owned(), Variant::String(String::new()))])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::TextArea {
            key: "title",
            label: "Title",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("title") {
            Some(Variant::String(s)) if !s.is_empty() && s.chars().count() <= MAX_TITLE_CHARS => {}
            Some(Variant::String(s)) if s.is_empty() => {
                return Err(RegistryError::UnknownKindId(format!(
                    "{KIND_ID}: 'title' must not be empty"
                )));
            }
            Some(Variant::String(_)) => {
                return Err(RegistryError::UnknownKindId(format!(
                    "{KIND_ID}: 'title' must be ≤{MAX_TITLE_CHARS} characters"
                )));
            }
            _ => {
                return Err(RegistryError::UnknownKindId(format!(
                    "{KIND_ID}: 'title' must be a non-empty string"
                )));
            }
        }
        Ok(())
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

        let outcome = self.apply(&title).await;

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

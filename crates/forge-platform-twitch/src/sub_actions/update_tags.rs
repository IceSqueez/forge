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

const KIND_ID: &str = "twitch.channel.update_tags";
const MAX_TAGS: usize = 10;
const MAX_TAG_CHARS: usize = 25;

pub struct UpdateTagsRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl UpdateTagsRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn apply(&self, tags: Vec<String>) -> SubActionOutcome {
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(e.to_string()),
        };
        // PATCH /helix/channels returns 204 No Content on success; Value::Null from transport.
        // An empty tags array clears all custom tags.
        let request = HelixRequest::new(HelixMethod::Patch, "/helix/channels")
            .query("broadcaster_id", user_id)
            .body(serde_json::json!({ "tags": tags }));
        match self.transport.execute(request).await {
            Ok(_) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        }
    }
}

/// Splits the textarea value on newlines and commas, trimming each entry and
/// dropping blanks. Returns `Err` if a tag exceeds 25 chars or there are over 10.
fn parse_tags(raw: &str) -> Result<Vec<String>, String> {
    let tags: Vec<String> = raw
        .split(['\n', ','])
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect();

    for tag in &tags {
        if tag.chars().count() > MAX_TAG_CHARS {
            return Err(format!(
                "tag '{tag}' exceeds {MAX_TAG_CHARS}-character limit"
            ));
        }
    }
    if tags.len() > MAX_TAGS {
        return Err(format!("too many tags: max {MAX_TAGS}, got {}", tags.len()));
    }
    Ok(tags)
}

#[async_trait]
impl SubActionRunner for UpdateTagsRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Twitch
    }

    fn label(&self) -> &str {
        "Update Stream Tags"
    }

    fn summary(&self) -> &str {
        "Replaces the broadcaster's stream tags (max 10, each ≤25 chars). Empty clears all tags."
    }

    fn search_text(&self) -> &str {
        "twitch channel tags update broadcast stream"
    }

    fn icon_name(&self) -> &str {
        "tag"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([("tags".to_owned(), Variant::String(String::new()))])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::TextArea {
            key: "tags",
            label: "Tags (one per line or comma-separated)",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        let raw = match config.get("tags") {
            Some(Variant::String(s)) => s.as_str(),
            None => "",
            _ => {
                return Err(RegistryError::UnknownKindId(format!(
                    "{KIND_ID}: 'tags' must be a string"
                )));
            }
        };
        parse_tags(raw).map_err(|msg| RegistryError::UnknownKindId(format!("{KIND_ID}: {msg}")))?;
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
            .get("tags")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let raw = ctx.arg_stack.interpolate(template);

        let outcome = match parse_tags(&raw) {
            Ok(tags) => self.apply(tags).await,
            Err(msg) => SubActionOutcome::Failed(msg),
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

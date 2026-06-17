use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_platform_core::PlatformError;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use futures::future::BoxFuture;
use time::OffsetDateTime;

use crate::channel::KickChannel;

const KIND_ID: &str = "kick.channel.update_info";
const MAX_TAGS: usize = 10;

pub struct UpdateInfoRunner {
    client: Arc<KickChannel>,
    token_source: Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>,
}

impl UpdateInfoRunner {
    pub fn new(
        client: Arc<KickChannel>,
        token_source: Arc<
            dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync,
        >,
    ) -> Self {
        Self {
            client,
            token_source,
        }
    }
}

fn parse_tags(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect()
}

#[async_trait]
impl SubActionRunner for UpdateInfoRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Kick
    }

    fn label(&self) -> &str {
        "Update Channel Info"
    }

    fn summary(&self) -> &str {
        "Updates stream title, category, or tags on the Kick channel. Requires channel:write scope."
    }

    fn search_text(&self) -> &str {
        "kick channel update title category tags stream info"
    }

    fn icon_name(&self) -> &str {
        "edit"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            ("title".to_owned(), Variant::String(String::new())),
            ("category_id".to_owned(), Variant::String(String::new())),
            ("tags".to_owned(), Variant::String(String::new())),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "title",
                label: "Stream Title",
                placeholder: "Leave empty to keep current",
            },
            FormField::Text {
                key: "category_id",
                label: "Category ID",
                placeholder: "Leave empty to keep current",
            },
            FormField::Text {
                key: "tags",
                label: "Tags (comma-separated, max 10)",
                placeholder: "Leave empty to keep current",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        let title = config
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let category_id = config
            .get("category_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let tags_raw = config
            .get("tags")
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        let has_title = !title.is_empty();
        let has_category = !category_id.is_empty();
        let has_tags = !tags_raw.is_empty();

        if !has_title && !has_category && !has_tags {
            return Err(RegistryError::UnknownKindId(format!(
                "{KIND_ID}: at least one of 'title', 'category_id', or 'tags' must be provided"
            )));
        }

        if has_tags {
            let count = parse_tags(tags_raw).len();
            if count > MAX_TAGS {
                return Err(RegistryError::UnknownKindId(format!(
                    "{KIND_ID}: tags count {count} exceeds the maximum of {MAX_TAGS}"
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

        let raw_title = config
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let raw_category = config
            .get("category_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let raw_tags = config
            .get("tags")
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        let title_resolved = ctx.arg_stack.interpolate(raw_title);
        let category_resolved = ctx.arg_stack.interpolate(raw_category);
        let tags_resolved = ctx.arg_stack.interpolate(raw_tags);

        let title = if title_resolved.is_empty() {
            None
        } else {
            Some(title_resolved)
        };

        let category_id = if category_resolved.is_empty() {
            None
        } else {
            match category_resolved.parse::<u64>() {
                Ok(id) => Some(id),
                Err(_) => {
                    let outcome = SubActionOutcome::Failed(format!(
                        "category_id '{category_resolved}' is not a valid numeric id"
                    ));
                    return (
                        SubActionTelemetry {
                            kind: KIND_ID.to_owned(),
                            started_at,
                            duration_ms: start.elapsed().as_millis() as u64,
                            outcome,
                            index: ctx.index,
                        },
                        None,
                    );
                }
            }
        };

        let tags = if tags_resolved.is_empty() {
            None
        } else {
            let parsed = parse_tags(&tags_resolved);
            if parsed.len() > MAX_TAGS {
                let outcome = SubActionOutcome::Failed(format!(
                    "tags count {} exceeds the maximum of {MAX_TAGS}",
                    parsed.len()
                ));
                return (
                    SubActionTelemetry {
                        kind: KIND_ID.to_owned(),
                        started_at,
                        duration_ms: start.elapsed().as_millis() as u64,
                        outcome,
                        index: ctx.index,
                    },
                    None,
                );
            }
            Some(parsed)
        };

        if title.is_none() && category_id.is_none() && tags.is_none() {
            let outcome = SubActionOutcome::Failed(
                "all fields are empty after interpolation; nothing to update".to_owned(),
            );
            return (
                SubActionTelemetry {
                    kind: KIND_ID.to_owned(),
                    started_at,
                    duration_ms: start.elapsed().as_millis() as u64,
                    outcome,
                    index: ctx.index,
                },
                None,
            );
        }

        let outcome = match (self.token_source)().await {
            Err(e) => SubActionOutcome::Failed(format!("token error: {e}")),
            Ok(token) => match self
                .client
                .update_info(&token, title, category_id, tags)
                .await
            {
                Ok(()) => SubActionOutcome::Success,
                Err(e) => SubActionOutcome::Failed(e.to_string()),
            },
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

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

const KIND_ID: &str = "twitch.channel.update_category";

pub struct UpdateCategoryRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl UpdateCategoryRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn apply(&self, category_id: &str) -> SubActionOutcome {
        if category_id.is_empty() {
            return SubActionOutcome::Failed("category_id is empty after interpolation".to_owned());
        }
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(e.to_string()),
        };
        // PATCH /helix/channels returns 204 No Content on success; Value::Null from transport.
        let request = HelixRequest::new(HelixMethod::Patch, "/helix/channels")
            .query("broadcaster_id", user_id)
            .body(serde_json::json!({ "game_id": category_id }));
        match self.transport.execute(request).await {
            Ok(_) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        }
    }
}

#[async_trait]
impl SubActionRunner for UpdateCategoryRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Twitch
    }

    fn label(&self) -> &str {
        "Update Stream Category"
    }

    fn summary(&self) -> &str {
        "Changes the broadcaster's game/category by its Helix game_id."
    }

    fn search_text(&self) -> &str {
        "twitch channel category game update broadcast"
    }

    fn icon_name(&self) -> &str {
        "game-controller"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            ("category_id".to_owned(), Variant::String(String::new())),
            // Display-only label; runtime sends category_id, not this string.
            ("category_name".to_owned(), Variant::String(String::new())),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "category_id",
                label: "Category ID",
                placeholder: "e.g. 509658",
            },
            FormField::Text {
                key: "category_name",
                label: "Category Name (display only)",
                placeholder: "e.g. Just Chatting",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("category_id") {
            Some(Variant::String(s)) if !s.is_empty() => {}
            _ => {
                return Err(RegistryError::UnknownKindId(format!(
                    "{KIND_ID}: 'category_id' must be a non-empty string"
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
            .get("category_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let category_id = ctx.arg_stack.interpolate(template);

        let outcome = self.apply(&category_id).await;

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

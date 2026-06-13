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

const KIND_ID: &str = "twitch.channel.create_marker";
const MAX_DESCRIPTION_CHARS: usize = 140;

pub struct CreateMarkerRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl CreateMarkerRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn create(&self, description: &str) -> Result<(String, i64, String), SubActionOutcome> {
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return Err(SubActionOutcome::Failed(e.to_string())),
        };

        let mut body = serde_json::Map::new();
        if !description.is_empty() {
            body.insert("description".to_owned(), description.into());
        }

        // POST /helix/streams/markers returns 200 with { "data": [{ "id", "position_seconds",
        // "created_at", "description" }] }. Requires user:manage:broadcast scope.
        let request = HelixRequest::new(HelixMethod::Post, "/helix/streams/markers")
            .query("broadcaster_id", user_id)
            .body(serde_json::Value::Object(body));

        let resp = self
            .transport
            .execute(request)
            .await
            .map_err(|e| SubActionOutcome::Failed(e.to_string()))?;

        let marker = resp["data"]
            .as_array()
            .and_then(|arr| arr.first())
            .ok_or_else(|| {
                SubActionOutcome::Failed("empty response from stream markers".to_owned())
            })?;

        let id = marker["id"]
            .as_str()
            .ok_or_else(|| SubActionOutcome::Failed("marker id missing in response".to_owned()))?
            .to_owned();
        let position_seconds = marker["position_seconds"].as_i64().ok_or_else(|| {
            SubActionOutcome::Failed("marker position_seconds missing in response".to_owned())
        })?;
        let created_at = marker["created_at"]
            .as_str()
            .ok_or_else(|| {
                SubActionOutcome::Failed("marker created_at missing in response".to_owned())
            })?
            .to_owned();

        Ok((id, position_seconds, created_at))
    }
}

#[async_trait]
impl SubActionRunner for CreateMarkerRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Twitch
    }

    fn label(&self) -> &str {
        "Create Stream Marker"
    }

    fn summary(&self) -> &str {
        "Places a bookmark at the current live stream position."
    }

    fn search_text(&self) -> &str {
        "twitch stream marker bookmark position highlight timestamp"
    }

    fn icon_name(&self) -> &str {
        "flag"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([("description".to_owned(), Variant::String(String::new()))])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::TextArea {
            key: "description",
            label: "Description (optional)",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        if let Some(Variant::String(s)) = config.get("description")
            && s.chars().count() > MAX_DESCRIPTION_CHARS
        {
            return Err(RegistryError::UnknownKindId(format!(
                "{KIND_ID}: 'description' must be ≤{MAX_DESCRIPTION_CHARS} characters"
            )));
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
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let description = ctx.arg_stack.interpolate(template);

        if description.chars().count() > MAX_DESCRIPTION_CHARS {
            return (
                SubActionTelemetry {
                    kind: KIND_ID.to_owned(),
                    started_at,
                    duration_ms: start.elapsed().as_millis() as u64,
                    outcome: SubActionOutcome::Failed(
                        "description exceeds 140-character limit".to_owned(),
                    ),
                    index: ctx.index,
                },
                None,
            );
        }

        match self.create(&description).await {
            Ok((id, position_seconds, created_at)) => {
                let output_stack = ctx
                    .arg_stack
                    .clone()
                    .set("marker.id".to_owned(), Variant::String(id))
                    .set(
                        "marker.position_seconds".to_owned(),
                        Variant::Int(position_seconds),
                    )
                    .set("marker.created_at".to_owned(), Variant::String(created_at));
                (
                    SubActionTelemetry {
                        kind: KIND_ID.to_owned(),
                        started_at,
                        duration_ms: start.elapsed().as_millis() as u64,
                        outcome: SubActionOutcome::Success,
                        index: ctx.index,
                    },
                    Some(output_stack),
                )
            }
            Err(outcome) => (
                SubActionTelemetry {
                    kind: KIND_ID.to_owned(),
                    started_at,
                    duration_ms: start.elapsed().as_millis() as u64,
                    outcome,
                    index: ctx.index,
                },
                None,
            ),
        }
    }
}

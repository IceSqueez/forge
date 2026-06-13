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

const KIND_ID: &str = "twitch.poll.end";

// Allowed status strings from the config Select; stored as Variant::String.
// Mapped to uppercase before sending to Twitch (Twitch requires "TERMINATED" or "ARCHIVED").
const STATUS_TERMINATED: &str = "terminated";
const STATUS_ARCHIVED: &str = "archived";

pub struct EndPollRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl EndPollRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn end(&self, poll_id: &str, status_uppercase: &str) -> SubActionOutcome {
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(e.to_string()),
        };

        // PATCH /helix/polls — broadcaster_id is a query param; id and status go in the body.
        // status must be uppercase: "TERMINATED" stops immediately, "ARCHIVED" ends and hides.
        // Requires channel:manage:polls scope.
        let request = HelixRequest::new(HelixMethod::Patch, "/helix/polls")
            .query("broadcaster_id", user_id)
            .body(serde_json::json!({
                "id": poll_id,
                "status": status_uppercase,
            }));

        match self.transport.execute(request).await {
            Ok(_) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        }
    }
}

#[async_trait]
impl SubActionRunner for EndPollRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Twitch
    }

    fn label(&self) -> &str {
        "End Poll"
    }

    fn summary(&self) -> &str {
        "Ends an active poll immediately. Use 'terminated' to stop with results visible or 'archived' to hide it."
    }

    fn search_text(&self) -> &str {
        "twitch poll end stop terminate archive"
    }

    fn icon_name(&self) -> &str {
        "chart-bar"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            (
                "poll_id".to_owned(),
                Variant::String("%poll.id%".to_owned()),
            ),
            (
                "status".to_owned(),
                Variant::String(STATUS_TERMINATED.to_owned()),
            ),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "poll_id",
                label: "Poll ID",
                placeholder: "%poll.id%",
            },
            FormField::Select {
                key: "status",
                label: "End Status",
                options: &[STATUS_TERMINATED, STATUS_ARCHIVED],
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        let poll_id = match config.get("poll_id") {
            Some(Variant::String(s)) => s.as_str(),
            _ => "",
        };
        if poll_id.is_empty() {
            return Err(RegistryError::UnknownKindId(format!(
                "{KIND_ID}: 'poll_id' is required"
            )));
        }

        match config.get("status") {
            Some(Variant::String(s)) if s == STATUS_TERMINATED || s == STATUS_ARCHIVED => {}
            _ => {
                return Err(RegistryError::UnknownKindId(format!(
                    "{KIND_ID}: 'status' must be '{STATUS_TERMINATED}' or '{STATUS_ARCHIVED}'"
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

        let poll_id_template = config
            .get("poll_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let poll_id = ctx.arg_stack.interpolate(poll_id_template);

        if poll_id.is_empty() {
            return (
                SubActionTelemetry {
                    kind: KIND_ID.to_owned(),
                    started_at,
                    duration_ms: start.elapsed().as_millis() as u64,
                    outcome: SubActionOutcome::Failed("poll_id is required".to_owned()),
                    index: ctx.index,
                },
                None,
            );
        }

        // Config stores lowercase ("terminated"/"archived"); Twitch requires uppercase.
        let status_lower = config
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or(STATUS_TERMINATED);
        let status_uppercase = status_lower.to_uppercase();

        let outcome = self.end(&poll_id, &status_uppercase).await;

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

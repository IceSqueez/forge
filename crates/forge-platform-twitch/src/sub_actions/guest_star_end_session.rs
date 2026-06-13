use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};
use time::OffsetDateTime;

use super::guest_star::{interpolate, session_id_field, validate_session_id, with_session_id};
use super::identity::SelfIdentity;
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};

const KIND_ID: &str = "twitch.guest_star.end_session";

pub struct GuestStarEndSessionRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl GuestStarEndSessionRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn end(&self, session_id: &str) -> SubActionOutcome {
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(format!("{KIND_ID}: {e}")),
        };

        // Verified against dev.twitch.tv (2026-06-13, BETA): DELETE
        // /helix/guest_star/session. Only broadcaster_id and session_id are required
        // in the query; moderator_id is NOT sent — only the broadcaster can end their
        // own session. Scope: channel:manage:guest_star.
        let request = HelixRequest::new(HelixMethod::Delete, "/helix/guest_star/session")
            .query("broadcaster_id", user_id)
            .query("session_id", session_id.to_owned());

        match self.transport.execute(request).await {
            Ok(_) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(format!("{KIND_ID}: {e}")),
        }
    }
}

#[async_trait]
impl SubActionRunner for GuestStarEndSessionRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Twitch
    }

    fn label(&self) -> &str {
        "End Guest Star Session"
    }

    fn summary(&self) -> &str {
        "Ends the active Guest Star session for the broadcaster's channel."
    }

    fn search_text(&self) -> &str {
        "twitch guest star session end close stop collab"
    }

    fn icon_name(&self) -> &str {
        "x-circle"
    }

    fn default_config(&self) -> SubActionConfig {
        with_session_id(BTreeMap::new())
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![session_id_field()]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        validate_session_id(KIND_ID, config)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let session_id = interpolate(config, ctx.arg_stack, "session_id");

        let outcome = if session_id.is_empty() {
            SubActionOutcome::Failed("session_id is required".to_owned())
        } else {
            self.end(&session_id).await
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

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};
use time::OffsetDateTime;

use super::guest_star::{
    GuestStarContext, interpolate, session_id_field, target_login_field, validate_session_id,
    validate_target_login, with_session_id, with_target_login,
};
use super::identity::SelfIdentity;
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};

const KIND_ID: &str = "twitch.guest_star.invite";

pub struct GuestStarInviteRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl GuestStarInviteRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn invite(&self, session_id: &str, target_login: &str) -> SubActionOutcome {
        let ctx =
            match GuestStarContext::resolve(self.transport.as_ref(), &self.identity, target_login)
                .await
            {
                Ok(c) => c,
                Err(e) => return SubActionOutcome::Failed(format!("{KIND_ID}: {e}")),
            };

        // Send Guest Star Invite returns 204 No Content, so there is no invite
        // id to surface; the runner pushes no output stack.
        let request = HelixRequest::new(HelixMethod::Post, "/helix/guest_star/invites")
            .query("broadcaster_id", ctx.self_id.clone())
            .query("moderator_id", ctx.self_id)
            .query("session_id", session_id.to_owned())
            .query("guest_id", ctx.guest_id);

        match self.transport.execute(request).await {
            Ok(_) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(format!("{KIND_ID}: {e}")),
        }
    }
}

#[async_trait]
impl SubActionRunner for GuestStarInviteRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Twitch
    }

    fn label(&self) -> &str {
        "Send Guest Star Invite"
    }

    fn summary(&self) -> &str {
        "Invites a viewer to join the active Guest Star session."
    }

    fn search_text(&self) -> &str {
        "twitch guest star invite collab session viewer join"
    }

    fn icon_name(&self) -> &str {
        "user-plus"
    }

    fn default_config(&self) -> SubActionConfig {
        with_target_login(with_session_id(BTreeMap::new()))
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![session_id_field(), target_login_field()]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        validate_session_id(KIND_ID, config)?;
        validate_target_login(KIND_ID, config)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let session_id = interpolate(config, ctx.arg_stack, "session_id");
        let target_login = interpolate(config, ctx.arg_stack, "target_user_login");

        let outcome = if session_id.is_empty() {
            SubActionOutcome::Failed("session_id is required".to_owned())
        } else if target_login.is_empty() {
            SubActionOutcome::Failed("target_user_login is required".to_owned())
        } else {
            self.invite(&session_id, &target_login).await
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

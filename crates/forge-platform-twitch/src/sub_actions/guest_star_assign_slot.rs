use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use super::guest_star::{
    GuestStarContext, interpolate, session_id_field, target_login_field, validate_session_id,
    validate_target_login, with_session_id, with_target_login,
};
use super::identity::SelfIdentity;
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};

const KIND_ID: &str = "twitch.guest_star.assign_slot";

pub struct GuestStarAssignSlotRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl GuestStarAssignSlotRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn assign(
        &self,
        session_id: &str,
        target_login: &str,
        slot_id: &str,
    ) -> SubActionOutcome {
        let ctx =
            match GuestStarContext::resolve(self.transport.as_ref(), &self.identity, target_login)
                .await
            {
                Ok(c) => c,
                Err(e) => return SubActionOutcome::Failed(format!("{KIND_ID}: {e}")),
            };

        // Assign Guest Star Slot pins an invited guest to a numbered slot in the
        // session layout. broadcaster_id == moderator_id == self; guest_id is the
        // resolved target; slot_id targets the layout position.
        let request = HelixRequest::new(HelixMethod::Post, "/helix/guest_star/slot")
            .query("broadcaster_id", ctx.self_id.clone())
            .query("moderator_id", ctx.self_id)
            .query("session_id", session_id.to_owned())
            .query("guest_id", ctx.guest_id)
            .query("slot_id", slot_id.to_owned());

        match self.transport.execute(request).await {
            Ok(_) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(format!("{KIND_ID}: {e}")),
        }
    }
}

#[async_trait]
impl SubActionRunner for GuestStarAssignSlotRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Twitch
    }

    fn label(&self) -> &str {
        "Assign Guest Star Slot"
    }

    fn summary(&self) -> &str {
        "Assigns an invited guest to a numbered slot in the Guest Star session."
    }

    fn search_text(&self) -> &str {
        "twitch guest star slot assign seat layout collab session"
    }

    fn icon_name(&self) -> &str {
        "layout"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut config = with_target_login(with_session_id(BTreeMap::new()));
        config.insert("slot_id".to_owned(), Variant::String(String::new()));
        config
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            session_id_field(),
            target_login_field(),
            FormField::Text {
                key: "slot_id",
                label: "Slot ID",
                placeholder: "1",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        validate_session_id(KIND_ID, config)?;
        validate_target_login(KIND_ID, config)?;
        match config.get("slot_id") {
            Some(Variant::String(s)) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::UnknownKindId(format!(
                "{KIND_ID}: 'slot_id' is required"
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

        let session_id = interpolate(config, ctx.arg_stack, "session_id");
        let target_login = interpolate(config, ctx.arg_stack, "target_user_login");
        let slot_id = interpolate(config, ctx.arg_stack, "slot_id");

        let outcome = if session_id.is_empty() {
            SubActionOutcome::Failed("session_id is required".to_owned())
        } else if target_login.is_empty() {
            SubActionOutcome::Failed("target_user_login is required".to_owned())
        } else if slot_id.is_empty() {
            SubActionOutcome::Failed("slot_id is required".to_owned())
        } else {
            self.assign(&session_id, &target_login, &slot_id).await
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

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
            _ => Err(RegistryError::InvalidConfig(format!(
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
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::helix::HelixError;
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, SELF_USER_ID, make_ctx, users_fixture,
    };

    fn runner_with(
        responses: Vec<Result<serde_json::Value, HelixError>>,
    ) -> (Arc<MockTransport>, GuestStarAssignSlotRunner) {
        let transport = Arc::new(MockTransport::returning_sequence(responses));
        let runner = GuestStarAssignSlotRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        );
        (transport, runner)
    }

    fn config(session_template: &str, target: &str, slot: &str) -> SubActionConfig {
        BTreeMap::from([
            (
                "session_id".to_owned(),
                Variant::String(session_template.to_owned()),
            ),
            (
                "target_user_login".to_owned(),
                Variant::String(target.to_owned()),
            ),
            ("slot_id".to_owned(), Variant::String(slot.to_owned())),
        ])
    }

    fn stack_with_session(session_id: &str) -> ArgStack {
        ArgStack::new().set(
            "guest_star.session_id".to_owned(),
            Variant::String(session_id.to_owned()),
        )
    }

    #[tokio::test]
    async fn execute_resolves_login_then_posts_slot_with_slot_id_in_query() {
        let (transport, runner) =
            runner_with(vec![users_fixture("55"), Ok(serde_json::Value::Null)]);
        let stack = stack_with_session("SESSION-XYZ");

        let (telemetry, _) = runner
            .execute(
                &config("%guest_star.session_id%", "ghost", "3"),
                &make_ctx(&stack),
            )
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert_eq!(transport.call_count(), 2, "resolve then assign");

        let resolve = transport.request(0);
        assert_eq!(resolve.method, HelixMethod::Get);
        assert_eq!(resolve.path, "/helix/users");
        assert!(
            resolve
                .query
                .contains(&("login".to_owned(), "ghost".to_owned())),
            "resolve must look up the target login: {:?}",
            resolve.query
        );

        let act = transport.request(1);
        assert_eq!(act.method, HelixMethod::Post);
        assert_eq!(act.path, "/helix/guest_star/slot");
        assert!(
            act.query
                .contains(&("broadcaster_id".to_owned(), SELF_USER_ID.to_owned())),
            "broadcaster must be self: {:?}",
            act.query
        );
        assert!(
            act.query
                .contains(&("moderator_id".to_owned(), SELF_USER_ID.to_owned())),
            "moderator must be self: {:?}",
            act.query
        );
        assert!(
            act.query
                .contains(&("session_id".to_owned(), "SESSION-XYZ".to_owned())),
            "session_id must come off the arg stack: {:?}",
            act.query
        );
        assert!(
            act.query
                .contains(&("guest_id".to_owned(), "55".to_owned())),
            "guest_id must be the RESOLVED id: {:?}",
            act.query
        );
        assert!(
            act.query.contains(&("slot_id".to_owned(), "3".to_owned())),
            "assign_slot MUST carry slot_id: {:?}",
            act.query
        );
    }

    #[tokio::test]
    async fn empty_slot_id_after_interpolation_fails_before_any_helix_call() {
        let (transport, runner) =
            runner_with(vec![users_fixture("55"), Ok(serde_json::Value::Null)]);
        let stack = stack_with_session("SESSION-XYZ");

        let (telemetry, _) = runner
            .execute(
                &config("%guest_star.session_id%", "ghost", ""),
                &make_ctx(&stack),
            )
            .await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert_eq!(
            transport.call_count(),
            0,
            "empty slot_id must fail before resolve and act"
        );
    }

    #[test]
    fn validate_config_requires_slot_id_in_addition_to_session_and_login() {
        let runner = GuestStarAssignSlotRunner::new(
            Arc::new(MockTransport::returning(Ok(serde_json::Value::Null))),
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        );

        assert!(
            runner
                .validate_config(&config("%guest_star.session_id%", "ghost", ""))
                .is_err(),
            "missing slot_id must be rejected even when session+login are present"
        );
        assert!(
            runner
                .validate_config(&config("%guest_star.session_id%", "ghost", "1"))
                .is_ok(),
            "all three present must validate"
        );
    }
}

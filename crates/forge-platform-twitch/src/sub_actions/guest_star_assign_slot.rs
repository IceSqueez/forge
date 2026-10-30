use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigProblem, ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner,
    TypedField,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};

use super::guest_star::{GUEST_LOGIN, GuestStarContext, SESSION_ID, SLOT_ID};
use crate::account::identity::SelfIdentity;
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};
use crate::step_failure;
use crate::vocabulary::sub_action_ids;

const KIND_ID: &str = sub_action_ids::GUEST_STAR_ASSIGN_SLOT;

pub(crate) const GUEST_SLOT_CONFIG: ConfigSpec =
    ConfigSpec::new(&[&SESSION_ID, &GUEST_LOGIN, &SLOT_ID]);

pub(crate) struct GuestSlot {
    pub(crate) session_id: String,
    pub(crate) target_login: String,
    pub(crate) slot_id: String,
}

impl GuestSlot {
    pub(crate) fn read(config: &SubActionConfig, args: &ArgStack) -> Result<Self, ConfigProblem> {
        Ok(Self {
            session_id: SESSION_ID.read(config, args)?,
            target_login: GUEST_LOGIN.read(config, args)?,
            slot_id: SLOT_ID.read(config, args)?,
        })
    }
}

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

    async fn assign(&self, slot: &GuestSlot) -> SubActionOutcome {
        let ctx = match GuestStarContext::resolve(
            self.transport.as_ref(),
            &self.identity,
            &slot.target_login,
        )
        .await
        {
            Ok(c) => c,
            Err(e) => {
                return step_failure::failed(&e);
            }
        };
        let request = HelixRequest::new(HelixMethod::Post, "/helix/guest_star/slot")
            .query("broadcaster_id", ctx.self_id.clone())
            .query("moderator_id", ctx.self_id)
            .query("session_id", slot.session_id.clone())
            .query("guest_id", ctx.guest_id)
            .query("slot_id", slot.slot_id.clone());
        match self.transport.execute(request).await {
            Ok(_) => SubActionOutcome::Success,
            Err(e) => step_failure::failed(&e),
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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(GUEST_SLOT_CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, KIND_ID);
        let slot = match GuestSlot::read(config, ctx.arg_stack) {
            Ok(slot) => slot,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        (timer.finish(self.assign(&slot).await), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helix::HelixError;
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, SELF_USER_ID, make_ctx, users_fixture,
    };
    use forge_types::Variant;
    use std::collections::BTreeMap;

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

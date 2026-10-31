use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::SubActionConfig;
use forge_registry::{ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};

use super::guest_star::GuestStarContext;
use super::guest_star_assign_slot::{GUEST_SLOT_CONFIG, GuestSlot};
use crate::account::identity::SelfIdentity;
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};
use crate::step_failure;
use crate::vocabulary::sub_action_ids;

const KIND_ID: &str = sub_action_ids::GUEST_STAR_REMOVE_GUEST;

pub(crate) struct GuestStarRemoveGuestRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl GuestStarRemoveGuestRunner {
    pub(crate) fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn remove(&self, slot: &GuestSlot) -> SubActionOutcome {
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
        let request = HelixRequest::new(HelixMethod::Delete, "/helix/guest_star/slot")
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
impl SubActionRunner for GuestStarRemoveGuestRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Twitch
    }

    fn label(&self) -> &str {
        "Remove Guest Star Guest"
    }

    fn summary(&self) -> &str {
        "Removes a guest from their slot in the active Guest Star session."
    }

    fn search_text(&self) -> &str {
        "twitch guest star remove kick guest slot session collab"
    }

    fn icon_name(&self) -> &str {
        "user-minus"
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
        (timer.finish(self.remove(&slot).await), None)
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
    ) -> (Arc<MockTransport>, GuestStarRemoveGuestRunner) {
        let transport = Arc::new(MockTransport::returning_sequence(responses));
        let runner = GuestStarRemoveGuestRunner::new(
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
    async fn execute_resolves_login_then_deletes_slot_with_all_five_query_params() {
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
        assert_eq!(transport.call_count(), 2, "resolve then remove");

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
        assert_eq!(act.method, HelixMethod::Delete);
        assert_eq!(act.path, "/helix/guest_star/slot");
        for expected in [
            ("broadcaster_id", SELF_USER_ID),
            ("moderator_id", SELF_USER_ID),
            ("session_id", "SESSION-XYZ"),
            ("guest_id", "55"),
            ("slot_id", "3"),
        ] {
            assert!(
                act.query
                    .contains(&(expected.0.to_owned(), expected.1.to_owned())),
                "remove_guest DELETE must carry {}={}: {:?}",
                expected.0,
                expected.1,
                act.query
            );
        }
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
        let runner = GuestStarRemoveGuestRunner::new(
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

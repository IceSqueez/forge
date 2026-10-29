use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner, TypedField,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};

use super::guest_star::{SESSION_ID, first_missing};
use crate::account::identity::SelfIdentity;
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};
use crate::vocabulary::sub_action_ids;

const KIND_ID: &str = sub_action_ids::GUEST_STAR_END_SESSION;

const CONFIG: ConfigSpec = ConfigSpec::new(&[&SESSION_ID]);

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
        if let Some(missing) = first_missing(&[(&SESSION_ID, session_id)]) {
            return SubActionOutcome::Failed(missing);
        }
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(format!("{KIND_ID}: {e}")),
        };
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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, KIND_ID);
        let session_id = match SESSION_ID.read(config, ctx.arg_stack) {
            Ok(session_id) => session_id,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        (timer.finish(self.end(&session_id).await), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helix::HelixError;
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, SELF_USER_ID, TOKEN_SENTINEL, make_ctx,
    };
    use forge_types::Variant;
    use std::collections::BTreeMap;

    fn runner_with(
        response: Result<serde_json::Value, HelixError>,
    ) -> (Arc<MockTransport>, GuestStarEndSessionRunner) {
        let transport = Arc::new(MockTransport::returning(response));
        let runner = GuestStarEndSessionRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        );
        (transport, runner)
    }

    fn cfg(session: &str) -> SubActionConfig {
        BTreeMap::from([("session_id".to_owned(), Variant::String(session.to_owned()))])
    }

    #[tokio::test]
    async fn ends_session_with_self_and_interpolated_session_no_moderator() {
        let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
        let stack = ArgStack::new().set(
            "guest_star.session_id".to_owned(),
            Variant::String("SESSION-END".to_owned()),
        );

        let (telemetry, out) = runner
            .execute(&cfg("%guest_star.session_id%"), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert!(out.is_none());
        assert_eq!(transport.call_count(), 1);

        let request = transport.request(0);
        assert_eq!(request.method, HelixMethod::Delete);
        assert_eq!(request.path, "/helix/guest_star/session");
        assert!(
            request
                .query
                .contains(&("broadcaster_id".to_owned(), SELF_USER_ID.to_owned())),
            "broadcaster must be self: {:?}",
            request.query
        );
        assert!(
            request
                .query
                .contains(&("session_id".to_owned(), "SESSION-END".to_owned())),
            "session_id must come off the arg stack: {:?}",
            request.query
        );
        assert!(
            !request.query.iter().any(|(k, _)| k == "moderator_id"),
            "end_session must NOT send moderator_id: {:?}",
            request.query
        );
        assert!(request.body.is_none(), "DELETE carries no body");
    }

    #[tokio::test]
    async fn empty_session_id_fails_before_helix_call() {
        let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
        let stack = ArgStack::new();

        let (telemetry, _) = runner.execute(&cfg(""), &make_ctx(&stack)).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert_eq!(transport.call_count(), 0);
    }

    #[test]
    fn validate_config_requires_session_id() {
        let (_t, runner) = runner_with(Ok(serde_json::Value::Null));
        assert!(runner.validate_config(&cfg("")).is_err());
        assert!(
            runner
                .validate_config(&cfg("%guest_star.session_id%"))
                .is_ok()
        );
    }

    #[tokio::test]
    async fn helix_failure_maps_to_failed_without_token() {
        let (_transport, runner) = runner_with(Err(HelixError::Http {
            status: 404,
            body: "no active session".to_owned(),
        }));
        let stack = ArgStack::new();

        let (telemetry, _) = runner.execute(&cfg("SESSION-END"), &make_ctx(&stack)).await;

        assert!(matches!(
            telemetry.outcome,
            SubActionOutcome::Failed(msg) if msg.contains("404") && !msg.contains(TOKEN_SENTINEL)
        ));
    }
}

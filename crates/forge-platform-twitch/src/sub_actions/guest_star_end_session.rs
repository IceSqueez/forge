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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::helix::HelixError;
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, SELF_USER_ID, TOKEN_SENTINEL, make_ctx,
    };
    use forge_types::Variant;

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

    // #7 Happy: DELETE /helix/guest_star/session with broadcaster_id=self and the
    // interpolated session_id in the query, NO moderator_id (broadcaster-only),
    // and no body.
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

    // #8 Empty session_id fails before any Helix call.
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

    // #9 (session variant) token-leak guard on the failure path.
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

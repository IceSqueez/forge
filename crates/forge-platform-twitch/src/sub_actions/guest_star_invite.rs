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

        // 204 No Content: no invite id to surface, so this runner pushes no output stack.
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
        MockCreds, MockTransport, SELF_USER_ID, TOKEN_SENTINEL, make_ctx, users_fixture,
    };
    use forge_types::Variant;

    fn runner_with(
        responses: Vec<Result<serde_json::Value, HelixError>>,
    ) -> (Arc<MockTransport>, GuestStarInviteRunner) {
        let transport = Arc::new(MockTransport::returning_sequence(responses));
        let runner = GuestStarInviteRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        );
        (transport, runner)
    }

    fn config(session_template: &str, target: &str) -> SubActionConfig {
        BTreeMap::from([
            (
                "session_id".to_owned(),
                Variant::String(session_template.to_owned()),
            ),
            (
                "target_user_login".to_owned(),
                Variant::String(target.to_owned()),
            ),
        ])
    }

    fn stack_with_session(session_id: &str) -> ArgStack {
        ArgStack::new().set(
            "guest_star.session_id".to_owned(),
            Variant::String(session_id.to_owned()),
        )
    }

    #[tokio::test]
    async fn execute_resolves_login_then_posts_invite_with_exact_query_set() {
        let (transport, runner) =
            runner_with(vec![users_fixture("55"), Ok(serde_json::Value::Null)]);
        let stack = stack_with_session("SESSION-XYZ");

        let (telemetry, output) = runner
            .execute(
                &config("%guest_star.session_id%", "ghost"),
                &make_ctx(&stack),
            )
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert_eq!(transport.call_count(), 2, "resolve then invite");
        assert!(output.is_none(), "invite returns 204, no output stack");

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
        assert_eq!(act.path, "/helix/guest_star/invites");
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
            "guest_id must be the RESOLVED id, not the login: {:?}",
            act.query
        );
        assert!(
            !act.query.iter().any(|(k, _)| k == "slot_id"),
            "invite must NOT carry slot_id: {:?}",
            act.query
        );
    }

    #[tokio::test]
    async fn empty_session_id_after_interpolation_fails_before_any_helix_call() {
        let (transport, runner) =
            runner_with(vec![users_fixture("55"), Ok(serde_json::Value::Null)]);
        let stack = stack_with_session("");

        let (telemetry, _) = runner
            .execute(
                &config("%guest_star.session_id%", "ghost"),
                &make_ctx(&stack),
            )
            .await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert_eq!(
            transport.call_count(),
            0,
            "empty session_id must fail before resolve and act"
        );
    }

    #[tokio::test]
    async fn empty_target_login_after_interpolation_fails_before_any_helix_call() {
        let (transport, runner) =
            runner_with(vec![users_fixture("55"), Ok(serde_json::Value::Null)]);
        let stack = stack_with_session("SESSION-XYZ");

        let (telemetry, _) = runner
            .execute(&config("%guest_star.session_id%", ""), &make_ctx(&stack))
            .await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert_eq!(
            transport.call_count(),
            0,
            "empty target login must fail before resolve and act"
        );
    }

    #[tokio::test]
    async fn resolve_failure_skips_the_invite_call() {
        let (transport, runner) = runner_with(vec![Err(HelixError::Http {
            status: 404,
            body: "user not found".to_owned(),
        })]);
        let stack = stack_with_session("SESSION-XYZ");

        let (telemetry, _) = runner
            .execute(
                &config("%guest_star.session_id%", "ghost"),
                &make_ctx(&stack),
            )
            .await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert_eq!(
            transport.call_count(),
            1,
            "resolve failure must not issue the invite POST"
        );
    }

    #[tokio::test]
    async fn invite_http_failure_maps_to_failed_without_token_or_url() {
        let (transport, runner) = runner_with(vec![
            users_fixture("55"),
            Err(HelixError::Http {
                status: 429,
                body: "invite ratelimited".to_owned(),
            }),
        ]);
        let stack = stack_with_session("SESSION-XYZ");

        let (telemetry, _) = runner
            .execute(
                &config("%guest_star.session_id%", "ghost"),
                &make_ctx(&stack),
            )
            .await;

        assert_eq!(transport.call_count(), 2, "failure from the invite call");
        let SubActionOutcome::Failed(msg) = telemetry.outcome else {
            panic!("expected Failed, got {:?}", telemetry.outcome);
        };
        assert!(msg.contains("429"), "status must surface: {msg}");
        assert!(!msg.contains(TOKEN_SENTINEL), "token leaked: {msg}");
        assert!(!msg.contains("api.twitch.tv"), "URL leaked: {msg}");
    }

    #[test]
    fn validate_config_requires_session_id_and_target_login() {
        let runner = GuestStarInviteRunner::new(
            Arc::new(MockTransport::returning(Ok(serde_json::Value::Null))),
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        );

        assert!(runner.validate_config(&runner.default_config()).is_err());
        assert!(
            runner
                .validate_config(&config("%guest_star.session_id%", ""))
                .is_err(),
            "missing target login must be rejected"
        );
        assert!(
            runner.validate_config(&config("", "ghost")).is_err(),
            "missing session id must be rejected"
        );
        assert!(
            runner
                .validate_config(&config("%guest_star.session_id%", "ghost"))
                .is_ok(),
            "both present must validate"
        );
    }
}

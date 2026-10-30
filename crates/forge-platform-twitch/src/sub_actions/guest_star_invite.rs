use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigProblem, ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner,
    TypedField,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};

use super::guest_star::{GUEST_LOGIN, GuestStarContext, SESSION_ID};
use crate::account::identity::SelfIdentity;
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};
use crate::step_failure;
use crate::vocabulary::sub_action_ids;

const KIND_ID: &str = sub_action_ids::GUEST_STAR_INVITE;

const CONFIG: ConfigSpec = ConfigSpec::new(&[&SESSION_ID, &GUEST_LOGIN]);

fn read_config(
    config: &SubActionConfig,
    args: &ArgStack,
) -> Result<(String, String), ConfigProblem> {
    Ok((
        SESSION_ID.read(config, args)?,
        GUEST_LOGIN.read(config, args)?,
    ))
}

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
                Err(e) => {
                    return step_failure::failed(&e);
                }
            };
        let request = HelixRequest::new(HelixMethod::Post, "/helix/guest_star/invites")
            .query("broadcaster_id", ctx.self_id.clone())
            .query("moderator_id", ctx.self_id)
            .query("session_id", session_id.to_owned())
            .query("guest_id", ctx.guest_id);
        match self.transport.execute(request).await {
            Ok(_) => SubActionOutcome::Success,
            Err(e) => step_failure::failed(&e),
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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, KIND_ID);
        let (session_id, target_login) = match read_config(config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        (
            timer.finish(self.invite(&session_id, &target_login).await),
            None,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helix::HelixError;
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, SELF_USER_ID, assert_request_refused, make_ctx, users_fixture,
    };
    use forge_types::Variant;
    use std::collections::BTreeMap;

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
        assert_request_refused(&telemetry.outcome, 429);
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

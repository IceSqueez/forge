use std::sync::Arc;

use async_trait::async_trait;
use forge_net::status::BAD_REQUEST;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigProblem, ConfigSpec, Interpolation, RunContext, StepTimer, SubActionCategory,
    SubActionRunner,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};

use super::fields::MODERATION_TARGET;
use crate::account::identity::SelfIdentity;
use crate::account::user_target::UserTarget;
use crate::helix::{HelixError, HelixMethod, HelixRequest, HelixTransport};
use crate::step_failure;
use crate::vocabulary::sub_action_ids;

const KIND_ID: &str = sub_action_ids::MODERATION_UNBAN_USER;
const NOT_BANNED_MARKER: &str = "not banned";

const CONFIG: ConfigSpec = ConfigSpec::new(&[&MODERATION_TARGET.login]);

fn already_unbanned(error: &HelixError) -> bool {
    match error {
        HelixError::Http { status, body } => {
            *status == BAD_REQUEST && body.to_lowercase().contains(NOT_BANNED_MARKER)
        }
        _ => false,
    }
}

pub struct UnbanUserRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl UnbanUserRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn unban(&self, target: Option<UserTarget<'_>>) -> SubActionOutcome {
        let Some(target) = target else {
            return SubActionOutcome::Failed(MODERATION_TARGET.empty_after_interpolation().into());
        };
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return step_failure::failed(&e),
        };
        let target_user_id = match target.user_id(self.transport.as_ref()).await {
            Ok(id) => id,
            Err(e) => return step_failure::failed(&e),
        };
        let request = HelixRequest::new(HelixMethod::Delete, "/helix/moderation/bans")
            .query("broadcaster_id", user_id.clone())
            .query("moderator_id", user_id)
            .query("user_id", target_user_id)
            .expect_status_with_excerpt(BAD_REQUEST, NOT_BANNED_MARKER);
        match self.transport.execute(request).await {
            Err(error) if already_unbanned(&error) => SubActionOutcome::Success,
            result => step_failure::outcome(&result),
        }
    }
}

#[async_trait]
impl SubActionRunner for UnbanUserRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Moderation
    }

    fn label(&self) -> &str {
        "Unban User"
    }

    fn summary(&self) -> &str {
        "Removes a ban or active timeout from a user."
    }

    fn search_text(&self) -> &str {
        "twitch moderation unban untimeout lift remove ban user"
    }

    fn icon_name(&self) -> &str {
        "shield-check"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn check_config(
        &self,
        config: &SubActionConfig,
        interpolation: Interpolation<'_>,
    ) -> Result<(), ConfigProblem> {
        MODERATION_TARGET.check(config, interpolation)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, KIND_ID);
        let target = match MODERATION_TARGET.read(config, ctx.arg_stack) {
            Ok(target) => target,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        (timer.finish(self.unban(target.target()).await), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helix::HelixError;
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, REQUEST_FAIL_KIND, SELF_USER_ID, make_ctx, observed_helix_at,
        users_fixture,
    };
    use forge_types::Variant;
    use std::collections::BTreeMap;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn runner_with(
        responses: Vec<Result<serde_json::Value, HelixError>>,
    ) -> (Arc<MockTransport>, UnbanUserRunner) {
        let transport = Arc::new(MockTransport::returning_sequence(responses));
        let runner = UnbanUserRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        );
        (transport, runner)
    }

    fn config(target: &str) -> SubActionConfig {
        BTreeMap::from([(
            "target_user_login".to_owned(),
            Variant::String(target.to_owned()),
        )])
    }

    #[tokio::test]
    async fn execute_deletes_ban_for_resolved_target_as_self_moderator() {
        let (transport, runner) =
            runner_with(vec![users_fixture("555"), Ok(serde_json::Value::Null)]);
        let stack = ArgStack::new();

        let (telemetry, _) = runner.execute(&config("target"), &make_ctx(&stack)).await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let request = transport.last_request();
        assert_eq!(request.method, HelixMethod::Delete);
        assert_eq!(request.path, "/helix/moderation/bans");
        assert!(
            request
                .query
                .contains(&("broadcaster_id".to_owned(), SELF_USER_ID.to_owned()))
        );
        assert!(
            request
                .query
                .contains(&("moderator_id".to_owned(), SELF_USER_ID.to_owned()))
        );
        assert!(
            request
                .query
                .contains(&("user_id".to_owned(), "555".to_owned())),
            "user_id query must carry the RESOLVED target id"
        );
        assert!(request.body.is_none(), "unban sends no body");
    }

    #[tokio::test]
    async fn execute_counts_a_400_saying_the_user_is_not_banned_as_success() {
        let bad_request = |body: &str| HelixError::Http {
            status: reqwest::StatusCode::BAD_REQUEST.as_u16(),
            body: body.to_owned(),
        };
        let cases = [
            "{\"error\":\"Bad Request\",\"status\":400,\"message\":\"The user is not banned or in a timeout.\"}",
            "user is not banned",
            "USER IS NOT BANNED",
            "Пользователь not Banned",
        ];

        for body in cases {
            let (_transport, runner) =
                runner_with(vec![users_fixture("555"), Err(bad_request(body))]);
            let stack = ArgStack::new();

            let (telemetry, _) = runner.execute(&config("target"), &make_ctx(&stack)).await;

            assert_eq!(telemetry.outcome, SubActionOutcome::Success, "{body:?}");
        }
    }

    #[tokio::test]
    async fn execute_fails_on_any_other_unban_error() {
        let http = |status: reqwest::StatusCode, body: &str| HelixError::Http {
            status: status.as_u16(),
            body: body.to_owned(),
        };
        let cases = [
            (
                "400 without the marker",
                http(
                    reqwest::StatusCode::BAD_REQUEST,
                    "Missing required parameter user_id",
                ),
            ),
            (
                "400 with an empty body",
                http(reqwest::StatusCode::BAD_REQUEST, ""),
            ),
            (
                "400 with the words apart",
                http(reqwest::StatusCode::BAD_REQUEST, "not a banned word"),
            ),
            (
                "404 saying not banned",
                http(reqwest::StatusCode::NOT_FOUND, "user is not banned"),
            ),
            (
                "422 saying not banned",
                http(
                    reqwest::StatusCode::UNPROCESSABLE_ENTITY,
                    "user is not banned",
                ),
            ),
            (
                "500 saying not banned",
                http(
                    reqwest::StatusCode::INTERNAL_SERVER_ERROR,
                    "user is not banned",
                ),
            ),
            (
                "transport saying not banned",
                HelixError::Transport("user is not banned".to_owned()),
            ),
            ("reauth required", HelixError::ReauthRequired),
            ("rate limited", HelixError::RateLimited),
        ];

        for (label, error) in cases {
            let (_transport, runner) = runner_with(vec![users_fixture("555"), Err(error)]);
            let stack = ArgStack::new();

            let (telemetry, _) = runner.execute(&config("target"), &make_ctx(&stack)).await;

            assert!(
                matches!(telemetry.outcome, SubActionOutcome::Failed(_)),
                "{label}: {:?}",
                telemetry.outcome
            );
        }
    }

    #[tokio::test]
    async fn only_a_400_saying_not_banned_is_kept_out_of_request_fail() {
        let refusal = |status: reqwest::StatusCode, message: &str| {
            ResponseTemplate::new(status.as_u16()).set_body_json(
                serde_json::json!({"error": "x", "status": status.as_u16(), "message": message}),
            )
        };
        let cases = [
            (
                refusal(
                    reqwest::StatusCode::BAD_REQUEST,
                    "The user is not banned or in a timeout.",
                ),
                true,
                0,
            ),
            (
                refusal(reqwest::StatusCode::BAD_REQUEST, "USER IS NOT BANNED"),
                true,
                0,
            ),
            (
                refusal(
                    reqwest::StatusCode::BAD_REQUEST,
                    "Missing required parameter user_id",
                ),
                false,
                1,
            ),
            (
                refusal(reqwest::StatusCode::NOT_FOUND, "user is not banned"),
                false,
                1,
            ),
        ];

        for (reply, succeeds, request_fails) in cases {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/helix/users"))
                .respond_with(
                    ResponseTemplate::new(reqwest::StatusCode::OK.as_u16())
                        .set_body_json(users_fixture("555").unwrap()),
                )
                .mount(&server)
                .await;
            Mock::given(method("DELETE"))
                .and(path("/helix/moderation/bans"))
                .respond_with(reply)
                .mount(&server)
                .await;
            let (transport, bus) = observed_helix_at(&server.uri());
            let runner = UnbanUserRunner::new(
                transport,
                Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
            );
            let stack = ArgStack::new();

            let (telemetry, _) = runner.execute(&config("target"), &make_ctx(&stack)).await;

            assert_eq!(
                (
                    telemetry.outcome == SubActionOutcome::Success,
                    bus.of_kind(REQUEST_FAIL_KIND).len()
                ),
                (succeeds, request_fails),
                "{:?}",
                telemetry.outcome
            );
        }
    }
}

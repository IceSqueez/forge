use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use super::identity::SelfIdentity;
use super::user_target::{TargetKeys, UserTarget};
use crate::helix::{HelixError, HelixMethod, HelixRequest, HelixTransport};

const TARGET_KEYS: TargetKeys = TargetKeys {
    id: "target_user_id",
    login: "target_user_login",
};
const KIND_ID: &str = "twitch.moderation.unban_user";
const NOT_BANNED_MARKER: &str = "not banned";

fn already_unbanned(error: &HelixError) -> bool {
    match error {
        HelixError::Http { status, body } => {
            *status == reqwest::StatusCode::BAD_REQUEST.as_u16()
                && body.to_lowercase().contains(NOT_BANNED_MARKER)
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
            return SubActionOutcome::Failed(TARGET_KEYS.empty_after_interpolation());
        };
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(e.to_string()),
        };
        let target_user_id = match target.user_id(self.transport.as_ref()).await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(e.to_string()),
        };
        let request = HelixRequest::new(HelixMethod::Delete, "/helix/moderation/bans")
            .query("broadcaster_id", user_id.clone())
            .query("moderator_id", user_id)
            .query("user_id", target_user_id);
        match self.transport.execute(request).await {
            Err(error) if already_unbanned(&error) => SubActionOutcome::Success,
            result => SubActionOutcome::from_result(&result),
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

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([(TARGET_KEYS.login.to_owned(), Variant::String(String::new()))])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Text {
            key: TARGET_KEYS.login,
            label: "Target Username",
            placeholder: "%user_login%",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        TARGET_KEYS.validate(KIND_ID, config)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let interpolated = TARGET_KEYS.interpolate(config, ctx);

        let outcome = self.unban(interpolated.target()).await;

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
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::helix::HelixError;
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, SELF_USER_ID, make_ctx, users_fixture,
    };

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
}

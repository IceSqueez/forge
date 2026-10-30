use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigProblem, ConfigSpec, Interpolation, RunContext, StepTimer, SubActionCategory,
    SubActionRunner, TypedField,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, UserText};

use super::fields::{MODERATION_TARGET, OPTIONAL_REASON};
use crate::account::identity::SelfIdentity;
use crate::account::user_target::{InterpolatedTarget, UserTarget};
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};
use crate::vocabulary::sub_action_ids;

const KIND_ID: &str = sub_action_ids::MODERATION_BAN_USER;

const CONFIG: ConfigSpec = ConfigSpec::new(&[&MODERATION_TARGET.login, &OPTIONAL_REASON]);

fn read_config(
    config: &SubActionConfig,
    args: &ArgStack,
) -> Result<(InterpolatedTarget, String), ConfigProblem> {
    Ok((
        MODERATION_TARGET.read(config, args)?,
        OPTIONAL_REASON.read(config, args)?,
    ))
}

pub struct BanUserRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl BanUserRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn ban(&self, target: Option<UserTarget<'_>>, reason: &str) -> SubActionOutcome {
        let Some(target) = target else {
            return SubActionOutcome::Failed(UserText::unlocalized(
                MODERATION_TARGET.empty_after_interpolation(),
            ));
        };
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(UserText::unlocalized(e.to_string())),
        };
        let target_user_id = match target.user_id(self.transport.as_ref()).await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(UserText::unlocalized(e.to_string())),
        };
        let mut data = serde_json::json!({ "user_id": target_user_id });
        if !reason.is_empty() {
            data["reason"] = serde_json::Value::String(reason.to_owned());
        }
        let request = HelixRequest::new(HelixMethod::Post, "/helix/moderation/bans")
            .query("broadcaster_id", user_id.clone())
            .query("moderator_id", user_id)
            .body(serde_json::json!({ "data": data }));
        SubActionOutcome::from_result(&self.transport.execute(request).await)
    }
}

#[async_trait]
impl SubActionRunner for BanUserRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Moderation
    }

    fn label(&self) -> &str {
        "Ban User"
    }

    fn summary(&self) -> &str {
        "Permanently bans a user from the channel."
    }

    fn search_text(&self) -> &str {
        "twitch moderation ban user permanent remove"
    }

    fn icon_name(&self) -> &str {
        "ban"
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
        let (target, reason) = match read_config(config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        (timer.finish(self.ban(target.target(), &reason).await), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helix::HelixError;
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, SELF_USER_ID, TOKEN_SENTINEL, make_ctx, users_fixture,
    };
    use forge_types::Variant;
    use std::collections::BTreeMap;

    fn runner_with(
        responses: Vec<Result<serde_json::Value, HelixError>>,
    ) -> (Arc<MockTransport>, BanUserRunner) {
        let transport = Arc::new(MockTransport::returning_sequence(responses));
        let runner = BanUserRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        );
        (transport, runner)
    }

    fn config(target: &str, reason: &str) -> SubActionConfig {
        BTreeMap::from([
            (
                "target_user_login".to_owned(),
                Variant::String(target.to_owned()),
            ),
            ("reason".to_owned(), Variant::String(reason.to_owned())),
        ])
    }

    #[tokio::test]
    async fn execute_resolves_login_then_posts_ban_as_self_moderator() {
        let (transport, runner) =
            runner_with(vec![users_fixture("555"), Ok(serde_json::Value::Null)]);
        let stack = ArgStack::new().set(
            "user_login".to_owned(),
            Variant::String("target".to_owned()),
        );

        let (telemetry, _) = runner
            .execute(&config("%user_login%", "spam"), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert_eq!(transport.call_count(), 2, "resolve then ban");
        assert_eq!(transport.request(0).path, "/helix/users");
        let ban = transport.last_request();
        assert_eq!(ban.method, HelixMethod::Post);
        assert_eq!(ban.path, "/helix/moderation/bans");
        assert!(
            ban.query
                .contains(&("broadcaster_id".to_owned(), SELF_USER_ID.to_owned()))
        );
        assert!(
            ban.query
                .contains(&("moderator_id".to_owned(), SELF_USER_ID.to_owned()))
        );
        assert_eq!(
            ban.body,
            Some(serde_json::json!({ "data": { "user_id": "555", "reason": "spam" } })),
            "body must carry the RESOLVED id, not the login"
        );
    }

    #[tokio::test]
    async fn empty_reason_is_omitted_from_ban_body() {
        let (transport, runner) =
            runner_with(vec![users_fixture("555"), Ok(serde_json::Value::Null)]);
        let stack = ArgStack::new();

        let (telemetry, _) = runner
            .execute(&config("target", ""), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert_eq!(
            transport.last_request().body,
            Some(serde_json::json!({ "data": { "user_id": "555" } })),
            "empty reason must not produce a 'reason' key"
        );
    }

    #[tokio::test]
    async fn ban_call_http_failure_maps_to_failed_without_token_or_url() {
        let (transport, runner) = runner_with(vec![
            users_fixture("555"),
            Err(HelixError::Http {
                status: 403,
                body: "moderator scope missing".to_owned(),
            }),
        ]);
        let stack = ArgStack::new();

        let (telemetry, _) = runner
            .execute(&config("target", "spam"), &make_ctx(&stack))
            .await;

        assert_eq!(
            transport.call_count(),
            2,
            "failure must come from the ban call, not the resolve"
        );
        let SubActionOutcome::Failed(msg) = telemetry.outcome else {
            panic!("expected Failed, got {:?}", telemetry.outcome);
        };
        assert!(
            msg.code_or_text().contains("403"),
            "status must surface: {msg:?}"
        );
        assert!(
            !msg.code_or_text().contains(TOKEN_SENTINEL),
            "token leaked: {msg:?}"
        );
        assert!(
            !msg.code_or_text().contains("api.twitch.tv"),
            "URL leaked: {msg:?}"
        );
    }
}

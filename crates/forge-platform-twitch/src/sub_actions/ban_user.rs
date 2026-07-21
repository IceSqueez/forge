use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    FormField, RegistryError, RunContext, SubActionCategory, SubActionConfigExt, SubActionRunner,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use super::identity::{SelfIdentity, resolve_user_id};
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};

const KIND_ID: &str = "twitch.moderation.ban_user";

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

    async fn ban(&self, target_login: &str, reason: &str) -> SubActionOutcome {
        if target_login.is_empty() {
            return SubActionOutcome::Failed(
                "target_user_login is empty after interpolation".to_owned(),
            );
        }
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(e.to_string()),
        };
        let target_user_id = match resolve_user_id(self.transport.as_ref(), target_login).await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(e.to_string()),
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

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            (
                "target_user_login".to_owned(),
                Variant::String(String::new()),
            ),
            ("reason".to_owned(), Variant::String(String::new())),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "target_user_login",
                label: "Target Username",
                placeholder: "%user_login%",
            },
            FormField::Text {
                key: "reason",
                label: "Reason (optional, max 500 chars)",
                placeholder: "",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("target_user_login") {
            Some(Variant::String(s)) if !s.is_empty() => {}
            _ => {
                return Err(RegistryError::InvalidConfig(format!(
                    "{KIND_ID}: 'target_user_login' must be a non-empty string"
                )));
            }
        }
        if let Some(Variant::String(r)) = config.get("reason")
            && r.chars().count() > 500
        {
            return Err(RegistryError::InvalidConfig(format!(
                "{KIND_ID}: 'reason' must not exceed 500 characters"
            )));
        }
        Ok(())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let login_template = config.str("target_user_login").unwrap_or_default();
        let target_login = ctx.arg_stack.interpolate(login_template);
        let reason = config
            .str("reason")
            .map(|s| ctx.arg_stack.interpolate(s))
            .unwrap_or_default();

        let outcome = self.ban(&target_login, &reason).await;

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
    async fn empty_target_login_after_interpolation_fails_before_any_helix_call() {
        let (transport, runner) = runner_with(vec![users_fixture("555")]);
        let stack = ArgStack::new().set("user_login".to_owned(), Variant::String(String::new()));

        let (telemetry, _) = runner
            .execute(&config("%user_login%", "spam"), &make_ctx(&stack))
            .await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert_eq!(
            transport.call_count(),
            0,
            "empty target must fail before the resolve call"
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
        assert!(msg.contains("403"), "status must surface: {msg}");
        assert!(!msg.contains(TOKEN_SENTINEL), "token leaked: {msg}");
        assert!(!msg.contains("api.twitch.tv"), "URL leaked: {msg}");
    }
}

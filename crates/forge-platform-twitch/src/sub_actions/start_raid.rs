use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use super::identity::{SelfIdentity, resolve_user_id};
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};

const KIND_ID: &str = "twitch.channel.start_raid";

pub struct StartRaidRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl StartRaidRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn start_raid(&self, to_broadcaster_login: &str) -> SubActionOutcome {
        if to_broadcaster_login.is_empty() {
            return SubActionOutcome::Failed(
                "to_broadcaster_login is empty after interpolation".to_owned(),
            );
        }
        let self_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(e.to_string()),
        };
        let to_broadcaster_id =
            match resolve_user_id(self.transport.as_ref(), to_broadcaster_login).await {
                Ok(id) => id,
                Err(e) => return SubActionOutcome::Failed(e.to_string()),
            };
        let request = HelixRequest::new(HelixMethod::Post, "/helix/raids")
            .query("from_broadcaster_id", self_id)
            .query("to_broadcaster_id", to_broadcaster_id);
        match self.transport.execute(request).await {
            Ok(_) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        }
    }
}

#[async_trait]
impl SubActionRunner for StartRaidRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Twitch
    }

    fn label(&self) -> &str {
        "Start Raid"
    }

    fn summary(&self) -> &str {
        "Starts a raid to another broadcaster's channel."
    }

    fn search_text(&self) -> &str {
        "twitch raid start send viewers channel"
    }

    fn icon_name(&self) -> &str {
        "raid"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([(
            "to_broadcaster_login".to_owned(),
            Variant::String(String::new()),
        )])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Text {
            key: "to_broadcaster_login",
            label: "Target Channel",
            placeholder: "%user_login%",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("to_broadcaster_login") {
            Some(Variant::String(s)) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::UnknownKindId(format!(
                "{KIND_ID}: 'to_broadcaster_login' must be a non-empty string"
            ))),
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let login_template = config
            .get("to_broadcaster_login")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let to_broadcaster_login = ctx.arg_stack.interpolate(login_template);

        let outcome = self.start_raid(&to_broadcaster_login).await;

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
    ) -> (Arc<MockTransport>, StartRaidRunner) {
        let transport = Arc::new(MockTransport::returning_sequence(responses));
        let runner = StartRaidRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        );
        (transport, runner)
    }

    fn config(login: &str) -> SubActionConfig {
        BTreeMap::from([(
            "to_broadcaster_login".to_owned(),
            Variant::String(login.to_owned()),
        )])
    }

    #[tokio::test]
    async fn execute_resolves_login_then_posts_raid_from_self_to_resolved_id() {
        let (transport, runner) =
            runner_with(vec![users_fixture("555"), Ok(serde_json::Value::Null)]);
        let stack = ArgStack::new().set(
            "user_login".to_owned(),
            Variant::String("target".to_owned()),
        );

        let (telemetry, _) = runner
            .execute(&config("%user_login%"), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert_eq!(transport.call_count(), 2, "resolve then raid");

        let resolve = transport.request(0);
        assert_eq!(resolve.method, HelixMethod::Get);
        assert_eq!(resolve.path, "/helix/users");
        assert!(
            resolve
                .query
                .contains(&("login".to_owned(), "target".to_owned())),
            "resolve must look up the interpolated login: {:?}",
            resolve.query
        );

        let act = transport.last_request();
        assert_eq!(act.method, HelixMethod::Post);
        assert_eq!(act.path, "/helix/raids");
        assert!(
            act.query
                .contains(&("from_broadcaster_id".to_owned(), SELF_USER_ID.to_owned())),
            "from must be self: {:?}",
            act.query
        );
        assert!(
            act.query
                .contains(&("to_broadcaster_id".to_owned(), "555".to_owned())),
            "to must be the RESOLVED id, not the login: {:?}",
            act.query
        );
        assert_eq!(act.body, None, "raid carries no JSON body");
    }

    #[tokio::test]
    async fn empty_login_after_interpolation_fails_before_any_helix_call() {
        let (transport, runner) = runner_with(vec![users_fixture("555")]);
        let stack = ArgStack::new().set("user_login".to_owned(), Variant::String(String::new()));

        let (telemetry, _) = runner
            .execute(&config("%user_login%"), &make_ctx(&stack))
            .await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert_eq!(
            transport.call_count(),
            0,
            "empty login must fail before the resolve call"
        );
    }

    #[tokio::test]
    async fn resolve_failure_skips_the_raid_call() {
        let (transport, runner) = runner_with(vec![Err(HelixError::Http {
            status: 404,
            body: "user not found".to_owned(),
        })]);
        let stack = ArgStack::new();

        let (telemetry, _) = runner.execute(&config("ghost"), &make_ctx(&stack)).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert_eq!(
            transport.call_count(),
            1,
            "resolve failure must not issue the raid POST"
        );
    }

    #[tokio::test]
    async fn raid_http_failure_maps_to_failed_without_token_or_url() {
        let (transport, runner) = runner_with(vec![
            users_fixture("555"),
            Err(HelixError::Http {
                status: 409,
                body: "raid already pending".to_owned(),
            }),
        ]);
        let stack = ArgStack::new();

        let (telemetry, _) = runner.execute(&config("target"), &make_ctx(&stack)).await;

        assert_eq!(
            transport.call_count(),
            2,
            "failure must come from the raid call, not the resolve"
        );
        let SubActionOutcome::Failed(msg) = telemetry.outcome else {
            panic!("expected Failed, got {:?}", telemetry.outcome);
        };
        assert!(msg.contains("409"), "status must surface: {msg}");
        assert!(!msg.contains(TOKEN_SENTINEL), "token leaked: {msg}");
        assert!(!msg.contains("api.twitch.tv"), "URL leaked: {msg}");
    }
}

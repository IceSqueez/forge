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

const KIND_ID: &str = "twitch.moderation.add_moderator";

pub struct AddModeratorRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl AddModeratorRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn apply(&self, target_login: &str) -> SubActionOutcome {
        if target_login.is_empty() {
            return SubActionOutcome::Failed(
                "target_user_login is empty after interpolation".to_owned(),
            );
        }
        let broadcaster_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(e.to_string()),
        };
        let user_id = match resolve_user_id(self.transport.as_ref(), target_login).await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(e.to_string()),
        };
        let request = HelixRequest::new(HelixMethod::Post, "/helix/moderation/moderators")
            .query("broadcaster_id", broadcaster_id)
            .query("user_id", user_id);
        SubActionOutcome::from_result(&self.transport.execute(request).await)
    }
}

#[async_trait]
impl SubActionRunner for AddModeratorRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Moderation
    }

    fn label(&self) -> &str {
        "Add Moderator"
    }

    fn summary(&self) -> &str {
        "Grants moderator status to a user in the channel."
    }

    fn search_text(&self) -> &str {
        "twitch moderation add moderator grant mod role"
    }

    fn icon_name(&self) -> &str {
        "shield-plus"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([(
            "target_user_login".to_owned(),
            Variant::String(String::new()),
        )])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Text {
            key: "target_user_login",
            label: "Target Username",
            placeholder: "%user_login%",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("target_user_login") {
            Some(Variant::String(s)) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::InvalidConfig(format!(
                "{KIND_ID}: 'target_user_login' must be a non-empty string"
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

        let login_template = config.str("target_user_login").unwrap_or_default();
        let target_login = ctx.arg_stack.interpolate(login_template);

        let outcome = self.apply(&target_login).await;

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
    use std::collections::BTreeMap;

    use forge_types::{ArgStack, SubActionOutcome, Variant};

    use super::*;
    use crate::helix::{HelixError, HelixMethod};
    use crate::sub_actions::add_vip::AddVipRunner;
    use crate::sub_actions::remove_moderator::RemoveModeratorRunner;
    use crate::sub_actions::remove_vip::RemoveVipRunner;
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, SELF_USER_ID, TOKEN_SENTINEL, make_ctx, users_fixture,
    };

    fn runner_with(
        responses: Vec<Result<serde_json::Value, HelixError>>,
    ) -> (Arc<MockTransport>, AddModeratorRunner) {
        let transport = Arc::new(MockTransport::returning_sequence(responses));
        let runner = AddModeratorRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        );
        (transport, runner)
    }

    fn login_config(target: &str) -> SubActionConfig {
        BTreeMap::from([(
            "target_user_login".to_owned(),
            Variant::String(target.to_owned()),
        )])
    }

    // ── table-driven: all four resolve-then-act runners ───────────────────────
    //
    // Verifies that each runner (a) issues exactly 2 Helix calls, (b) first
    // resolves the login via GET /helix/users, and (c) issues the expected
    // method + path with broadcaster_id==SELF and user_id==resolved "555".

    #[tokio::test]
    async fn resolve_then_act_runners_each_call_users_then_action_endpoint() {
        struct Case {
            label: &'static str,
            runner: Box<dyn SubActionRunner>,
            transport: Arc<MockTransport>,
            expected_method: HelixMethod,
            expected_path: &'static str,
        }

        fn make_transport() -> Arc<MockTransport> {
            Arc::new(MockTransport::returning_sequence(vec![
                users_fixture("555"),
                Ok(serde_json::Value::Null),
            ]))
        }

        fn identity() -> Arc<SelfIdentity> {
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity())))
        }

        let t_add_mod = make_transport();
        let t_rem_mod = make_transport();
        let t_add_vip = make_transport();
        let t_rem_vip = make_transport();

        let cases: Vec<Case> = vec![
            Case {
                label: "add_moderator",
                runner: Box::new(AddModeratorRunner::new(
                    Arc::clone(&t_add_mod) as Arc<dyn HelixTransport>,
                    identity(),
                )),
                transport: t_add_mod,
                expected_method: HelixMethod::Post,
                expected_path: "/helix/moderation/moderators",
            },
            Case {
                label: "remove_moderator",
                runner: Box::new(RemoveModeratorRunner::new(
                    Arc::clone(&t_rem_mod) as Arc<dyn HelixTransport>,
                    identity(),
                )),
                transport: t_rem_mod,
                expected_method: HelixMethod::Delete,
                expected_path: "/helix/moderation/moderators",
            },
            Case {
                label: "add_vip",
                runner: Box::new(AddVipRunner::new(
                    Arc::clone(&t_add_vip) as Arc<dyn HelixTransport>,
                    identity(),
                )),
                transport: t_add_vip,
                expected_method: HelixMethod::Post,
                expected_path: "/helix/channels/vips",
            },
            Case {
                label: "remove_vip",
                runner: Box::new(RemoveVipRunner::new(
                    Arc::clone(&t_rem_vip) as Arc<dyn HelixTransport>,
                    identity(),
                )),
                transport: t_rem_vip,
                expected_method: HelixMethod::Delete,
                expected_path: "/helix/channels/vips",
            },
        ];

        let stack = ArgStack::new().set(
            "user_login".to_owned(),
            Variant::String("target".to_owned()),
        );
        let ctx = make_ctx(&stack);

        for case in cases {
            let config = login_config("%user_login%");
            let (telemetry, _) = case.runner.execute(&config, &ctx).await;

            assert_eq!(
                telemetry.outcome,
                SubActionOutcome::Success,
                "{}: expected Success",
                case.label
            );
            assert_eq!(
                case.transport.call_count(),
                2,
                "{}: must issue resolve + action (2 calls total)",
                case.label
            );
            assert_eq!(
                case.transport.request(0).path,
                "/helix/users",
                "{}: first call must resolve the login",
                case.label
            );
            let action_req = case.transport.last_request();
            assert_eq!(
                action_req.method, case.expected_method,
                "{}: wrong HTTP method",
                case.label
            );
            assert_eq!(
                action_req.path, case.expected_path,
                "{}: wrong endpoint path",
                case.label
            );
            assert!(
                action_req
                    .query
                    .contains(&("broadcaster_id".to_owned(), SELF_USER_ID.to_owned())),
                "{}: broadcaster_id must be the self/broadcaster id",
                case.label
            );
            assert!(
                action_req
                    .query
                    .contains(&("user_id".to_owned(), "555".to_owned())),
                "{}: user_id must be the resolved id, not the login",
                case.label
            );
        }
    }

    // ── pre-check: empty target_user_login skips all Helix calls ─────────────

    #[tokio::test]
    async fn empty_target_login_after_interpolation_fails_before_any_helix_call() {
        // Shared pre-check code path is the same across all four runners;
        // a single representative (AddModeratorRunner) is sufficient.
        let (transport, runner) = runner_with(vec![users_fixture("555")]);
        let stack = ArgStack::new().set("user_login".to_owned(), Variant::String(String::new()));

        let (telemetry, _) = runner
            .execute(&login_config("%user_login%"), &make_ctx(&stack))
            .await;

        assert!(
            matches!(telemetry.outcome, SubActionOutcome::Failed(_)),
            "empty login must produce Failed outcome"
        );
        assert_eq!(
            transport.call_count(),
            0,
            "empty login must not reach Helix"
        );
    }

    // ── action-call 4xx ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn action_call_http_error_maps_to_failed_without_token_or_url() {
        // A representative runner is sufficient (shared error-propagation path).
        let (transport, runner) = runner_with(vec![
            users_fixture("555"),
            Err(HelixError::Http {
                status: 422,
                body: "user is already a moderator".to_owned(),
            }),
        ]);
        let stack = ArgStack::new();

        let (telemetry, _) = runner
            .execute(&login_config("target"), &make_ctx(&stack))
            .await;

        assert_eq!(
            transport.call_count(),
            2,
            "failure must come from the action call, not the resolve"
        );
        let SubActionOutcome::Failed(msg) = telemetry.outcome else {
            panic!("expected Failed, got {:?}", telemetry.outcome);
        };
        assert!(msg.contains("422"), "HTTP status must surface: {msg}");
        assert!(!msg.contains(TOKEN_SENTINEL), "token must not leak: {msg}");
        assert!(!msg.contains("api.twitch.tv"), "URL must not leak: {msg}");
    }
}

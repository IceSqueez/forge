use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner, TextEditor, TextField,
    TypedField,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};

use crate::account::identity::SelfIdentity;
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::AUTOMOD_APPROVE_MESSAGE;
const MESSAGE_ID_TEMPLATE: &str = "%automod.message_id%";

const MESSAGE_ID: TextField = TextField {
    key: config_keys::MESSAGE_ID,
    label: "Message ID",
    editor: TextEditor::Line {
        placeholder: MESSAGE_ID_TEMPLATE,
    },
    default: MESSAGE_ID_TEMPLATE,
    required: true,
};

pub(crate) const AUTOMOD_CONFIG: ConfigSpec = ConfigSpec::new(&[&MESSAGE_ID]);

pub struct ApproveAutomodMessageRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl ApproveAutomodMessageRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }
}

async fn manage_automod_message(
    transport: &Arc<dyn HelixTransport>,
    identity: &Arc<SelfIdentity>,
    kind_id: &str,
    message_id: &str,
    action: &str,
) -> SubActionOutcome {
    if message_id.is_empty() {
        return SubActionOutcome::Failed("message_id is required".to_owned());
    }
    let user_id = match identity.user_id().await {
        Ok(id) => id,
        Err(e) => return SubActionOutcome::Failed(e.to_string()),
    };
    let body = serde_json::json!({
        "user_id": user_id,
        "msg_id": message_id,
        "action": action,
    });
    let request =
        HelixRequest::new(HelixMethod::Post, "/helix/moderation/automod/message").body(body);
    match transport.execute(request).await {
        Ok(_) => SubActionOutcome::Success,
        Err(e) => SubActionOutcome::Failed(format!("{kind_id}: {e}")),
    }
}

pub(crate) async fn execute_automod_runner(
    transport: &Arc<dyn HelixTransport>,
    identity: &Arc<SelfIdentity>,
    kind_id: &str,
    action: &str,
    config: &SubActionConfig,
    ctx: &RunContext<'_>,
) -> (SubActionTelemetry, Option<ArgStack>) {
    let timer = StepTimer::start(ctx, kind_id);
    let message_id = match MESSAGE_ID.read(config, ctx.arg_stack) {
        Ok(message_id) => message_id,
        Err(problem) => return (timer.invalid_config(&problem), None),
    };
    let outcome = manage_automod_message(transport, identity, kind_id, &message_id, action).await;
    (timer.finish(outcome), None)
}

#[async_trait]
impl SubActionRunner for ApproveAutomodMessageRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Moderation
    }

    fn label(&self) -> &str {
        "Approve AutoMod Message"
    }

    fn summary(&self) -> &str {
        "Allows a message held by AutoMod to appear in chat."
    }

    fn search_text(&self) -> &str {
        "twitch automod approve allow message held moderation"
    }

    fn icon_name(&self) -> &str {
        "check"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(AUTOMOD_CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        execute_automod_runner(
            &self.transport,
            &self.identity,
            KIND_ID,
            "ALLOW",
            config,
            ctx,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helix::{HelixError, HelixMethod, HelixTransport};
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, SELF_USER_ID, TOKEN_SENTINEL, make_ctx,
    };

    fn approve_runner_with(
        response: Result<serde_json::Value, HelixError>,
    ) -> (Arc<MockTransport>, ApproveAutomodMessageRunner) {
        let transport = Arc::new(MockTransport::returning(response));
        let runner = ApproveAutomodMessageRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        );
        (transport, runner)
    }

    fn config_with_message_id(value: &str) -> SubActionConfig {
        BTreeMap::from([("message_id".to_owned(), Variant::String(value.to_owned()))])
    }

    #[tokio::test]
    async fn approve_posts_allow_with_self_user_id_in_body_and_no_query() {
        let (transport, runner) = approve_runner_with(Ok(serde_json::Value::Null));
        let stack = ArgStack::new().set(
            "automod.message_id".to_owned(),
            Variant::String("msg77".to_owned()),
        );

        let (telemetry, out) = runner
            .execute(&automod_default_config(), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert!(out.is_none(), "automod runners never push an ArgStack");
        let request = transport.request(0);
        assert_eq!(request.method, HelixMethod::Post);
        assert_eq!(request.path, "/helix/moderation/automod/message");
        assert!(
            request.query.is_empty(),
            "automod fields go in the body, not the query: {:?}",
            request.query
        );
        assert_eq!(
            request.body,
            Some(serde_json::json!({
                "user_id": SELF_USER_ID,
                "msg_id": "msg77",
                "action": "ALLOW",
            })),
        );
    }

    #[tokio::test]
    async fn message_id_template_interpolates_from_stack() {
        let (transport, runner) = approve_runner_with(Ok(serde_json::Value::Null));
        let stack = ArgStack::new().set(
            "automod.message_id".to_owned(),
            Variant::String("resolved9".to_owned()),
        );

        let _ = runner
            .execute(&automod_default_config(), &make_ctx(&stack))
            .await;

        assert_eq!(
            transport.request(0).body.unwrap()["msg_id"],
            serde_json::json!("resolved9"),
            "msg_id must interpolate, not pass %automod.message_id% verbatim",
        );
    }

    #[tokio::test]
    async fn empty_message_id_fails_without_helix_call() {
        let (transport, runner) = approve_runner_with(Ok(serde_json::Value::Null));

        let (telemetry, _) = runner
            .execute(&config_with_message_id(""), &make_ctx(&ArgStack::new()))
            .await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert_eq!(
            transport.call_count(),
            0,
            "empty message_id must short-circuit before POST",
        );
    }

    #[tokio::test]
    async fn validate_config_requires_non_empty_message_id() {
        let (_transport, runner) = approve_runner_with(Ok(serde_json::Value::Null));

        let cases: Vec<(&str, SubActionConfig, bool)> = vec![
            ("present non-empty", config_with_message_id("m1"), true),
            ("empty string", config_with_message_id(""), false),
            ("missing key", BTreeMap::new(), false),
            (
                "wrong type",
                BTreeMap::from([("message_id".to_owned(), Variant::Int(7))]),
                false,
            ),
        ];

        for (label, config, expect_ok) in cases {
            assert_eq!(
                runner.validate_config(&config).is_ok(),
                expect_ok,
                "case: {label}",
            );
        }
    }

    #[tokio::test]
    async fn helix_failure_maps_to_failed_with_status_and_no_token() {
        let (_transport, runner) = approve_runner_with(Err(HelixError::Http {
            status: 403,
            body: "forbidden".to_owned(),
        }));
        let stack = ArgStack::new().set(
            "automod.message_id".to_owned(),
            Variant::String("msg77".to_owned()),
        );

        let (telemetry, _) = runner
            .execute(&automod_default_config(), &make_ctx(&stack))
            .await;

        assert!(matches!(
            telemetry.outcome,
            SubActionOutcome::Failed(msg) if msg.contains("403") && !msg.contains(TOKEN_SENTINEL)
        ));
    }
}

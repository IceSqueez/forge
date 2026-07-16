use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use super::identity::SelfIdentity;
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};

const KIND_ID: &str = "twitch.automod.approve_message";

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

pub(crate) fn automod_default_config() -> SubActionConfig {
    BTreeMap::from([(
        "message_id".to_owned(),
        Variant::String("%automod.message_id%".to_owned()),
    )])
}

pub(crate) fn automod_config_fields() -> Vec<FormField> {
    vec![FormField::Text {
        key: "message_id",
        label: "Message ID",
        placeholder: "%automod.message_id%",
    }]
}

pub(crate) fn validate_automod_config(
    kind_id: &str,
    config: &SubActionConfig,
) -> Result<(), RegistryError> {
    match config.get("message_id") {
        Some(Variant::String(s)) if !s.is_empty() => Ok(()),
        _ => Err(RegistryError::UnknownKindId(format!(
            "{kind_id}: 'message_id' is required"
        ))),
    }
}

/// POST /helix/moderation/automod/message to allow or deny a held AutoMod message.
///
/// `user_id` is the MODERATOR's own id (self), not the sender's id - the Twitch API
/// uses it to verify the caller has moderator rights on the channel.
/// `action` must be uppercase "ALLOW" or "DENY" (lowercase is rejected by Twitch).
/// All three fields go in the JSON body, not as query params.
///
/// Reference: https://dev.twitch.tv/docs/api/reference/#manage-held-automod-messages
pub(crate) async fn manage_automod_message(
    transport: &Arc<dyn HelixTransport>,
    identity: &Arc<SelfIdentity>,
    kind_id: &str,
    message_id: &str,
    action: &str,
) -> SubActionOutcome {
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
    let started_at = OffsetDateTime::now_utc();
    let start = Instant::now();

    let message_id_template = config
        .get("message_id")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let message_id = ctx.arg_stack.interpolate(message_id_template);

    let outcome = if message_id.is_empty() {
        SubActionOutcome::Failed("message_id is required".to_owned())
    } else {
        manage_automod_message(transport, identity, kind_id, &message_id, action).await
    };

    (
        SubActionTelemetry {
            kind: kind_id.to_owned(),
            started_at,
            duration_ms: start.elapsed().as_millis() as u64,
            outcome,
            index: ctx.index,
        },
        None,
    )
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

    fn default_config(&self) -> SubActionConfig {
        automod_default_config()
    }

    fn config_fields(&self) -> Vec<FormField> {
        automod_config_fields()
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        validate_automod_config(KIND_ID, config)
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
#[allow(clippy::unwrap_used)]
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

    // Distinct-body contract for approve_message: POST the automod/message endpoint
    // with NO query params and a body of exactly user_id(self) / msg_id(resolved) /
    // action "ALLOW". The self-as-user_id placement in the BODY (not query) is the
    // moderation-auth contract Twitch verifies; deny re-uses this shape and asserts
    // only its own "DENY" action in-file.
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

    // SHARED behavior (asserted ONCE via the representative runner): the message_id
    // template resolves through the ArgStack. Default config holds %automod.message_id%,
    // so the body msg_id must equal the stack-resolved value, not the literal template.
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

    // SHARED behavior: an empty message_id after interpolation fails BEFORE any Helix
    // call (no message targeted). An explicitly empty template is the deterministic case.
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

    // SHARED behavior: validate_config gates on a non-empty message_id String.
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

    // SHARED behavior: a Helix failure surfaces as Failed carrying the status, and
    // the sentinel token never leaks into the outcome message.
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

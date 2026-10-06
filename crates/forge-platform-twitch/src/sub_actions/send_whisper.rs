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

use super::identity::SelfIdentity;
use super::user_target::{TargetKeys, UserTarget};
use crate::chat::send_whisper_to;
use crate::helix::HelixTransport;

const KIND_ID: &str = "twitch.chat.send_whisper";
const RECIPIENT_KEYS: TargetKeys = TargetKeys {
    id: "to_user_id",
    login: "to_user_login",
};

pub struct SendWhisperRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl SendWhisperRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn whisper(&self, recipient: Option<UserTarget<'_>>, message: &str) -> SubActionOutcome {
        let from_user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(e.to_string()),
        };
        SubActionOutcome::from_result(
            &send_whisper_to(self.transport.as_ref(), &from_user_id, recipient, message).await,
        )
    }
}

#[async_trait]
impl SubActionRunner for SendWhisperRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Chat
    }

    fn label(&self) -> &str {
        "Send Whisper"
    }

    fn summary(&self) -> &str {
        "Sends a private whisper to another user."
    }

    fn search_text(&self) -> &str {
        "twitch whisper private message dm direct"
    }

    fn icon_name(&self) -> &str {
        "message"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            (
                RECIPIENT_KEYS.login.to_owned(),
                Variant::String(String::new()),
            ),
            ("message".to_owned(), Variant::String(String::new())),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: RECIPIENT_KEYS.login,
                label: "Recipient Username",
                placeholder: "%user_login%",
            },
            FormField::TextArea {
                key: "message",
                label: "Message",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        RECIPIENT_KEYS.validate(KIND_ID, config)?;
        match config.get("message") {
            Some(Variant::String(s)) if !s.is_empty() => {}
            _ => {
                return Err(RegistryError::InvalidConfig(format!(
                    "{KIND_ID}: 'message' must be a non-empty string"
                )));
            }
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

        let recipient = RECIPIENT_KEYS.interpolate(config, ctx);

        let msg_template = config.str("message").unwrap_or_default();
        let message = ctx.arg_stack.interpolate(msg_template);

        let outcome = self.whisper(recipient.target(), &message).await;

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
    use crate::helix::HelixError;
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, SELF_USER_ID, TOKEN_SENTINEL, make_ctx, users_fixture,
    };

    fn runner_with(
        responses: Vec<Result<serde_json::Value, HelixError>>,
    ) -> (Arc<MockTransport>, SendWhisperRunner) {
        let transport = Arc::new(MockTransport::returning_sequence(responses));
        let runner = SendWhisperRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        );
        (transport, runner)
    }

    fn config(to_user_login: &str, message: &str) -> SubActionConfig {
        BTreeMap::from([
            (
                "to_user_login".to_owned(),
                Variant::String(to_user_login.to_owned()),
            ),
            ("message".to_owned(), Variant::String(message.to_owned())),
        ])
    }

    #[tokio::test]
    async fn execute_whispers_from_the_signed_in_identity() {
        let (transport, runner) =
            runner_with(vec![users_fixture("555"), Ok(serde_json::Value::Null)]);
        let stack = ArgStack::new();

        let (telemetry, _) = runner
            .execute(&config("target", "hey there"), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert!(
            transport
                .last_request()
                .query
                .contains(&("from_user_id".to_owned(), SELF_USER_ID.to_owned())),
            "from_user_id must be the signed-in identity"
        );
    }

    #[tokio::test]
    async fn empty_message_after_interpolation_fails_without_any_helix_call() {
        let (transport, runner) = runner_with(vec![users_fixture("555")]);
        let stack = ArgStack::new().set("msg".to_owned(), Variant::String(String::new()));

        let (telemetry, _) = runner
            .execute(&config("target_user", "%msg%"), &make_ctx(&stack))
            .await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert_eq!(transport.call_count(), 0);
    }

    #[tokio::test]
    async fn whisper_call_http_failure_maps_to_failed_without_token_or_url() {
        let (transport, runner) = runner_with(vec![
            users_fixture("555"),
            Err(HelixError::Http {
                status: 403,
                body: "missing whisper scope".to_owned(),
            }),
        ]);
        let stack = ArgStack::new();

        let (telemetry, _) = runner
            .execute(&config("target", "hello"), &make_ctx(&stack))
            .await;

        assert_eq!(
            transport.call_count(),
            2,
            "failure must come from the whisper call, not the resolve"
        );
        let SubActionOutcome::Failed(msg) = telemetry.outcome else {
            panic!("expected Failed, got {:?}", telemetry.outcome);
        };
        assert!(msg.contains("403"), "status must surface: {msg}");
        assert!(!msg.contains(TOKEN_SENTINEL), "token leaked: {msg}");
        assert!(!msg.contains("api.twitch.tv"), "URL leaked: {msg}");
    }
}

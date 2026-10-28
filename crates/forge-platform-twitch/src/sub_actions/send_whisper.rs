use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_events::EventPublisher;
use forge_platform_core::{SENDER_FIELD, SenderPreference};
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigReason, FormField, RegistryError, RunContext, SubActionCategory, SubActionConfigExt,
    SubActionRunner,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::account::identity::SelfIdentity;
use crate::account::user_target::{TargetKeys, UserTarget};
use crate::bot_account::TwitchBotAccount;
use crate::chat_sender::{ChatDelivery, ChatSenders, StepResult, configured_sender, step_result};
use crate::helix::HelixTransport;

const KIND_ID: &str = "twitch.chat.send_whisper";
const RECIPIENT_KEYS: TargetKeys = TargetKeys {
    id: "to_user_id",
    login: "to_user_login",
};

pub struct SendWhisperRunner {
    senders: ChatSenders,
}

impl SendWhisperRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            senders: ChatSenders::new(transport, identity),
        }
    }

    pub fn with_bot(mut self, bot: Option<Arc<TwitchBotAccount>>) -> Self {
        self.senders = self.senders.with_bot(bot);
        self
    }

    async fn whisper(
        &self,
        recipient: Option<UserTarget<'_>>,
        message: &str,
        sender: SenderPreference,
        events: &dyn EventPublisher,
    ) -> StepResult {
        let delivery = ChatDelivery::WhisperToTarget {
            recipient,
            text: message,
        };
        step_result(self.senders.deliver(delivery, sender, events).await)
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
            (
                SENDER_FIELD.to_owned(),
                Variant::String(SenderPreference::Default.as_str().to_owned()),
            ),
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
            FormField::Select {
                key: SENDER_FIELD,
                label: "Send As",
                options: SenderPreference::WIRE_NAMES,
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        RECIPIENT_KEYS.validate(config)?;
        match config.get("message") {
            Some(Variant::String(s)) if !s.is_empty() => {}
            _ => {
                return Err(RegistryError::invalid_field(
                    "message",
                    ConfigReason::Required,
                ));
            }
        }
        configured_sender(config).map(|_| ())
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

        let (outcome, produced) = match configured_sender(config) {
            Ok(sender) => {
                self.whisper(recipient.target(), &message, sender, ctx.publisher)
                    .await
            }
            Err(e) => (SubActionOutcome::Failed(e.to_string()), BTreeMap::new()),
        };

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced,
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

    #[tokio::test]
    async fn execute_whispers_as_the_connected_bot_and_records_it_as_the_sender() {
        use crate::bot_account::test_support::{BOT_LOGIN, BOT_USER_ID, BotHarness};
        use wiremock::ResponseTemplate;

        let harness = BotHarness::connected().await;
        harness
            .mount(
                "GET",
                "/helix/users",
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"data": [{"id": "555", "login": "target"}]})),
            )
            .await;
        harness
            .mount("POST", "/helix/whispers", ResponseTemplate::new(204))
            .await;
        let streamer = Arc::new(MockTransport::returning(Ok(serde_json::Value::Null)));
        let runner = SendWhisperRunner::new(
            Arc::clone(&streamer) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::clone(&harness.repo) as _)),
        )
        .with_bot(Some(Arc::clone(&harness.bot)));

        let (telemetry, _) = runner
            .execute(&config("target", "hey"), &make_ctx(&ArgStack::new()))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(
                &telemetry.produced[forge_platform_core::SENT_AS_FIELD]
            )
            .unwrap(),
            serde_json::json!({ "role": "bot", "login": BOT_LOGIN })
        );
        let whisper = &harness.requests_to("/helix/whispers").await[0];
        assert!(
            whisper
                .url
                .query_pairs()
                .any(|(key, value)| key == "from_user_id" && value == BOT_USER_ID)
        );
        assert_eq!(streamer.call_count(), 0);
    }

    #[tokio::test]
    async fn unknown_sender_is_rejected_by_validate_and_by_execute_without_a_helix_call() {
        let (transport, runner) = runner_with(vec![users_fixture("555")]);
        let mut config = config("target", "hey");
        config.insert(SENDER_FIELD.to_owned(), Variant::Int(2));

        let validation = runner.validate_config(&config);
        let (telemetry, _) = runner.execute(&config, &make_ctx(&ArgStack::new())).await;

        assert!(validation.is_err());
        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert_eq!(transport.call_count(), 0);
    }
}

use std::sync::Arc;

use async_trait::async_trait;
use forge_events::EventPublisher;
use forge_platform_core::SenderPreference;
use forge_registry::SubActionConfig;
use forge_registry::{
    ConfigProblem, ConfigSpec, Interpolation, RunContext, StepTimer, SubActionCategory,
    SubActionRunner, TextEditor, TextField, TypedField,
};
use forge_types::{ArgStack, SubActionTelemetry};

use super::fields::{SENDER, SendAs, TARGET_LOGIN};
use crate::account::identity::SelfIdentity;
use crate::account::user_target::{InterpolatedTarget, TargetFields, UserTarget};
use crate::bot_account::TwitchBotAccount;
use crate::chat_sender::{ChatDelivery, ChatSenders, StepResult, step_result};
use crate::helix::HelixTransport;
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::CHAT_SEND_WHISPER;

const RECIPIENT: TargetFields = TargetFields {
    id: TextField {
        key: config_keys::TO_USER_ID,
        label: "Recipient User ID",
        editor: TextEditor::Line { placeholder: "" },
        default: "",
        required: false,
    },
    login: TextField {
        key: config_keys::TO_USER_LOGIN,
        label: "Recipient Username",
        required: false,
        ..TARGET_LOGIN
    },
};

const MESSAGE: TextField = TextField {
    key: config_keys::MESSAGE,
    label: "Message",
    editor: TextEditor::Area,
    default: "",
    required: true,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&RECIPIENT.login, &MESSAGE, &SENDER]);

struct WhisperConfig {
    recipient: InterpolatedTarget,
    message: String,
    sender: SendAs,
}

fn read_config(config: &SubActionConfig, args: &ArgStack) -> Result<WhisperConfig, ConfigProblem> {
    Ok(WhisperConfig {
        recipient: RECIPIENT.read(config, args)?,
        message: MESSAGE.read(config, args)?,
        sender: SENDER.read(config, args)?,
    })
}

pub(crate) struct SendWhisperRunner {
    senders: ChatSenders,
}

impl SendWhisperRunner {
    pub(crate) fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            senders: ChatSenders::new(transport, identity),
        }
    }

    pub(crate) fn with_bot(mut self, bot: Option<Arc<TwitchBotAccount>>) -> Self {
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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn check_config(
        &self,
        config: &SubActionConfig,
        interpolation: Interpolation<'_>,
    ) -> Result<(), ConfigProblem> {
        RECIPIENT.check(config, interpolation)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, KIND_ID);
        let read = match read_config(config, ctx.arg_stack) {
            Ok(read) => read,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let (outcome, produced) = self
            .whisper(
                read.recipient.target(),
                &read.message,
                read.sender.0,
                ctx.publisher,
            )
            .await;
        let mut telemetry = timer.finish(outcome);
        telemetry.produced = produced;
        (telemetry, None)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use forge_types::{ArgStack, SubActionOutcome, Variant};

    use super::*;
    use crate::helix::HelixError;
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, SELF_USER_ID, assert_request_refused, make_ctx, users_fixture,
    };
    use forge_platform_core::SENDER_FIELD;

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
        assert_request_refused(&telemetry.outcome, 403);
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

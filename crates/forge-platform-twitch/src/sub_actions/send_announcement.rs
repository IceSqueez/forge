use crate::vocabulary::labels;
use forge_types::MessageCode;
use std::sync::Arc;

use async_trait::async_trait;
use forge_events::EventPublisher;
use forge_platform_core::SenderPreference;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ChoiceField, ConfigChoice, ConfigProblem, ConfigSpec, RunContext, StepTimer, SubActionCategory,
    SubActionRunner, TextEditor, TextField, TypedField,
};
use forge_types::{ArgStack, SubActionTelemetry};

use super::fields::{SENDER, SendAs};
use crate::account::identity::SelfIdentity;
use crate::bot_account::TwitchBotAccount;
use crate::cadence::CadenceLedger;
use crate::chat_sender::{ChatDelivery, ChatSenders, StepResult, failed_step, step_result};
use crate::helix::HelixTransport;
use crate::vocabulary::{config_keys, messages, sub_action_ids};

const KIND_ID: &str = sub_action_ids::CHAT_SEND_ANNOUNCEMENT;
const MAX_MESSAGE_CHARS: usize = 500;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AnnouncementColor {
    Primary,
    Blue,
    Green,
    Orange,
    Purple,
}

impl AnnouncementColor {
    pub(crate) fn label(self) -> MessageCode {
        match self {
            Self::Primary => labels::PRIMARY,
            Self::Blue => labels::BLUE,
            Self::Green => labels::GREEN,
            Self::Orange => labels::ORANGE,
            Self::Purple => labels::PURPLE,
        }
    }
}

impl ConfigChoice for AnnouncementColor {
    const ALL: &'static [Self] = &[
        Self::Primary,
        Self::Blue,
        Self::Green,
        Self::Orange,
        Self::Purple,
    ];
    const WIRE_NAMES: &'static [&'static str] = &["primary", "blue", "green", "orange", "purple"];

    fn wire_name(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Blue => "blue",
            Self::Green => "green",
            Self::Orange => "orange",
            Self::Purple => "purple",
        }
    }
}

const MESSAGE: TextField = TextField {
    key: config_keys::MESSAGE,
    label: "Message",
    editor: TextEditor::Area,
    default: "",
    required: true,
};

pub(crate) const COLOR: ChoiceField<AnnouncementColor> = ChoiceField {
    key: config_keys::COLOR,
    label: "Color",
    default: AnnouncementColor::Primary,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&MESSAGE, &COLOR, &SENDER]);

struct AnnouncementConfig {
    message: String,
    color: AnnouncementColor,
    sender: SendAs,
}

fn read_config(
    config: &SubActionConfig,
    args: &ArgStack,
) -> Result<AnnouncementConfig, ConfigProblem> {
    Ok(AnnouncementConfig {
        message: MESSAGE.read(config, args)?,
        color: COLOR.read(config, args)?,
        sender: SENDER.read(config, args)?,
    })
}

pub struct SendAnnouncementRunner {
    senders: ChatSenders,
}

impl SendAnnouncementRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            senders: ChatSenders::new(transport, identity),
        }
    }

    pub fn with_bot(mut self, bot: Option<Arc<TwitchBotAccount>>) -> Self {
        self.senders = self.senders.with_bot(bot);
        self
    }

    pub(crate) fn with_cadence(mut self, cadence: Arc<CadenceLedger>) -> Self {
        self.senders = self.senders.with_cadence(cadence);
        self
    }

    async fn send(
        &self,
        message: &str,
        color: AnnouncementColor,
        sender: SenderPreference,
        events: &dyn EventPublisher,
    ) -> StepResult {
        if message.chars().count() > MAX_MESSAGE_CHARS {
            return failed_step(messages::message_too_long(MAX_MESSAGE_CHARS));
        }
        let delivery = ChatDelivery::Announcement {
            text: message,
            color: color.wire_name(),
        };
        step_result(self.senders.deliver(delivery, sender, events).await)
    }
}

#[async_trait]
impl SubActionRunner for SendAnnouncementRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Chat
    }

    fn label(&self) -> &str {
        "Send Announcement"
    }

    fn summary(&self) -> &str {
        "Posts a highlighted announcement in the Twitch chat."
    }

    fn search_text(&self) -> &str {
        "twitch chat announcement announce highlight banner notice"
    }

    fn icon_name(&self) -> &str {
        "speakerphone"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
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
            .send(&read.message, read.color, read.sender.0, ctx.publisher)
            .await;
        let mut telemetry = timer.finish(outcome);
        telemetry.produced = produced;
        (telemetry, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helix::{HelixError, HelixMethod};
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, SELF_USER_ID, assert_failed_with_code, assert_failed_with_detail,
        make_ctx,
    };
    use forge_platform_core::SENDER_FIELD;
    use forge_types::SubActionOutcome;
    use forge_types::Variant;
    use std::collections::BTreeMap;

    fn runner_with(
        response: Result<serde_json::Value, HelixError>,
        creds: MockCreds,
    ) -> (Arc<MockTransport>, SendAnnouncementRunner) {
        let transport = Arc::new(MockTransport::returning(response));
        let runner = SendAnnouncementRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(creds))),
        );
        (transport, runner)
    }

    fn config(message: &str, color: Option<&str>) -> SubActionConfig {
        let mut c = BTreeMap::from([("message".to_owned(), Variant::String(message.to_owned()))]);
        if let Some(color) = color {
            c.insert("color".to_owned(), Variant::String(color.to_owned()));
        }
        c
    }

    #[tokio::test]
    async fn execute_posts_announcement_with_self_as_broadcaster_and_moderator() {
        let (transport, runner) =
            runner_with(Ok(serde_json::Value::Null), MockCreds::with_identity());
        let stack = ArgStack::new();

        let (telemetry, _) = runner
            .execute(&config("big news", Some("blue")), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let request = transport.last_request();
        assert_eq!(request.method, HelixMethod::Post);
        assert_eq!(request.path, "/helix/chat/announcements");
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
        let body = request.body.unwrap();
        assert_eq!(body["message"], "big news");
        assert_eq!(body["color"], "blue");
    }

    #[tokio::test]
    async fn execute_interpolates_message_template_before_send() {
        let (transport, runner) =
            runner_with(Ok(serde_json::Value::Null), MockCreds::with_identity());
        let stack = ArgStack::new().set("user".to_owned(), Variant::String("viewer42".to_owned()));

        let (telemetry, _) = runner
            .execute(&config("Welcome %user%!", None), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert_eq!(
            transport.last_request().body.unwrap()["message"],
            "Welcome viewer42!"
        );
    }

    #[tokio::test]
    async fn helix_http_failure_maps_to_failed_outcome_without_token_or_url() {
        let (_transport, runner) = runner_with(
            Err(HelixError::Http {
                status: 403,
                body: "moderator scope missing".to_owned(),
            }),
            MockCreds::with_identity(),
        );
        let stack = ArgStack::new();

        let (telemetry, _) = runner
            .execute(&config("big news", None), &make_ctx(&stack))
            .await;

        assert_failed_with_detail(&telemetry.outcome, "twitch.permission_denied", "HTTP 403");
    }

    #[tokio::test]
    async fn empty_message_after_interpolation_fails_without_transport_call() {
        let (transport, runner) =
            runner_with(Ok(serde_json::Value::Null), MockCreds::with_identity());
        let stack = ArgStack::new().set("greeting".to_owned(), Variant::String(String::new()));

        let (telemetry, _) = runner
            .execute(&config("%greeting%", None), &make_ctx(&stack))
            .await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert_eq!(
            transport.call_count(),
            0,
            "empty message must not reach Helix"
        );
    }

    #[tokio::test]
    async fn message_limit_enforced_by_character_count_not_byte_count() {
        for (char_count, should_send) in [(500, true), (501, false)] {
            let (transport, runner) =
                runner_with(Ok(serde_json::Value::Null), MockCreds::with_identity());
            let stack =
                ArgStack::new().set("msg".to_owned(), Variant::String("я".repeat(char_count)));

            let (telemetry, _) = runner
                .execute(&config("%msg%", None), &make_ctx(&stack))
                .await;

            if should_send {
                assert_eq!(
                    telemetry.outcome,
                    SubActionOutcome::Success,
                    "{char_count}-char message must send"
                );
                assert_eq!(transport.call_count(), 1);
            } else {
                assert!(
                    matches!(telemetry.outcome, SubActionOutcome::Failed(_)),
                    "{char_count}-char message must fail"
                );
                assert_eq!(
                    transport.call_count(),
                    0,
                    "over-limit message must not reach Helix"
                );
            }
        }
    }

    #[tokio::test]
    async fn missing_credentials_fail_without_transport_call() {
        let (transport, runner) = runner_with(Ok(serde_json::Value::Null), MockCreds::empty());
        let stack = ArgStack::new();

        let (telemetry, _) = runner
            .execute(&config("big news", None), &make_ctx(&stack))
            .await;

        assert_failed_with_code(&telemetry.outcome, "twitch.credentials_unavailable");
        assert_eq!(transport.call_count(), 0, "no identity, no request");
    }

    #[tokio::test]
    async fn execute_fails_an_unrecognized_stored_color_without_calling_helix() {
        let (transport, runner) =
            runner_with(Ok(serde_json::Value::Null), MockCreds::with_identity());
        let stack = ArgStack::new();

        let (telemetry, _) = runner
            .execute(&config("big news", Some("magenta")), &make_ctx(&stack))
            .await;

        assert!(
            matches!(&telemetry.outcome, SubActionOutcome::Failed(msg) if crate::sub_actions::test_support::rejected_field(msg) == Some("color")),
            "{:?}",
            telemetry.outcome
        );
        assert_eq!(transport.call_count(), 0);
    }

    #[test]
    fn validate_config_rejects_bad_message_or_color_and_accepts_valid() {
        let cases: Vec<(&str, SubActionConfig, bool)> = vec![
            ("valid message and color", config("hi", Some("green")), true),
            ("valid message, absent color", config("hi", None), true),
            ("empty message", config("", Some("blue")), false),
            ("missing message", BTreeMap::new(), false),
            ("unknown color", config("hi", Some("magenta")), false),
            (
                "non-string message",
                BTreeMap::from([("message".to_owned(), Variant::Int(3))]),
                false,
            ),
            (
                "non-string color",
                BTreeMap::from([
                    ("message".to_owned(), Variant::String("hi".to_owned())),
                    ("color".to_owned(), Variant::Int(1)),
                ]),
                false,
            ),
        ];
        let (_transport, runner) =
            runner_with(Ok(serde_json::Value::Null), MockCreds::with_identity());

        for (label, cfg, expect_ok) in cases {
            assert_eq!(
                runner.validate_config(&cfg).is_ok(),
                expect_ok,
                "case: {label}"
            );
        }
    }

    #[tokio::test]
    async fn the_acting_sender_is_recorded_only_for_a_sent_announcement() {
        let cases = [
            (
                Ok(serde_json::Value::Null),
                Some(serde_json::json!({"role": "streamer", "login": "streamer"})),
            ),
            (
                Err(HelixError::Http {
                    status: 400,
                    body: "channel is offline".to_owned(),
                }),
                None,
            ),
        ];

        for (response, expected) in cases {
            let (_transport, runner) = runner_with(response, MockCreds::with_identity());

            let (telemetry, _) = runner
                .execute(&config("hello", None), &make_ctx(&ArgStack::new()))
                .await;

            let recorded = telemetry
                .produced
                .get(forge_platform_core::SENT_AS_FIELD)
                .map(|json| serde_json::from_str::<serde_json::Value>(json).unwrap());
            assert_eq!(recorded, expected);
        }
    }

    #[tokio::test]
    async fn unknown_sender_is_rejected_by_validate_and_by_execute_without_a_helix_call() {
        let (transport, runner) =
            runner_with(Ok(serde_json::Value::Null), MockCreds::with_identity());
        let mut config = config("hello", None);
        config.insert(SENDER_FIELD.to_owned(), Variant::String("mod".to_owned()));

        let validation = runner.validate_config(&config);
        let (telemetry, _) = runner.execute(&config, &make_ctx(&ArgStack::new())).await;

        assert!(validation.is_err());
        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert_eq!(transport.call_count(), 0);
    }
}

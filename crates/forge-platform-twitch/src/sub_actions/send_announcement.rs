use crate::vocabulary::sub_action_ids;
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
use forge_types::{ArgStack, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::account::identity::SelfIdentity;
use crate::bot_account::TwitchBotAccount;
use crate::cadence::CadenceLedger;
use crate::chat_sender::{
    ChatDelivery, ChatSenders, StepResult, configured_sender, failed_step, step_result,
};
use crate::helix::HelixTransport;

const KIND_ID: &str = sub_action_ids::CHAT_SEND_ANNOUNCEMENT;
const MAX_MESSAGE_CHARS: usize = 500;
const COLORS: &[&str] = &["primary", "blue", "green", "orange", "purple"];
const DEFAULT_COLOR: &str = "primary";

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
        color: &str,
        sender: SenderPreference,
        events: &dyn EventPublisher,
    ) -> StepResult {
        if message.is_empty() {
            return failed_step("message is empty after interpolation");
        }
        if message.chars().count() > MAX_MESSAGE_CHARS {
            return failed_step("message exceeds 500-character limit");
        }
        let delivery = ChatDelivery::Announcement {
            text: message,
            color,
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

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            ("message".to_owned(), Variant::String(String::new())),
            (
                "color".to_owned(),
                Variant::String(DEFAULT_COLOR.to_owned()),
            ),
            (
                SENDER_FIELD.to_owned(),
                Variant::String(SenderPreference::Default.as_str().to_owned()),
            ),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::TextArea {
                key: "message",
                label: "Message",
            },
            FormField::Select {
                key: "color",
                label: "Color",
                options: COLORS,
            },
            FormField::Select {
                key: SENDER_FIELD,
                label: "Send As",
                options: SenderPreference::WIRE_NAMES,
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("message") {
            Some(Variant::String(s)) if !s.is_empty() => {}
            _ => {
                return Err(RegistryError::invalid_field(
                    "message",
                    ConfigReason::Required,
                ));
            }
        }
        match config.get("color") {
            None => {}
            Some(Variant::String(c)) if COLORS.contains(&c.as_str()) => {}
            _ => {
                return Err(RegistryError::invalid_field(
                    "color",
                    ConfigReason::choices(["blue", "green", "orange", "purple", "primary"]),
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

        let template = config.str("message").unwrap_or_default();
        let message = ctx.arg_stack.interpolate(template);
        let color = config
            .str("color")
            .filter(|c| COLORS.contains(c))
            .unwrap_or(DEFAULT_COLOR);

        let (outcome, produced) = match configured_sender(config) {
            Ok(sender) => self.send(&message, color, sender, ctx.publisher).await,
            Err(e) => failed_step(&e.to_string()),
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
    use super::*;
    use crate::helix::{HelixError, HelixMethod};
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, SELF_USER_ID, TOKEN_SENTINEL, make_ctx,
    };
    use forge_types::SubActionOutcome;

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

        let SubActionOutcome::Failed(msg) = telemetry.outcome else {
            panic!("expected Failed, got {:?}", telemetry.outcome);
        };
        assert!(
            msg.contains("403"),
            "status must surface for diagnosis: {msg}"
        );
        assert!(
            !msg.contains(TOKEN_SENTINEL),
            "outcome must not leak the token: {msg}"
        );
        assert!(
            !msg.contains("api.twitch.tv"),
            "outcome must not leak the request URL: {msg}"
        );
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

        assert!(matches!(
            telemetry.outcome,
            SubActionOutcome::Failed(msg) if msg.contains("no twitch credentials")
        ));
        assert_eq!(transport.call_count(), 0, "no identity, no request");
    }

    #[tokio::test]
    async fn execute_falls_back_to_primary_color_for_unrecognized_stored_color() {
        let (transport, runner) =
            runner_with(Ok(serde_json::Value::Null), MockCreds::with_identity());
        let stack = ArgStack::new();

        let (telemetry, _) = runner
            .execute(&config("big news", Some("magenta")), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert_eq!(transport.last_request().body.unwrap()["color"], "primary");
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

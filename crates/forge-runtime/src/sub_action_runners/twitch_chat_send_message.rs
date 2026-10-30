use async_trait::async_trait;
use forge_events::{ChatSendRequest, EventSource, TypedEvent};
use forge_platform_core::{SENDER_FIELD, SenderPreference};
use forge_registry::{
    ChoiceField, ConfigChoice, ConfigProblem, ConfigSpec, RunContext, StepTimer, SubActionCategory,
    SubActionRunner, TextEditor, TextField, TypedField,
};
use forge_types::{
    ArgStack, IntegrationId, MessageRef, NO_CHAT_PLATFORM_ENABLED, SubActionConfig,
    SubActionTelemetry, resolve_chat_target,
};

pub const CHAT_SEND_KIND_ID: &str = "twitch.chat.send_message";
pub const CHAT_SEND_DEFAULT_TARGET: &str = "twitch";
pub const CHAT_SEND_MESSAGE_KEY: &str = "message";
pub const CHAT_SEND_TARGET_KEY: &str = "target";

#[derive(Clone, Copy)]
struct SendAs(SenderPreference);

impl ConfigChoice for SendAs {
    const ALL: &'static [Self] = &[
        Self(SenderPreference::Default),
        Self(SenderPreference::Streamer),
        Self(SenderPreference::BotOnly),
    ];
    const WIRE_NAMES: &'static [&'static str] = SenderPreference::WIRE_NAMES;

    fn wire_name(self) -> &'static str {
        self.0.as_str()
    }
}

const MESSAGE: TextField = TextField {
    key: CHAT_SEND_MESSAGE_KEY,
    label: "Message",
    editor: TextEditor::Area,
    default: "",
    required: true,
};

const TARGET: TextField = TextField {
    key: CHAT_SEND_TARGET_KEY,
    label: "Target Platform",
    editor: TextEditor::Line {
        placeholder: CHAT_SEND_DEFAULT_TARGET,
    },
    default: CHAT_SEND_DEFAULT_TARGET,
    required: false,
};

const SENDER: ChoiceField<SendAs> = ChoiceField {
    key: SENDER_FIELD,
    label: "Send As",
    default: SendAs(SenderPreference::Default),
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&MESSAGE, &TARGET, &SENDER]);

struct ChatSendConfig {
    message: String,
    target: String,
    sender: SenderPreference,
}

fn read_config(config: &SubActionConfig, args: &ArgStack) -> Result<ChatSendConfig, ConfigProblem> {
    Ok(ChatSendConfig {
        message: MESSAGE.read(config, args)?,
        target: TARGET.read(config, args)?,
        sender: SENDER.read(config, args)?.0,
    })
}

pub struct TwitchChatSendMessageRunner;

#[async_trait]
impl SubActionRunner for TwitchChatSendMessageRunner {
    fn id(&self) -> &str {
        CHAT_SEND_KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Chat
    }

    fn label(&self) -> &str {
        "Send Chat Message"
    }

    fn summary(&self) -> &str {
        "Send a message to a platform chat channel"
    }

    fn search_text(&self) -> &str {
        "send chat message twitch write post"
    }

    fn icon_name(&self) -> &str {
        "message-circle"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn targeted_integration(
        &self,
        config: &SubActionConfig,
        arg_stack: &ArgStack,
    ) -> Option<IntegrationId> {
        let target = TARGET.read(config, arg_stack).ok()?;
        resolve_chat_target(&target)
            .ok()
            .flatten()
            .map(|platform| IntegrationId::from_static(platform.as_str()))
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, CHAT_SEND_KIND_ID);
        let ChatSendConfig {
            message,
            target,
            sender,
        } = match read_config(config, ctx.arg_stack) {
            Ok(read) => read,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let target = match resolve_chat_target(&target) {
            Err(reason) => return (timer.failed(reason.into()), None),
            Ok(Some(platform)) => Some(platform.as_str().to_owned()),
            Ok(None) => {
                let none_enabled = ctx
                    .executor
                    .integration_availability()
                    .is_some_and(|integrations| !integrations.any_chat_platform_enabled());
                if none_enabled {
                    return (
                        timer.failed(MessageRef::new(NO_CHAT_PLATFORM_ENABLED).into()),
                        None,
                    );
                }
                None
            }
        };

        ChatSendRequest {
            message,
            target,
            sender: (sender != SenderPreference::Default).then(|| sender.as_str().to_owned()),
            ..ChatSendRequest::default()
        }
        .publish(ctx.publisher, EventSource::Core, Some(ctx.parent_event_id));

        (timer.success(), None)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use forge_events::EventPublisher;
    use forge_registry::{CancelSignal, ControlCell, TelemetrySink};
    use forge_types::{EventId, SubActionOutcome, unknown_chat_target_message};

    use super::*;
    use crate::integration_gate::{GatedLeafExecutor, IntegrationGate};
    use forge_events::Event;
    use forge_registry::FormField;
    use forge_registry::RegistryError;
    use forge_types::Variant;

    #[derive(Default)]
    struct CapturingPublisher(Mutex<Vec<Event>>);

    impl EventPublisher for CapturingPublisher {
        fn publish(&self, event: Event) {
            self.0.lock().unwrap().push(event);
        }
    }

    impl CapturingPublisher {
        fn sent_requests(&self) -> Vec<serde_json::Value> {
            self.0
                .lock()
                .unwrap()
                .iter()
                .filter(|e| e.kind == "chat.send.request")
                .map(|e| e.payload.clone())
                .collect()
        }
    }

    fn gate_disabling(ids: &[&'static str]) -> IntegrationGate {
        let gate = IntegrationGate::new();
        for id in ids {
            gate.disable(IntegrationId::from_static(id));
        }
        gate
    }

    async fn run_gated(
        target: Option<&str>,
        gate: IntegrationGate,
    ) -> (SubActionOutcome, Vec<serde_json::Value>) {
        let publisher = CapturingPublisher::default();
        let executor = GatedLeafExecutor::new(gate, CancelSignal::new());
        let args = ArgStack::new();
        let ctx = RunContext {
            arg_stack: &args,
            index: 0,
            parent_event_id: EventId::new(),
            publisher: &publisher,
            executor: &executor,
            cancel: CancelSignal::new(),
            control: ControlCell::new(),
            telemetry: TelemetrySink::new(),
        };
        let (telemetry, _) = TwitchChatSendMessageRunner
            .execute(&config_with_target(target), &ctx)
            .await;
        (telemetry.outcome, publisher.sent_requests())
    }

    fn config_with_target(target: Option<&str>) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("message".to_owned(), Variant::String("hi".to_owned()));
        if let Some(target) = target {
            cfg.insert("target".to_owned(), Variant::String(target.to_owned()));
        }
        cfg
    }

    #[tokio::test]
    async fn a_message_that_fills_in_blank_fails_and_sends_nothing() {
        let args = ArgStack::new().set("reply".to_owned(), Variant::String(" \t ".to_owned()));
        for template in ["%reply%", "  %reply%  "] {
            let publisher = CapturingPublisher::default();
            let ctx = RunContext::leaf(&args, 0, EventId::new(), &publisher);
            let mut cfg = SubActionConfig::new();
            cfg.insert("message".to_owned(), Variant::String(template.to_owned()));

            let (telemetry, _) = TwitchChatSendMessageRunner.execute(&cfg, &ctx).await;

            assert!(
                matches!(&telemetry.outcome, SubActionOutcome::Failed(reason) if crate::test_support::failure::is_field_problem(reason, CHAT_SEND_MESSAGE_KEY, "config.required")),
                "{template:?}: {:?}",
                telemetry.outcome
            );
            assert!(publisher.sent_requests().is_empty(), "{template:?}");
        }
    }

    #[test]
    fn the_gated_integration_is_the_resolved_send_target() {
        let args = ArgStack::new().set("platform".to_owned(), Variant::String("kick".to_owned()));
        for (target, expected) in [
            (Some("youtube"), Some("youtube")),
            (Some("%platform%"), Some("kick")),
            (Some("  kick \t"), Some("kick")),
            (Some("Twitch"), Some("twitch")),
            (Some("%platform%"), Some("kick")),
            (Some("myspace"), None),
            (None, Some("twitch")),
            (Some(""), None),
            (Some("   "), None),
        ] {
            let resolved = TwitchChatSendMessageRunner
                .targeted_integration(&config_with_target(target), &args);
            assert_eq!(
                resolved,
                expected.map(IntegrationId::new),
                "target {target:?}"
            );
        }
    }

    #[tokio::test]
    async fn a_blank_target_broadcasts_without_a_target_key_while_any_chat_platform_is_enabled() {
        for (target, disabled) in [
            ("", &[][..]),
            ("  \t", &[][..]),
            ("", &["twitch", "youtube"][..]),
            (" ", &["youtube", "kick", "obs"][..]),
        ] {
            let (outcome, sent) = run_gated(Some(target), gate_disabling(disabled)).await;

            assert_eq!(
                outcome,
                SubActionOutcome::Success,
                "{target:?} {disabled:?}"
            );
            assert_eq!(sent.len(), 1, "{target:?} {disabled:?}");
            assert!(
                sent[0].get("target").is_none(),
                "{target:?} {disabled:?} published {}",
                sent[0]
            );
            assert_eq!(sent[0]["message"].as_str(), Some("hi"));
        }
    }

    #[tokio::test]
    async fn a_blank_target_fails_unsent_when_every_chat_platform_is_disabled() {
        let (outcome, sent) =
            run_gated(Some("  "), gate_disabling(&["twitch", "youtube", "kick"])).await;

        assert_eq!(
            outcome,
            SubActionOutcome::Failed(MessageRef::new(NO_CHAT_PLATFORM_ENABLED).into())
        );
        assert!(sent.is_empty(), "published {sent:?}");
    }

    #[tokio::test]
    async fn a_padded_target_is_published_trimmed() {
        let (outcome, sent) = run_gated(Some("  kick \t"), IntegrationGate::new()).await;

        assert_eq!(outcome, SubActionOutcome::Success);
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0]["target"].as_str(), Some("kick"));
    }

    #[tokio::test]
    async fn an_unknown_target_fails_with_the_valid_targets_and_publishes_nothing() {
        let (outcome, sent) = run_gated(Some("myspace"), IntegrationGate::new()).await;

        assert_eq!(
            outcome,
            SubActionOutcome::Failed(unknown_chat_target_message("myspace").into())
        );
        assert!(sent.is_empty(), "published {sent:?}");
    }

    #[tokio::test]
    async fn a_mixed_case_target_is_published_normalized() {
        let (outcome, sent) = run_gated(Some("Twitch"), IntegrationGate::new()).await;

        assert_eq!(outcome, SubActionOutcome::Success);
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0]["target"].as_str(), Some("twitch"));
    }

    #[tokio::test]
    async fn a_mixed_case_platform_argument_is_published_normalized() {
        let publisher = CapturingPublisher::default();
        let args = ArgStack::new().set("platform".to_owned(), Variant::String("Kick".to_owned()));
        let ctx = RunContext::leaf(&args, 0, EventId::new(), &publisher);

        let (telemetry, _) = TwitchChatSendMessageRunner
            .execute(&config_with_target(Some("%platform%")), &ctx)
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let sent = publisher.sent_requests();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0]["target"].as_str(), Some("kick"));
    }

    #[tokio::test]
    async fn a_mixed_case_target_naming_a_disabled_platform_keeps_the_disabled_outcome() {
        let publisher = CapturingPublisher::default();
        let gate = gate_disabling(&["twitch"]);
        let executor = GatedLeafExecutor::new(gate.clone(), CancelSignal::new());
        let args = ArgStack::new();
        let config = config_with_target(Some("Twitch"));
        let ctx = RunContext {
            arg_stack: &args,
            index: 0,
            parent_event_id: EventId::new(),
            publisher: &publisher,
            executor: &executor,
            cancel: CancelSignal::new(),
            control: ControlCell::new(),
            telemetry: TelemetrySink::new(),
        };
        let owner = TwitchChatSendMessageRunner.targeted_integration(&config, &args);

        let (telemetry, _) = crate::integration_gate::run_gated_step(
            &gate,
            owner.as_ref(),
            &ctx,
            "twitch.chat.send_message",
            TwitchChatSendMessageRunner.execute(&config, &ctx),
        )
        .await;

        assert_eq!(
            telemetry.outcome,
            SubActionOutcome::IntegrationDisabled(IntegrationId::from_static("twitch"))
        );
        assert!(publisher.sent_requests().is_empty());
    }

    #[tokio::test]
    async fn a_blank_target_broadcasts_when_the_executor_knows_no_integration_availability() {
        let publisher = CapturingPublisher::default();
        let args = ArgStack::new();
        let ctx = RunContext::leaf(&args, 0, EventId::new(), &publisher);

        let (telemetry, _) = TwitchChatSendMessageRunner
            .execute(&config_with_target(Some("")), &ctx)
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let sent = publisher.sent_requests();
        assert_eq!(sent.len(), 1);
        assert!(sent[0].get("target").is_none(), "published {}", sent[0]);
    }

    fn config_with_sender(sender: Option<Variant>) -> SubActionConfig {
        let mut cfg = config_with_target(Some("twitch"));
        if let Some(sender) = sender {
            cfg.insert(SENDER_FIELD.to_owned(), sender);
        }
        cfg
    }

    async fn run_with_sender(
        sender: Option<Variant>,
    ) -> (SubActionOutcome, Vec<serde_json::Value>) {
        let publisher = CapturingPublisher::default();
        let args = ArgStack::new();
        let ctx = RunContext::leaf(&args, 0, EventId::new(), &publisher);
        let (telemetry, _) = TwitchChatSendMessageRunner
            .execute(&config_with_sender(sender), &ctx)
            .await;
        (telemetry.outcome, publisher.sent_requests())
    }

    fn invalid_senders() -> [Variant; 4] {
        [
            Variant::String("Bot".to_owned()),
            Variant::String(String::new()),
            Variant::String("bot_only".to_owned()),
            Variant::Int(1),
        ]
    }

    #[tokio::test]
    async fn the_request_carries_the_sender_only_when_it_is_not_default() {
        for (sender, published) in [
            (None, None),
            (Some("default"), None),
            (Some("streamer"), Some("streamer")),
            (Some("bot"), Some("bot")),
        ] {
            let (outcome, sent) =
                run_with_sender(sender.map(|s| Variant::String(s.to_owned()))).await;

            assert_eq!(outcome, SubActionOutcome::Success, "{sender:?}");
            assert_eq!(sent.len(), 1, "{sender:?}");
            assert_eq!(
                sent[0].get(SENDER_FIELD).and_then(|v| v.as_str()),
                published,
                "{sender:?} published {}",
                sent[0]
            );
        }
    }

    #[test]
    fn validation_accepts_every_sender_choice_offered_by_the_form() {
        let offered = TwitchChatSendMessageRunner
            .config_fields()
            .into_iter()
            .find_map(|field| match field {
                FormField::Select { key, options, .. } if key == SENDER_FIELD => Some(options),
                _ => None,
            })
            .unwrap();
        for option in offered {
            let config = config_with_sender(Some(Variant::String((*option).to_owned())));
            assert!(
                TwitchChatSendMessageRunner.validate_config(&config).is_ok(),
                "{option}"
            );
        }
    }

    #[test]
    fn validation_rejects_an_unknown_sender_on_the_sender_field() {
        for sender in invalid_senders() {
            let err = TwitchChatSendMessageRunner
                .validate_config(&config_with_sender(Some(sender.clone())))
                .unwrap_err();

            assert!(
                matches!(&err, RegistryError::InvalidConfig(problem) if problem.field.as_deref() == Some(SENDER_FIELD)),
                "{sender:?} got {err:?}"
            );
        }
    }

    #[tokio::test]
    async fn an_unknown_sender_fails_the_run_and_publishes_nothing() {
        for sender in invalid_senders() {
            let (outcome, sent) = run_with_sender(Some(sender.clone())).await;

            assert!(
                matches!(outcome, SubActionOutcome::Failed(_)),
                "{sender:?} got {outcome:?}"
            );
            assert!(sent.is_empty(), "{sender:?} published {sent:?}");
        }
    }
}

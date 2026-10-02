use async_trait::async_trait;
use forge_events::{Event, EventSource};
use forge_registry::{
    FormField, RegistryError, RunContext, StepTimer, SubActionCategory, SubActionConfigExt,
    SubActionRunner,
};
use forge_types::{
    ArgStack, IntegrationId, NO_CHAT_PLATFORM_ENABLED_REASON, SubActionConfig, SubActionTelemetry,
    Variant, requested_chat_target,
};

const DEFAULT_TARGET: &str = "twitch";

fn resolved_target(config: &SubActionConfig, arg_stack: &ArgStack) -> String {
    arg_stack.interpolate(config.str("target").unwrap_or(DEFAULT_TARGET))
}

pub struct TwitchChatSendMessageRunner;

#[async_trait]
impl SubActionRunner for TwitchChatSendMessageRunner {
    fn id(&self) -> &str {
        "twitch.chat.send_message"
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

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("message".to_owned(), Variant::String(String::new()));
        cfg.insert(
            "target".to_owned(),
            Variant::String(DEFAULT_TARGET.to_owned()),
        );
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::TextArea {
                key: "message",
                label: "Message",
            },
            FormField::Text {
                key: "target",
                label: "Target Platform",
                placeholder: DEFAULT_TARGET,
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        config.require_str("message").map(|_| ())
    }

    fn targeted_integration(
        &self,
        config: &SubActionConfig,
        arg_stack: &ArgStack,
    ) -> Option<IntegrationId> {
        requested_chat_target(&resolved_target(config, arg_stack)).map(IntegrationId::new)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, "twitch.chat.send_message");

        let message = ctx
            .arg_stack
            .interpolate(config.str("message").unwrap_or_default());
        let resolved = resolved_target(config, ctx.arg_stack);
        let payload = match requested_chat_target(&resolved) {
            Some(target) => serde_json::json!({
                "target": target,
                "message": message,
            }),
            None => {
                let none_enabled = ctx
                    .executor
                    .integration_availability()
                    .is_some_and(|integrations| !integrations.any_chat_platform_enabled());
                if none_enabled {
                    return (timer.failed(NO_CHAT_PLATFORM_ENABLED_REASON), None);
                }
                serde_json::json!({ "message": message })
            }
        };

        ctx.publisher.publish(Event::caused_by(
            EventSource::Core,
            "chat.send.request",
            payload,
            ctx.parent_event_id,
        ));

        (timer.success(), None)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::Mutex;

    use forge_events::EventPublisher;
    use forge_registry::{CancelSignal, ControlCell, TelemetrySink};
    use forge_types::{EventId, SubActionOutcome};

    use super::*;
    use crate::integration_gate::{GatedLeafExecutor, IntegrationGate};

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

    #[test]
    fn the_gated_integration_is_the_resolved_send_target() {
        let args = ArgStack::new().set("platform".to_owned(), Variant::String("kick".to_owned()));
        for (target, expected) in [
            (Some("youtube"), Some("youtube")),
            (Some("%platform%"), Some("kick")),
            (Some("  kick \t"), Some("kick")),
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
            SubActionOutcome::Failed(NO_CHAT_PLATFORM_ENABLED_REASON.to_owned())
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
}

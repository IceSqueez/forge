use async_trait::async_trait;
use forge_events::{Event, EventSource};
use forge_registry::{
    FormField, RegistryError, RunContext, StepTimer, SubActionCategory, SubActionConfigExt,
    SubActionRunner,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry, Variant};

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
        cfg.insert("target".to_owned(), Variant::String("twitch".to_owned()));
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
                placeholder: "twitch",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        config.require_str("message").map(|_| ())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, "twitch.chat.send_message");

        let message_template = config.str("message").unwrap_or_default();
        let target_template = config.str("target").unwrap_or("twitch");

        let message = ctx.arg_stack.interpolate(message_template);
        let target = ctx.arg_stack.interpolate(target_template);

        ctx.publisher.publish(Event::caused_by(
            EventSource::Core,
            "chat.send.request",
            serde_json::json!({
                "target": target,
                "message": message,
            }),
            ctx.parent_event_id,
        ));

        (timer.success(), None)
    }
}

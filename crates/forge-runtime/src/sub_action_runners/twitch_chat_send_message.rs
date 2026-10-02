use async_trait::async_trait;
use forge_events::{Event, EventSource};
use forge_registry::{
    FormField, RegistryError, RunContext, StepTimer, SubActionCategory, SubActionConfigExt,
    SubActionRunner,
};
use forge_types::{ArgStack, IntegrationId, SubActionConfig, SubActionTelemetry, Variant};

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
        let target = resolved_target(config, arg_stack);
        let target = target.trim();
        (!target.is_empty()).then(|| IntegrationId::new(target))
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
        let target = resolved_target(config, ctx.arg_stack);

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

#[cfg(test)]
mod tests {
    use super::*;

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
}

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct ModerationBanDescriptor;

impl TriggerKindDescriptor for ModerationBanDescriptor {
    fn id(&self) -> &str {
        "youtube.moderation.ban"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Users
    }

    fn label(&self) -> &str {
        "User banned"
    }

    fn summary(&self) -> &str {
        "Fires when a user receives a permanent ban from YouTube live chat"
    }

    fn search_text(&self) -> &str {
        "youtube moderation ban permanent user chat"
    }

    fn icon_name(&self) -> &str {
        "user-x"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::YouTube)
    }

    fn default_config(&self) -> TriggerConfig {
        TriggerConfig::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![]
    }

    fn condition_display(&self, _config: &TriggerConfig) -> String {
        "any".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::YouTube),
            kind_prefix: Some("youtube.moderation.ban".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let user_display_name = event
            .payload
            .get("user_display_name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new().set(
            "user_display_name".to_owned(),
            Variant::String(user_display_name),
        )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn ban_event() -> Event {
        Event::new(
            EventSource::YouTube,
            "youtube.moderation.ban",
            serde_json::json!({
                "user_display_name": "Spammer"
            }),
        )
    }

    #[test]
    fn always_matches() {
        assert!(ModerationBanDescriptor.matches_trigger(&TriggerConfig::new(), &ban_event()));
    }

    #[test]
    fn build_arg_stack_extracts_ban_fields() {
        let stack = ModerationBanDescriptor.build_arg_stack(&ban_event());
        assert_eq!(
            stack.get("user_display_name"),
            Some(&Variant::String("Spammer".to_owned()))
        );
    }
}

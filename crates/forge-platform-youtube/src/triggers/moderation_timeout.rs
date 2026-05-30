use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct ModerationTimeoutDescriptor;

impl TriggerKindDescriptor for ModerationTimeoutDescriptor {
    fn id(&self) -> &str {
        "youtube.moderation.timeout"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Users
    }

    fn label(&self) -> &str {
        "User timed out"
    }

    fn summary(&self) -> &str {
        "Fires when a user receives a temporary ban in YouTube live chat"
    }

    fn search_text(&self) -> &str {
        "youtube moderation timeout ban temporary user chat"
    }

    fn icon_name(&self) -> &str {
        "clock-off"
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
            kind_prefix: Some("youtube.moderation.timeout".to_owned()),
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
        let ban_duration_seconds = event
            .payload
            .get("ban_duration_seconds")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        ArgStack::new()
            .set(
                "user_display_name".to_owned(),
                Variant::String(user_display_name),
            )
            .set(
                "ban_duration_seconds".to_owned(),
                Variant::Int(ban_duration_seconds),
            )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn timeout_event() -> Event {
        Event::new(
            EventSource::YouTube,
            "youtube.moderation.timeout",
            serde_json::json!({
                "user_display_name": "BadActor",
                "ban_duration_seconds": 300
            }),
        )
    }

    #[test]
    fn kind_id_matches_canonical() {
        assert_eq!(
            ModerationTimeoutDescriptor.id(),
            "youtube.moderation.timeout"
        );
    }

    #[test]
    fn is_platform_specific_youtube() {
        assert_eq!(
            ModerationTimeoutDescriptor.event_filter().source,
            Some(EventSource::YouTube)
        );
    }

    #[test]
    fn always_matches() {
        assert!(
            ModerationTimeoutDescriptor.matches_trigger(&TriggerConfig::new(), &timeout_event())
        );
    }

    #[test]
    fn build_arg_stack_extracts_timeout_fields() {
        let stack = ModerationTimeoutDescriptor.build_arg_stack(&timeout_event());
        assert_eq!(
            stack.get("user_display_name"),
            Some(&Variant::String("BadActor".to_owned()))
        );
        assert_eq!(stack.get("ban_duration_seconds"), Some(&Variant::Int(300)));
    }
}

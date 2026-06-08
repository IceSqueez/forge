use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig};

use super::chat::build_standard_arg_stack;

pub(crate) struct GiftSubDescriptor;

impl TriggerKindDescriptor for GiftSubDescriptor {
    fn id(&self) -> &str {
        "trovo.gift_sub"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Subscriptions
    }

    fn label(&self) -> &str {
        "Gift subscription"
    }

    fn summary(&self) -> &str {
        "Fires when a viewer gifts a subscription in Trovo"
    }

    fn search_text(&self) -> &str {
        "trovo gift sub gifted subscription supporter"
    }

    fn icon_name(&self) -> &str {
        "gift"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::Trovo)
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
            source: Some(EventSource::Trovo),
            kind_prefix: Some("trovo.gift_sub".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        build_standard_arg_stack(event)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_events::EventSource;
    use forge_types::Variant;

    fn gift_event() -> Event {
        Event::new(
            EventSource::Trovo,
            "trovo.gift_sub",
            serde_json::json!({
                "content": "1",
                "nick_name": "Gifter",
                "user_name": "gifter_login",
                "sender_id": "uid_gifter"
            }),
        )
    }

    #[test]
    fn build_arg_stack_extracts_gift_count_from_content() {
        let stack = GiftSubDescriptor.build_arg_stack(&gift_event());
        assert_eq!(stack.get("content"), Some(&Variant::String("1".to_owned())));
        assert_eq!(
            stack.get("nick_name"),
            Some(&Variant::String("Gifter".to_owned()))
        );
    }
}

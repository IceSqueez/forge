use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig};

use super::chat::build_standard_arg_stack;

pub(crate) struct SubscriptionDescriptor;

impl TriggerKindDescriptor for SubscriptionDescriptor {
    fn id(&self) -> &str {
        "trovo.subscription"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Subscriptions
    }

    fn label(&self) -> &str {
        "Subscription"
    }

    fn summary(&self) -> &str {
        "Fires when a viewer subscribes to the Trovo channel"
    }

    fn search_text(&self) -> &str {
        "trovo subscription new sub tier supporter"
    }

    fn icon_name(&self) -> &str {
        "star"
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
            kind_prefix: Some("trovo.subscription".to_owned()),
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

    fn sub_event() -> Event {
        Event::new(
            EventSource::Trovo,
            "trovo.subscription",
            serde_json::json!({
                "content": "Tier 1",
                "nick_name": "NewSub",
                "user_name": "newsub_login",
                "sender_id": "uid_sub"
            }),
        )
    }

    #[test]
    fn build_arg_stack_extracts_tier_from_content() {
        let stack = SubscriptionDescriptor.build_arg_stack(&sub_event());
        assert_eq!(
            stack.get("content"),
            Some(&Variant::String("Tier 1".to_owned()))
        );
    }
}

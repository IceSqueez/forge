use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct SubDescriptor;

impl TriggerKindDescriptor for SubDescriptor {
    fn id(&self) -> &str {
        "kick.sub"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Subscriptions
    }

    fn label(&self) -> &str {
        "Subscription"
    }

    fn summary(&self) -> &str {
        "Fires when a viewer subscribes to the Kick channel"
    }

    fn search_text(&self) -> &str {
        "kick subscription new sub supporter tier"
    }

    fn icon_name(&self) -> &str {
        "star"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::Kick)
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
            source: Some(EventSource::Kick),
            kind_prefix: Some("kick.sub".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let user_id = event
            .payload
            .get("user_ids")
            .and_then(|v| v.as_array())
            .and_then(|a| a.first())
            .and_then(|v| v.as_u64())
            .map_or_else(String::new, |n| n.to_string());

        let username = event
            .payload
            .get("username")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        let months = event
            .payload
            .get("months")
            .and_then(|v| v.as_u64())
            .map_or_else(String::new, |n| n.to_string());

        let tier = event
            .payload
            .get("subscription")
            .and_then(|s| s.get("slug"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("user_id".to_owned(), Variant::String(user_id))
            .set("username".to_owned(), Variant::String(username))
            .set("months".to_owned(), Variant::String(months))
            .set("tier".to_owned(), Variant::String(tier))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn sub_event() -> Event {
        Event::new(
            EventSource::Kick,
            "kick.sub",
            serde_json::json!({
                "user_ids": [123],
                "username": "new_subscriber",
                "months": 3,
                "subscription": { "slug": "tier1" }
            }),
        )
    }

    #[test]
    fn kind_id_is_canonical() {
        assert_eq!(SubDescriptor.id(), "kick.sub");
    }

    #[test]
    fn category_is_subscriptions() {
        assert_eq!(SubDescriptor.category(), TriggerCategory::Subscriptions);
    }

    #[test]
    fn platform_contract_is_kick() {
        assert_eq!(
            SubDescriptor.platform_contract(),
            KindPlatformContract::PlatformSpecific(PlatformId::Kick)
        );
    }

    #[test]
    fn build_arg_stack_extracts_sub_fields() {
        let stack = SubDescriptor.build_arg_stack(&sub_event());
        assert_eq!(
            stack.get("user_id"),
            Some(&Variant::String("123".to_owned()))
        );
        assert_eq!(
            stack.get("username"),
            Some(&Variant::String("new_subscriber".to_owned()))
        );
        assert_eq!(stack.get("months"), Some(&Variant::String("3".to_owned())));
        assert_eq!(
            stack.get("tier"),
            Some(&Variant::String("tier1".to_owned()))
        );
    }
}

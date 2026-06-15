use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct VipRemovedDescriptor;

impl TriggerKindDescriptor for VipRemovedDescriptor {
    fn id(&self) -> &str {
        "twitch.channel.vip_removed"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Moderation
    }

    fn label(&self) -> &str {
        "VIP removed"
    }

    fn summary(&self) -> &str {
        "Fires when VIP status is revoked from a viewer"
    }

    fn search_text(&self) -> &str {
        "twitch vip removed revoked unvip diamond moderation"
    }

    fn icon_name(&self) -> &str {
        "diamond-off"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::Twitch)
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
            source: Some(EventSource::Twitch),
            kind_prefix: Some("channel.vip.remove".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let user = event.payload.get("user");

        let user_login = user
            .and_then(|u| u.get("login"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let user_id = user
            .and_then(|u| u.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let user_name = user
            .and_then(|u| u.get("display_name"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("user_login".to_owned(), Variant::String(user_login))
            .set("user_id".to_owned(), Variant::String(user_id))
            .set("user_name".to_owned(), Variant::String(user_name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_filter_targets_vip_remove_topic_from_twitch() {
        let filter = VipRemovedDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(filter.kind_prefix.as_deref(), Some("channel.vip.remove"));
    }

    #[test]
    fn build_arg_stack_maps_user_fields_from_nested_payload() {
        let payload = serde_json::json!({
            "user": { "id": "555", "login": "former_vip", "display_name": "FormerVip" },
        });
        let event = Event::new(EventSource::Twitch, "channel.vip.remove", payload);
        let stack = VipRemovedDescriptor.build_arg_stack(&event);
        assert_eq!(
            stack.get("user_id"),
            Some(&Variant::String("555".to_owned()))
        );
        assert_eq!(
            stack.get("user_login"),
            Some(&Variant::String("former_vip".to_owned()))
        );
        assert_eq!(
            stack.get("user_name"),
            Some(&Variant::String("FormerVip".to_owned()))
        );
    }

    #[test]
    fn build_arg_stack_yields_empty_strings_when_user_object_absent() {
        let event = Event::new(
            EventSource::Twitch,
            "channel.vip.remove",
            serde_json::json!({}),
        );
        let stack = VipRemovedDescriptor.build_arg_stack(&event);
        for key in ["user_id", "user_login", "user_name"] {
            assert_eq!(
                stack.get(key),
                Some(&Variant::String(String::new())),
                "{key}"
            );
        }
    }
}

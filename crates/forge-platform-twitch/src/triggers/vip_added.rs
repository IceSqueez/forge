use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{ActorRole, PlatformId, TriggerConfig};

use super::payload_read::{self, twitch_actor};
use crate::payload_fields::vip as vip_fields;

pub(crate) struct VipAddedDescriptor;

impl TriggerKindDescriptor for VipAddedDescriptor {
    fn id(&self) -> &str {
        "twitch.channel.vip_added"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Moderation
    }

    fn label(&self) -> &str {
        "VIP added"
    }

    fn summary(&self) -> &str {
        "Fires when a viewer is granted VIP status in the channel"
    }

    fn search_text(&self) -> &str {
        "twitch vip added granted diamond moderation"
    }

    fn icon_name(&self) -> &str {
        "diamond"
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
            kind_prefix: Some("twitch.channel.vip.add".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(twitch_actor(ActorRole::Principal), affected_user_identity),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::principal()
    }
}

fn affected_user_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(
        event.payload.get(vip_fields::USER),
        vip_fields::USER_ID,
        vip_fields::USER_LOGIN,
        vip_fields::USER_DISPLAY_NAME,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_types::Variant;

    #[test]
    fn event_filter_targets_vip_add_topic_from_twitch() {
        let filter = VipAddedDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("twitch.channel.vip.add")
        );
    }

    #[test]
    fn build_arg_stack_maps_user_fields_from_nested_payload() {
        let payload = serde_json::json!({
            "user": { "id": "555", "login": "new_vip", "display_name": "NewVip" },
        });
        let event = Event::new(EventSource::Twitch, "twitch.channel.vip.add", payload);
        let stack = VipAddedDescriptor.build_arg_stack(&event);
        assert_eq!(
            stack.get("user_id"),
            Some(&Variant::String("555".to_owned()))
        );
        assert_eq!(
            stack.get("user_login"),
            Some(&Variant::String("new_vip".to_owned()))
        );
        assert_eq!(
            stack.get("user_name"),
            Some(&Variant::String("NewVip".to_owned()))
        );
    }

    #[test]
    fn build_arg_stack_yields_empty_strings_when_user_object_absent() {
        let event = Event::new(
            EventSource::Twitch,
            "channel.vip.add",
            serde_json::json!({}),
        );
        let stack = VipAddedDescriptor.build_arg_stack(&event);
        for key in ["user_id", "user_login", "user_name"] {
            assert_eq!(
                stack.get(key),
                Some(&Variant::String(String::new())),
                "{key}"
            );
        }
    }
}

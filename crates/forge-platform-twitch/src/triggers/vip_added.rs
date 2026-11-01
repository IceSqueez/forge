use super::spec::TwitchTriggerSpec;
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_events::Event;
use forge_registry::{
    ActorDeclaration, ActorIdentity, ActorRole, TriggerCategory, TriggerVariables,
};

use super::payload_read::{self, twitch_actor};
use crate::vocabulary::fields::vip as vip_fields;

fn variables() -> Option<TriggerVariables> {
    Some(TriggerVariables::new().actor(twitch_actor(ActorRole::Principal), affected_user_identity))
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::CHANNEL_VIP_ADDED,
    category: TriggerCategory::Moderation,
    label: "VIP added",
    summary: "Fires when a viewer is granted VIP status in the channel",
    search_text: "twitch vip added granted diamond moderation",
    icon: "diamond",
    kind: event_kinds::CHANNEL_VIP_ADD,
    actors: ActorDeclaration::principal(),
    variables,
    ..TwitchTriggerSpec::BASE
};

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
    use forge_events::EventSource;
    use forge_registry::TriggerKindDescriptor;
    use forge_types::Variant;

    #[test]
    fn event_filter_targets_vip_add_topic_from_twitch() {
        let filter = SPEC.event_filter();
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
        let stack = SPEC.build_arg_stack(&event);
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
        let stack = SPEC.build_arg_stack(&event);
        for key in ["user_id", "user_login", "user_name"] {
            assert_eq!(
                stack.get(key),
                Some(&Variant::String(String::new())),
                "{key}"
            );
        }
    }
}

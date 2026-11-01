use super::spec::TwitchTriggerSpec;
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_events::Event;
use forge_registry::{
    ActorDeclaration, ActorIdentity, ActorRole, TriggerCategory, TriggerVariables,
};

use super::payload_read::{self, twitch_actor};
use crate::vocabulary::fields::moderator as moderator_fields;

fn variables() -> Option<TriggerVariables> {
    Some(TriggerVariables::new().actor(twitch_actor(ActorRole::Principal), affected_user_identity))
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::CHANNEL_MODERATOR_ADDED,
    category: TriggerCategory::Moderation,
    label: "Moderator added",
    summary: "Fires when a viewer is granted moderator status in the channel",
    search_text: "twitch moderator mod added promoted moderation",
    icon: "shield-check",
    kind: event_kinds::CHANNEL_MODERATOR_ADD,
    actors: ActorDeclaration::principal(),
    variables,
    ..TwitchTriggerSpec::BASE
};

fn affected_user_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(
        event.payload.get(moderator_fields::USER),
        moderator_fields::USER_ID,
        moderator_fields::USER_LOGIN,
        moderator_fields::USER_DISPLAY_NAME,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::EventSource;
    use forge_registry::TriggerKindDescriptor;
    use forge_types::Variant;

    fn moderator_add_event() -> Event {
        let payload = serde_json::json!({
            "user": { "id": "777", "login": "viewer_one", "display_name": "ViewerOne" },
        });
        Event::new(EventSource::Twitch, "twitch.channel.moderator.add", payload)
    }

    #[test]
    fn build_arg_stack_maps_user_fields_from_nested_payload() {
        let stack = SPEC.build_arg_stack(&moderator_add_event());
        assert_eq!(
            stack.get("user_login"),
            Some(&Variant::String("viewer_one".to_owned()))
        );
        assert_eq!(
            stack.get("user_id"),
            Some(&Variant::String("777".to_owned()))
        );
        assert_eq!(
            stack.get("user_name"),
            Some(&Variant::String("ViewerOne".to_owned()))
        );
    }
}

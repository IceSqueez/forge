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
    id: trigger_ids::CHANNEL_MODERATOR_REMOVED,
    category: TriggerCategory::Moderation,
    label: "Moderator removed",
    summary: "Fires when moderator status is revoked from a viewer",
    search_text: "twitch moderator mod removed unmodded demoted moderation",
    icon: "shield-x",
    kind: event_kinds::CHANNEL_MODERATOR_REMOVE,
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

    fn moderator_remove_event() -> Event {
        let payload = serde_json::json!({
            "user": { "id": "888", "login": "ex_mod", "display_name": "ExMod" },
        });
        Event::new(
            EventSource::Twitch,
            "twitch.channel.moderator.remove",
            payload,
        )
    }

    #[test]
    fn build_arg_stack_maps_user_fields_from_nested_payload() {
        let stack = SPEC.build_arg_stack(&moderator_remove_event());
        assert_eq!(
            stack.get("user_login"),
            Some(&Variant::String("ex_mod".to_owned()))
        );
        assert_eq!(
            stack.get("user_id"),
            Some(&Variant::String("888".to_owned()))
        );
        assert_eq!(
            stack.get("user_name"),
            Some(&Variant::String("ExMod".to_owned()))
        );
    }
}

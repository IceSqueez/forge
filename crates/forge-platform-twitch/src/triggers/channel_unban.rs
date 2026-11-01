use super::spec::TwitchTriggerSpec;
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_registry::{ActorDeclaration, ActorRole, TriggerCategory, TriggerVariables};

use super::payload_read::{self, twitch_actor};

fn variables() -> Option<TriggerVariables> {
    Some(
        TriggerVariables::new()
            .actor(
                twitch_actor(ActorRole::Principal),
                payload_read::moderated_user_identity,
            )
            .actor(
                twitch_actor(ActorRole::Moderator),
                payload_read::acting_moderator_identity,
            ),
    )
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::CHANNEL_UNBAN,
    category: TriggerCategory::Moderation,
    label: "User unbanned",
    summary: "Fires when a viewer's ban or timeout is lifted",
    search_text: "twitch unban unbanned moderation pardon",
    icon: "check",
    kind: event_kinds::CHANNEL_UNBAN,
    actors: ActorDeclaration::Actors(&[ActorRole::Moderator]),
    variables,
    ..TwitchTriggerSpec::BASE
};

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::{Event, EventSource};
    use forge_registry::TriggerKindDescriptor;
    use forge_types::Variant;

    fn unban_event() -> Event {
        let payload = serde_json::json!({
            "user": { "id": "777", "login": "viewer_one", "display_name": "ViewerOne" },
            "moderator": { "login": "mod_jane", "display_name": "ModJane" },
        });
        Event::new(EventSource::Twitch, "twitch.channel.unban", payload)
    }

    #[test]
    fn build_arg_stack_exposes_user_and_moderator_without_ban_specific_vars() {
        let stack = SPEC.build_arg_stack(&unban_event());
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
        assert_eq!(
            stack.get("moderator_login"),
            Some(&Variant::String("mod_jane".to_owned()))
        );
        assert_eq!(stack.get("reason"), None);
        assert_eq!(stack.get("banned_at"), None);
        assert_eq!(stack.get("ends_at"), None);
    }
}

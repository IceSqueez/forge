use super::spec::TwitchTriggerSpec;
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_events::Event;
use forge_registry::{
    ActorDeclaration, ActorIdentity, ActorRole, TriggerCategory, TriggerVariables,
};

use super::payload_read::{self, twitch_actor};
use crate::vocabulary::fields::warning as warning_fields;

fn variables() -> Option<TriggerVariables> {
    Some(TriggerVariables::new().actor(twitch_actor(ActorRole::Principal), affected_user_identity))
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::CHANNEL_WARNING_ACKNOWLEDGED,
    category: TriggerCategory::Moderation,
    label: "Warning acknowledged",
    summary: "Fires when a user acknowledges a warning issued by a moderator",
    search_text: "twitch warning acknowledged user moderation",
    icon: "bell-check",
    kind: event_kinds::CHANNEL_WARNING_ACKNOWLEDGE,
    actors: ActorDeclaration::principal(),
    variables,
    ..TwitchTriggerSpec::BASE
};

fn affected_user_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(
        event.payload.get(warning_fields::USER),
        warning_fields::USER_ID,
        warning_fields::USER_LOGIN,
        warning_fields::USER_DISPLAY_NAME,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::EventSource;
    use forge_registry::TriggerKindDescriptor;
    use forge_types::Variant;

    fn warning_acknowledged_event() -> Event {
        let payload = serde_json::json!({
            "user": { "id": "654", "login": "warned_user", "display_name": "WarnedUser" },
        });
        Event::new(
            EventSource::Twitch,
            "twitch.channel.warning.acknowledge",
            payload,
        )
    }

    #[test]
    fn build_arg_stack_maps_user_fields_from_nested_payload() {
        let stack = SPEC.build_arg_stack(&warning_acknowledged_event());
        assert_eq!(
            stack.get("user_login"),
            Some(&Variant::String("warned_user".to_owned()))
        );
        assert_eq!(
            stack.get("user_id"),
            Some(&Variant::String("654".to_owned()))
        );
        assert_eq!(
            stack.get("user_name"),
            Some(&Variant::String("WarnedUser".to_owned()))
        );
    }
}

use super::families;
use super::spec::TwitchTriggerSpec;
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_registry::{ActorDeclaration, TriggerCategory};

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::CHANNEL_POINTS_REWARD_UPDATED,
    category: TriggerCategory::ChannelPoints,
    label: "Channel point reward updated",
    summary: "Fires when an existing channel point reward is modified on the broadcaster's channel",
    search_text: "twitch channel points reward updated changed modified",
    icon: "star",
    kind: event_kinds::CHANNEL_CHANNEL_POINTS_CUSTOM_REWARD_UPDATE,
    actors: ActorDeclaration::Actorless,
    variables: families::reward_variables,
    ..TwitchTriggerSpec::BASE
};

#[cfg(test)]
mod tests {

    use super::*;
    use forge_events::{Event, EventSource};
    use forge_registry::TriggerKindDescriptor;
    use forge_types::Variant;

    fn reward_event() -> Event {
        Event::new(
            EventSource::Twitch,
            "channel.channel_points_custom_reward.update",
            serde_json::json!({
                "reward": {
                    "id": "reward-7",
                    "title": "Hydrate",
                    "cost": 750,
                    "prompt": "Updated prompt",
                    "is_enabled": false,
                },
            }),
        )
    }

    #[test]
    fn build_arg_stack_marshals_cost_as_int_and_is_enabled_as_bool() {
        let stack = SPEC.build_arg_stack(&reward_event());
        assert_eq!(
            stack.get("reward.id"),
            Some(&Variant::String("reward-7".to_owned()))
        );
        assert_eq!(
            stack.get("reward.title"),
            Some(&Variant::String("Hydrate".to_owned()))
        );
        assert_eq!(stack.get("reward.cost"), Some(&Variant::Int(750)));
        assert_eq!(
            stack.get("reward.prompt"),
            Some(&Variant::String("Updated prompt".to_owned()))
        );
        assert_eq!(stack.get("reward.is_enabled"), Some(&Variant::Bool(false)));
    }
}

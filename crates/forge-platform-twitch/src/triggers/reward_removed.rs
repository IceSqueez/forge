use super::families;
use super::spec::TwitchTriggerSpec;
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_registry::{ActorDeclaration, TriggerCategory};

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::CHANNEL_POINTS_REWARD_REMOVED,
    category: TriggerCategory::ChannelPoints,
    label: "Channel point reward removed",
    summary: "Fires when a channel point reward is deleted from the broadcaster's channel",
    search_text: "twitch channel points reward removed deleted",
    icon: "star",
    kind: event_kinds::CHANNEL_CHANNEL_POINTS_CUSTOM_REWARD_REMOVE,
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
            "channel.channel_points_custom_reward.remove",
            serde_json::json!({
                "reward": {
                    "id": "reward-7",
                    "title": "Hydrate",
                    "cost": 500,
                    "prompt": "Make the streamer drink water",
                    "is_enabled": true,
                },
            }),
        )
    }

    #[test]
    fn event_filter_targets_reward_remove_kind_from_twitch() {
        let filter = SPEC.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("twitch.channel.channel_points_custom_reward.remove")
        );
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
        assert_eq!(stack.get("reward.cost"), Some(&Variant::Int(500)));
        assert_eq!(
            stack.get("reward.prompt"),
            Some(&Variant::String("Make the streamer drink water".to_owned()))
        );
        assert_eq!(stack.get("reward.is_enabled"), Some(&Variant::Bool(true)));
    }
}

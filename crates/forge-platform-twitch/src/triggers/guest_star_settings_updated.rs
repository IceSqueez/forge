use super::spec::{TwitchTriggerSpec, Variable, table_variables};
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_registry::TriggerVariables;
use forge_registry::{ActorDeclaration, TriggerCategory};

use crate::vocabulary::fields::guest_star as guest_star_fields;

const VARIABLES: &[Variable] = &[
    Variable::nested_int(
        "guest_star.slot_count",
        "Slot count",
        guest_star_fields::SETTINGS,
        guest_star_fields::SLOT_COUNT,
    )
    .bounded(0, 6),
    Variable::nested_text(
        "guest_star.group_layout",
        "Group layout",
        guest_star_fields::SETTINGS,
        guest_star_fields::GROUP_LAYOUT,
    ),
    Variable::nested_flag(
        "guest_star.is_moderator_send_live_enabled",
        "Moderator can send live",
        guest_star_fields::SETTINGS,
        guest_star_fields::IS_MODERATOR_SEND_LIVE_ENABLED,
    ),
];

fn variables() -> Option<TriggerVariables> {
    Some(table_variables(VARIABLES))
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::GUEST_STAR_SETTINGS_UPDATED,
    category: TriggerCategory::Streams,
    label: "Guest Star settings updated",
    summary: "Fires when the broadcaster's Guest Star preferences change",
    search_text: "twitch guest star settings preferences layout slots moderator audio",
    icon: "settings",
    kind: event_kinds::CHANNEL_GUEST_STAR_SETTINGS_UPDATE,
    actors: ActorDeclaration::Actorless,
    variables,
    ..TwitchTriggerSpec::BASE
};

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::{Event, EventSource};
    use forge_registry::TriggerKindDescriptor;
    use forge_types::Variant;

    fn settings_event() -> Event {
        Event::new(
            EventSource::Twitch,
            "channel.guest_star_settings.update",
            serde_json::json!({
                "settings": {
                    "slot_count": 5,
                    "group_layout": "TILED_LAYOUT",
                    "is_moderator_send_live_enabled": true,
                },
            }),
        )
    }

    #[test]
    fn event_filter_targets_settings_update_kind_from_twitch() {
        let filter = SPEC.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("twitch.channel.guest_star_settings.update")
        );
    }

    #[test]
    fn build_arg_stack_marshals_slot_count_as_int_and_moderator_flag_as_bool() {
        let stack = SPEC.build_arg_stack(&settings_event());
        assert_eq!(stack.get("guest_star.slot_count"), Some(&Variant::Int(5)));
        assert_eq!(
            stack.get("guest_star.group_layout"),
            Some(&Variant::String("TILED_LAYOUT".to_owned()))
        );
        assert_eq!(
            stack.get("guest_star.is_moderator_send_live_enabled"),
            Some(&Variant::Bool(true))
        );
    }
}

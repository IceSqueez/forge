use super::spec::{TwitchTriggerSpec, Variable, table_variables};
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_registry::TriggerVariables;
use forge_registry::{ActorDeclaration, TriggerCategory};

use crate::vocabulary::fields::chat_mod as chat_mod_fields;

const VARIABLES: &[Variable] = &[
    Variable::nested_flag(
        "settings.emote_mode",
        "Emote-only mode",
        chat_mod_fields::SETTINGS,
        chat_mod_fields::EMOTE_MODE,
    ),
    Variable::nested_flag(
        "settings.follower_mode",
        "Follower-only mode",
        chat_mod_fields::SETTINGS,
        chat_mod_fields::FOLLOWER_MODE,
    ),
    Variable::nested_flag(
        "settings.slow_mode",
        "Slow mode",
        chat_mod_fields::SETTINGS,
        chat_mod_fields::SLOW_MODE,
    ),
    Variable::nested_flag(
        "settings.subscriber_mode",
        "Subscriber-only mode",
        chat_mod_fields::SETTINGS,
        chat_mod_fields::SUBSCRIBER_MODE,
    ),
    Variable::nested_flag(
        "settings.unique_chat_mode",
        "Unique chat mode",
        chat_mod_fields::SETTINGS,
        chat_mod_fields::UNIQUE_CHAT_MODE,
    ),
    Variable::nested_int(
        "settings.slow_mode_wait_time_seconds",
        "Slow mode wait (seconds)",
        chat_mod_fields::SETTINGS,
        chat_mod_fields::SLOW_MODE_WAIT_TIME_SECONDS,
    )
    .bounded(0, 120),
    Variable::nested_int(
        "settings.follower_mode_duration_minutes",
        "Follower mode duration (minutes)",
        chat_mod_fields::SETTINGS,
        chat_mod_fields::FOLLOWER_MODE_DURATION_MINUTES,
    )
    .bounded(0, 10080),
];

fn variables() -> Option<TriggerVariables> {
    Some(table_variables(VARIABLES))
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::CHANNEL_CHAT_SETTINGS_UPDATED,
    category: TriggerCategory::Chat,
    label: "Chat settings updated",
    summary: "Fires when a broadcaster's chat settings change (emote-only, follower-only, slow mode, sub-only, unique-chat)",
    search_text: "twitch chat settings emote follower slow subscriber unique mode updated",
    icon: "settings",
    kind: event_kinds::CHANNEL_CHAT_SETTINGS_UPDATE,
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
            "channel.chat_settings.update",
            serde_json::json!({
                "settings": {
                    "emote_mode": true,
                    "follower_mode": false,
                    "slow_mode": true,
                    "subscriber_mode": false,
                    "unique_chat_mode": true,
                    "slow_mode_wait_time_seconds": 30,
                    "follower_mode_duration_minutes": 10,
                },
            }),
        )
    }

    #[test]
    fn event_filter_targets_chat_settings_update_kind_from_twitch() {
        let filter = SPEC.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("twitch.channel.chat_settings.update")
        );
    }

    #[test]
    fn build_arg_stack_marshals_mode_flags_as_bool_and_durations_as_int() {
        let stack = SPEC.build_arg_stack(&settings_event());
        for (key, expected) in [
            ("settings.emote_mode", true),
            ("settings.follower_mode", false),
            ("settings.slow_mode", true),
            ("settings.subscriber_mode", false),
            ("settings.unique_chat_mode", true),
        ] {
            assert_eq!(
                stack.get(key),
                Some(&Variant::Bool(expected)),
                "bool var: {key}"
            );
        }
        assert_eq!(
            stack.get("settings.slow_mode_wait_time_seconds"),
            Some(&Variant::Int(30))
        );
        assert_eq!(
            stack.get("settings.follower_mode_duration_minutes"),
            Some(&Variant::Int(10))
        );
    }
}

use super::spec::{TwitchTriggerSpec, Variable, table_variables};
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_registry::TriggerVariables;
use forge_registry::{ActorDeclaration, TriggerCategory};

use crate::vocabulary::fields::channel_update as fields;

const VARIABLES: &[Variable] = &[
    Variable::nested_text("channel.title", fields::CHANNEL, fields::TITLE),
    Variable::nested_text("channel.category_id", fields::CHANNEL, fields::CATEGORY_ID),
    Variable::nested_text(
        "channel.category_name",
        fields::CHANNEL,
        fields::CATEGORY_NAME,
    ),
    Variable::nested_text("channel.language", fields::CHANNEL, fields::LANGUAGE),
];

fn variables() -> Option<TriggerVariables> {
    Some(table_variables(VARIABLES))
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::CHANNEL_UPDATE,
    category: TriggerCategory::Streams,
    label: "Channel updated",
    summary: "Fires when the broadcaster updates their channel title, category, or language",
    search_text: "twitch channel update title category game language",
    icon: "broadcast",
    kind: event_kinds::CHANNEL_UPDATE,
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

    fn channel_update_event() -> Event {
        let payload = serde_json::json!({
            "channel": {
                "title": "New title",
                "language": "en",
                "category_id": "509658",
                "category_name": "Just Chatting",
            },
        });
        Event::new(EventSource::Twitch, "twitch.channel.update", payload)
    }

    #[test]
    fn event_filter_targets_channel_update_topic_from_twitch() {
        let filter = SPEC.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(filter.kind_prefix.as_deref(), Some("twitch.channel.update"));
    }

    #[test]
    fn build_arg_stack_maps_channel_fields_from_nested_payload() {
        let stack = SPEC.build_arg_stack(&channel_update_event());
        assert_eq!(
            stack.get("channel.title"),
            Some(&Variant::String("New title".to_owned()))
        );
        assert_eq!(
            stack.get("channel.category_id"),
            Some(&Variant::String("509658".to_owned()))
        );
        assert_eq!(
            stack.get("channel.category_name"),
            Some(&Variant::String("Just Chatting".to_owned()))
        );
        assert_eq!(
            stack.get("channel.language"),
            Some(&Variant::String("en".to_owned()))
        );
    }

    #[test]
    fn build_arg_stack_defaults_missing_fields_to_empty_strings() {
        let event = Event::new(
            EventSource::Twitch,
            "twitch.channel.update",
            serde_json::json!({}),
        );
        let stack = SPEC.build_arg_stack(&event);
        assert_eq!(
            stack.get("channel.title"),
            Some(&Variant::String(String::new()))
        );
        assert_eq!(
            stack.get("channel.language"),
            Some(&Variant::String(String::new()))
        );
    }
}

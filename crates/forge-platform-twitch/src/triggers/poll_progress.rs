use super::spec::{TwitchTriggerSpec, Variable, table_variables};
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_registry::TriggerVariables;
use forge_registry::{ActorDeclaration, TriggerCategory};
use forge_types::VariantKind;

use super::families::build_choices_variant;
use crate::vocabulary::fields::poll as fields;

const VARIABLES: &[Variable] = &[
    Variable::nested_text("poll.id", fields::POLL, fields::POLL_ID),
    Variable::nested_text("poll.title", fields::POLL, fields::POLL_TITLE),
    Variable::custom("poll.choices", VariantKind::Array, build_choices_variant),
];

fn variables() -> Option<TriggerVariables> {
    Some(table_variables(VARIABLES))
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::POLL_PROGRESS,
    category: TriggerCategory::Polls,
    label: "Poll progress",
    summary: "Fires when vote counts update on an active poll",
    search_text: "twitch poll progress update vote",
    icon: "chart-bar",
    kind: event_kinds::CHANNEL_POLL_PROGRESS,
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

    fn poll_progress_event() -> Event {
        let payload = serde_json::json!({
            "poll": {
                "id": "poll-2",
                "title": "Next game?",
            },
        });
        Event::new(EventSource::Twitch, "twitch.channel.poll.progress", payload)
    }

    #[test]
    fn build_arg_stack_maps_poll_id_and_title() {
        let stack = SPEC.build_arg_stack(&poll_progress_event());
        assert_eq!(
            stack.get("poll.id"),
            Some(&Variant::String("poll-2".to_owned()))
        );
        assert_eq!(
            stack.get("poll.title"),
            Some(&Variant::String("Next game?".to_owned()))
        );
    }
}

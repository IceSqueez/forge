use super::spec::{TwitchTriggerSpec, Variable, table_variables};
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_registry::TriggerVariables;
use forge_registry::{ActorDeclaration, TriggerCategory};

use crate::vocabulary::fields::guest_star as guest_star_fields;

const VARIABLES: &[Variable] = &[
    Variable::nested_text(
        "guest_star.session_id",
        guest_star_fields::SESSION,
        guest_star_fields::SESSION_ID,
    ),
    Variable::nested_timestamp(
        "guest_star.ended_at",
        guest_star_fields::SESSION,
        guest_star_fields::SESSION_ENDED_AT,
    ),
];

fn variables() -> Option<TriggerVariables> {
    Some(table_variables(VARIABLES))
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::GUEST_STAR_SESSION_ENDED,
    category: TriggerCategory::Streams,
    label: "Guest Star session ended",
    summary: "Fires when a running Guest Star session ends",
    search_text: "twitch guest star session ended stopped host",
    icon: "star",
    kind: event_kinds::CHANNEL_GUEST_STAR_SESSION_END,
    actors: ActorDeclaration::Actorless,
    variables,
    ..TwitchTriggerSpec::BASE
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timestamp::tests::instant;
    use forge_events::{Event, EventSource};
    use forge_registry::TriggerKindDescriptor;
    use forge_types::Variant;

    fn end_event() -> Event {
        Event::new(
            EventSource::Twitch,
            "channel.guest_star_session.end",
            serde_json::json!({
                "session": { "id": "sess-42", "ended_at": "2026-06-13T21:00:00Z" },
            }),
        )
    }

    #[test]
    fn event_filter_targets_session_end_kind_from_twitch() {
        let filter = SPEC.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("twitch.channel.guest_star_session.end")
        );
    }

    #[test]
    fn build_arg_stack_exposes_session_id_chaining_var_and_ended_at() {
        let stack = SPEC.build_arg_stack(&end_event());
        assert_eq!(
            stack.get("guest_star.session_id"),
            Some(&Variant::String("sess-42".to_owned()))
        );
        assert_eq!(
            stack.get("guest_star.ended_at"),
            Some(&instant("2026-06-13T21:00:00Z"))
        );
    }
}

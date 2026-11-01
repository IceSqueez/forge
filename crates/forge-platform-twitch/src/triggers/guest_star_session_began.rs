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
        "guest_star.started_at",
        guest_star_fields::SESSION,
        guest_star_fields::SESSION_STARTED_AT,
    ),
];

fn variables() -> Option<TriggerVariables> {
    Some(table_variables(VARIABLES))
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::GUEST_STAR_SESSION_BEGAN,
    category: TriggerCategory::Streams,
    label: "Guest Star session began",
    summary: "Fires when the broadcaster starts a new Guest Star session",
    search_text: "twitch guest star session began started host",
    icon: "star",
    kind: event_kinds::CHANNEL_GUEST_STAR_SESSION_BEGIN,
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

    fn begin_event() -> Event {
        Event::new(
            EventSource::Twitch,
            "channel.guest_star_session.begin",
            serde_json::json!({
                "session": { "id": "sess-42", "started_at": "2026-06-13T20:00:00Z" },
            }),
        )
    }

    #[test]
    fn build_arg_stack_exposes_session_id_chaining_var_and_started_at() {
        let stack = SPEC.build_arg_stack(&begin_event());
        assert_eq!(
            stack.get("guest_star.session_id"),
            Some(&Variant::String("sess-42".to_owned()))
        );
        assert_eq!(
            stack.get("guest_star.started_at"),
            Some(&instant("2026-06-13T20:00:00Z"))
        );
    }

    #[test]
    fn build_arg_stack_defaults_session_id_to_empty_when_session_object_absent() {
        let event = Event::new(
            EventSource::Twitch,
            "channel.guest_star_session.begin",
            serde_json::json!({}),
        );
        let stack = SPEC.build_arg_stack(&event);
        assert_eq!(
            stack.get("guest_star.session_id"),
            Some(&Variant::String(String::new()))
        );
    }
}

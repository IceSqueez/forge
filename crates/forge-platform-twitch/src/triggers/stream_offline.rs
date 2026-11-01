use super::spec::{TwitchTriggerSpec, Variable, table_variables};
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_registry::TriggerVariables;
use forge_registry::{ActorDeclaration, TriggerCategory};
use forge_types::SynthesisHint;

use crate::vocabulary::fields::stream as stream_fields;

const VARIABLES: &[Variable] = &[
    Variable::nested_text(
        "broadcaster_login",
        stream_fields::BROADCASTER,
        stream_fields::BROADCASTER_LOGIN,
    )
    .synthesis(SynthesisHint::Username),
    Variable::nested_text(
        "broadcaster_id",
        stream_fields::BROADCASTER,
        stream_fields::BROADCASTER_ID,
    ),
];

fn variables() -> Option<TriggerVariables> {
    Some(table_variables(VARIABLES))
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::STREAM_OFFLINE,
    category: TriggerCategory::Streams,
    label: "Stream offline",
    summary: "Fires when your stream ends",
    search_text: "twitch stream offline ended stopped broadcast",
    icon: "broadcast",
    kind: event_kinds::STREAM_OFFLINE,
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

    fn offline_event() -> Event {
        Event::new(
            EventSource::Twitch,
            "stream.offline",
            serde_json::json!({
                "broadcaster": { "id": "100", "login": "host_chan" }
            }),
        )
    }

    #[test]
    fn build_arg_stack_maps_broadcaster_from_nested_payload() {
        let stack = SPEC.build_arg_stack(&offline_event());
        assert_eq!(
            stack.get("broadcaster_login"),
            Some(&Variant::String("host_chan".to_owned()))
        );
        assert_eq!(
            stack.get("broadcaster_id"),
            Some(&Variant::String("100".to_owned()))
        );
    }
}

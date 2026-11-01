use forge_events::EventSource;
use forge_registry::TriggerVariables;
use forge_registry::{
    EventTriggerSpec, EventVariable, TriggerCondition, TriggerMatcher, event_variables,
    no_config_fields,
};

use crate::vocabulary::fields::connection as fields;
use crate::vocabulary::messages;
use crate::vocabulary::{event_kinds, trigger_ids, variables};

const VARIABLES: &[EventVariable] = &[
    EventVariable::text(
        variables::connection::REASON,
        "Disconnect reason",
        fields::REASON,
    ),
    EventVariable::text(
        variables::connection::DETAIL,
        "Disconnect detail",
        fields::DETAIL,
    ),
];

fn variables() -> Option<TriggerVariables> {
    event_variables(VARIABLES)
}

pub(crate) const CONNECTION_DISCONNECTED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::CONNECTION_DISCONNECTED,
    source: EventSource::Obs,
    label: "OBS disconnected",
    summary: "Fires when the OBS WebSocket connection is lost.",
    search_text: "obs connection disconnected lost dropped offline",
    icon: "plug-x",
    prefix: event_kinds::CONNECTION_PREFIX,
    matcher: TriggerMatcher::Kind(event_kinds::CONNECTION_DISCONNECTED),
    condition: TriggerCondition::Fixed(messages::CONDITION_DISCONNECTED),
    config_fields: no_config_fields,
    variables,
};

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::{Event, EventSource};
    use forge_registry::TriggerKindDescriptor;
    use forge_types::Variant;
    use serde_json::json;
    use std::collections::BTreeMap;

    #[test]
    fn matches_only_disconnected_among_connection_lifecycle() {
        let d = CONNECTION_DISCONNECTED;
        for (kind, expected) in [
            ("obs.connection.disconnected", true),
            ("obs.connection.connected", false),
            ("obs.connection.auth_failed", false),
        ] {
            let event = Event::new(EventSource::Obs, kind, json!({}));
            assert_eq!(
                d.matches_trigger(&BTreeMap::new(), &event),
                expected,
                "kind {kind}"
            );
        }
    }

    #[test]
    fn build_arg_stack_extracts_reason() {
        let d = CONNECTION_DISCONNECTED;
        let event = Event::new(
            EventSource::Obs,
            "obs.connection.disconnected",
            json!({ "reason": "connection reset by peer" }),
        );
        assert_eq!(
            d.build_arg_stack(&event).get("obs.reason"),
            Some(&Variant::String("connection reset by peer".to_owned()))
        );
    }

    #[test]
    fn build_arg_stack_omits_reason_when_missing() {
        let d = CONNECTION_DISCONNECTED;
        let event = Event::new(EventSource::Obs, "obs.connection.disconnected", json!({}));
        assert_eq!(d.build_arg_stack(&event).get("obs.reason"), None);
    }
}

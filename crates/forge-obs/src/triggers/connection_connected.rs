use forge_events::EventSource;
use forge_registry::TriggerVariables;
use forge_registry::{
    EventTriggerSpec, EventVariable, TriggerCondition, TriggerMatcher, event_variables,
    no_config_fields,
};

use crate::vocabulary::{event_kinds, messages, trigger_ids};

const VARIABLES: &[EventVariable] = &[];

fn variables() -> Option<TriggerVariables> {
    event_variables(VARIABLES)
}

pub(crate) const CONNECTION_CONNECTED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::CONNECTION_CONNECTED,
    source: EventSource::Obs,
    label: "OBS connected",
    summary: "Fires when the OBS WebSocket connection is successfully established.",
    search_text: "obs connection connected established online",
    icon: "plug-connected",
    prefix: event_kinds::CONNECTION_PREFIX,
    matcher: TriggerMatcher::Kind(event_kinds::CONNECTION_CONNECTED),
    condition: TriggerCondition::Fixed(messages::CONDITION_CONNECTED),
    config_fields: no_config_fields,
    variables,
};

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::{Event, EventSource};
    use forge_registry::TriggerKindDescriptor;
    use serde_json::json;
    use std::collections::BTreeMap;

    #[test]
    fn matches_only_connected_among_connection_lifecycle() {
        let d = CONNECTION_CONNECTED;
        for (kind, expected) in [
            ("obs.connection.connected", true),
            ("obs.connection.disconnected", false),
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
    fn does_not_match_foreign_kind() {
        let d = CONNECTION_CONNECTED;
        let event = Event::new(EventSource::Obs, "obs.scene.changed", json!({}));
        assert!(!d.matches_trigger(&BTreeMap::new(), &event));
    }
}

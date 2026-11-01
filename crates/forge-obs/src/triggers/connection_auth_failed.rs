use forge_events::EventSource;
use forge_registry::TriggerVariables;
use forge_registry::{
    EventTriggerSpec, EventVariable, TriggerCondition, TriggerMatcher, event_variables,
    no_config_fields,
};

use crate::vocabulary::fields::connection as fields;
use crate::vocabulary::messages;
use crate::vocabulary::{event_kinds, trigger_ids, variables};

const VARIABLES: &[EventVariable] = &[EventVariable::text(
    variables::connection::ERROR_MESSAGE,
    fields::ERROR_MESSAGE,
)];

fn variables() -> Option<TriggerVariables> {
    event_variables(VARIABLES)
}

pub(crate) const CONNECTION_AUTH_FAILED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::CONNECTION_AUTH_FAILED,
    source: EventSource::Obs,
    label: "OBS authentication failed",
    summary: "Fires when the OBS WebSocket connection is rejected due to authentication failure.",
    search_text: "obs connection auth failed password wrong rejected",
    icon: "shield-x",
    prefix: event_kinds::CONNECTION_PREFIX,
    matcher: TriggerMatcher::Kind(event_kinds::CONNECTION_AUTH_FAILED),
    condition: TriggerCondition::Fixed(messages::CONDITION_AUTHENTICATION_FAILED),
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
    fn matches_only_auth_failed_among_connection_lifecycle() {
        let d = CONNECTION_AUTH_FAILED;
        for (kind, expected) in [
            ("obs.connection.auth_failed", true),
            ("obs.connection.connected", false),
            ("obs.connection.disconnected", false),
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
    fn build_arg_stack_extracts_error_message() {
        let d = CONNECTION_AUTH_FAILED;
        let event = Event::new(
            EventSource::Obs,
            "obs.connection.auth_failed",
            json!({ "error_message": "authentication rejected" }),
        );
        assert_eq!(
            d.build_arg_stack(&event).get("obs.error_message"),
            Some(&Variant::String("authentication rejected".to_owned()))
        );
    }

    #[test]
    fn build_arg_stack_omits_error_message_when_missing() {
        let d = CONNECTION_AUTH_FAILED;
        let event = Event::new(EventSource::Obs, "obs.connection.auth_failed", json!({}));
        assert_eq!(d.build_arg_stack(&event).get("obs.error_message"), None);
    }
}

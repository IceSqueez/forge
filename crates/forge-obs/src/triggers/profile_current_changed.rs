use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, TriggerConfig, Variant};

pub struct ProfileCurrentChangedDescriptor;

impl TriggerKindDescriptor for ProfileCurrentChangedDescriptor {
    fn id(&self) -> &str {
        "obs.profile.current_changed"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS profile changed"
    }

    fn summary(&self) -> &str {
        "Fires when OBS finishes switching to a different profile."
    }

    fn search_text(&self) -> &str {
        "obs profile changed switched current"
    }

    fn icon_name(&self) -> &str {
        "settings"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::Universal
    }

    fn default_config(&self) -> TriggerConfig {
        BTreeMap::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![]
    }

    fn condition_display(&self, _config: &TriggerConfig) -> String {
        "any profile".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some("profile.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "profile.current_changed"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(name) = event.payload.get("profile_name").and_then(|v| v.as_str()) {
            stack = stack.set(
                "obs.profile.name".to_owned(),
                Variant::String(name.to_owned()),
            );
        }
        stack
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_registry::TriggerKindDescriptor;
    use serde_json::json;

    #[test]
    fn matches_current_changed_and_rejects_list_changed_under_profile_prefix() {
        let d = ProfileCurrentChangedDescriptor;
        let cfg = BTreeMap::new();
        assert!(d.matches_trigger(
            &cfg,
            &Event::new(EventSource::Obs, "profile.current_changed", json!({})),
        ));
        assert!(!d.matches_trigger(
            &cfg,
            &Event::new(EventSource::Obs, "profile.list_changed", json!({})),
        ));
    }

    #[test]
    fn arg_stack_binds_profile_name_as_string() {
        let event = Event::new(
            EventSource::Obs,
            "profile.current_changed",
            json!({ "profile_name": "Streaming" }),
        );
        let stack = ProfileCurrentChangedDescriptor.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.profile.name"),
            Some(&Variant::String("Streaming".to_owned())),
        );
    }

    #[test]
    fn arg_stack_omits_name_when_payload_field_absent() {
        let event = Event::new(EventSource::Obs, "profile.current_changed", json!({}));
        let stack = ProfileCurrentChangedDescriptor.build_arg_stack(&event);
        assert!(stack.get("obs.profile.name").is_none());
    }
}

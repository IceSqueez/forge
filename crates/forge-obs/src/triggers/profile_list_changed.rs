use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, TriggerConfig, VariableSchema, Variant, VariantKind,
};

pub struct ProfileListChangedDescriptor;

impl TriggerKindDescriptor for ProfileListChangedDescriptor {
    fn id(&self) -> &str {
        "obs.profile.list_changed"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS profile list changed"
    }

    fn summary(&self) -> &str {
        "Fires when profiles are added or removed in OBS."
    }

    fn search_text(&self) -> &str {
        "obs profile list changed added removed"
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
        "any profile list change".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some("profile.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "profile.list_changed"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(names) = event.payload.get("all_names").and_then(|v| v.as_array()) {
            let profiles: Vec<Variant> = names
                .iter()
                .filter_map(|v| v.as_str())
                .map(|s| Variant::String(s.to_owned()))
                .collect();
            stack = stack.set("obs.profile.all_names".to_owned(), Variant::Array(profiles));
        }
        stack
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![DeclaredVariable {
                name: "obs.profile.all_names".to_owned(),
                kind: VariantKind::Array,
                label: "All profile names".to_owned(),
                synthesis: None,
            }],
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_registry::TriggerKindDescriptor;
    use serde_json::json;

    #[test]
    fn matches_list_changed_and_rejects_current_changed_under_profile_prefix() {
        let d = ProfileListChangedDescriptor;
        let cfg = BTreeMap::new();
        assert!(d.matches_trigger(
            &cfg,
            &Event::new(EventSource::Obs, "profile.list_changed", json!({})),
        ));
        assert!(!d.matches_trigger(
            &cfg,
            &Event::new(EventSource::Obs, "profile.current_changed", json!({})),
        ));
    }

    #[test]
    fn arg_stack_collects_all_names_into_string_array() {
        let event = Event::new(
            EventSource::Obs,
            "profile.list_changed",
            json!({ "all_names": ["Streaming", "Recording"] }),
        );
        let stack = ProfileListChangedDescriptor.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.profile.all_names"),
            Some(&Variant::Array(vec![
                Variant::String("Streaming".to_owned()),
                Variant::String("Recording".to_owned()),
            ])),
        );
    }

    #[test]
    fn arg_stack_skips_non_string_array_elements() {
        let event = Event::new(
            EventSource::Obs,
            "profile.list_changed",
            json!({ "all_names": ["Streaming", 42, true, "Recording"] }),
        );
        let stack = ProfileListChangedDescriptor.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.profile.all_names"),
            Some(&Variant::Array(vec![
                Variant::String("Streaming".to_owned()),
                Variant::String("Recording".to_owned()),
            ])),
        );
    }

    #[test]
    fn arg_stack_omits_key_when_all_names_absent() {
        let event = Event::new(EventSource::Obs, "profile.list_changed", json!({}));
        let stack = ProfileListChangedDescriptor.build_arg_stack(&event);
        assert!(stack.get("obs.profile.all_names").is_none());
    }
}

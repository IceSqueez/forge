use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, TriggerConfig, VariableSchema, Variant, VariantKind,
};

pub struct SourceInputRenamedDescriptor;

impl TriggerKindDescriptor for SourceInputRenamedDescriptor {
    fn id(&self) -> &str {
        "obs.sources.input_renamed"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS input source renamed"
    }

    fn summary(&self) -> &str {
        "Fires when an input source is renamed in OBS."
    }

    fn search_text(&self) -> &str {
        "obs input source renamed name changed"
    }

    fn icon_name(&self) -> &str {
        "pencil"
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
        "any input renamed".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some("source.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "source.input_renamed"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(name) = event
            .payload
            .get("source_name_old")
            .and_then(|v| v.as_str())
        {
            stack = stack.set(
                "obs.source.name_old".to_owned(),
                Variant::String(name.to_owned()),
            );
        }
        if let Some(name) = event
            .payload
            .get("source_name_new")
            .and_then(|v| v.as_str())
        {
            stack = stack.set(
                "obs.source.name_new".to_owned(),
                Variant::String(name.to_owned()),
            );
        }
        stack
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![
                DeclaredVariable {
                    name: "obs.source.name_old".to_owned(),
                    kind: VariantKind::String,
                    label: "Old source name".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "obs.source.name_new".to_owned(),
                    kind: VariantKind::String,
                    label: "New source name".to_owned(),
                    synthesis: None,
                },
            ],
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    // 1:1 kind discrimination across the `source.` family is covered in
    // `source_input_created`. Here we test only this descriptor's typed arg-stack
    // extraction, whose two independent keys (`name_old` / `name_new`) are extracted
    // independently.
    use super::*;
    use forge_registry::TriggerKindDescriptor;
    use serde_json::json;

    #[test]
    fn input_renamed_arg_stack_extracts_old_and_new_names() {
        let event = Event::new(
            EventSource::Obs,
            "source.input_renamed",
            json!({ "source_name_old": "Cam", "source_name_new": "Webcam" }),
        );
        let stack = SourceInputRenamedDescriptor.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.source.name_old"),
            Some(&Variant::String("Cam".to_owned())),
        );
        assert_eq!(
            stack.get("obs.source.name_new"),
            Some(&Variant::String("Webcam".to_owned())),
        );
    }

    #[test]
    fn input_renamed_arg_stack_extracts_each_name_independently() {
        let event = Event::new(
            EventSource::Obs,
            "source.input_renamed",
            json!({ "source_name_new": "Webcam" }),
        );
        let stack = SourceInputRenamedDescriptor.build_arg_stack(&event);
        assert!(stack.get("obs.source.name_old").is_none());
        assert_eq!(
            stack.get("obs.source.name_new"),
            Some(&Variant::String("Webcam".to_owned())),
        );
    }

    #[test]
    fn input_renamed_arg_stack_omits_both_keys_when_payload_empty() {
        let event = Event::new(EventSource::Obs, "source.input_renamed", json!({}));
        let stack = SourceInputRenamedDescriptor.build_arg_stack(&event);
        assert!(stack.get("obs.source.name_old").is_none());
        assert!(stack.get("obs.source.name_new").is_none());
    }
}

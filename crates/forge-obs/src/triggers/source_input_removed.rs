use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, TriggerConfig, UserText, VariableSchema, Variant, VariantKind,
};

use crate::vocabulary::fields::source as fields;
use crate::vocabulary::messages;
use crate::vocabulary::{event_kinds, trigger_ids, variables};

pub(crate) struct SourceInputRemovedDescriptor;

impl TriggerKindDescriptor for SourceInputRemovedDescriptor {
    fn id(&self) -> &str {
        trigger_ids::SOURCE_INPUT_REMOVED
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS input source removed"
    }

    fn summary(&self) -> &str {
        "Fires when an input source is removed from OBS."
    }

    fn search_text(&self) -> &str {
        "obs input source removed deleted"
    }

    fn icon_name(&self) -> &str {
        "minus-circle"
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

    fn condition_display(&self, _: &TriggerConfig) -> UserText {
        messages::CONDITION_ANY_INPUT_REMOVED.into()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some(event_kinds::SOURCE_PREFIX.to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == event_kinds::SOURCE_INPUT_REMOVED
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(name) = event
            .payload
            .get(fields::SOURCE_NAME)
            .and_then(|v| v.as_str())
        {
            stack = stack.set(
                variables::source::NAME.to_owned(),
                Variant::String(name.to_owned()),
            );
        }
        stack
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![DeclaredVariable {
                name: variables::source::NAME.to_owned(),
                kind: VariantKind::String,
                label: "Source name".to_owned(),
                synthesis: None,
            }],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_registry::TriggerKindDescriptor;
    use serde_json::json;

    #[test]
    fn input_removed_arg_stack_extracts_source_name() {
        let event = Event::new(
            EventSource::Obs,
            "obs.source.input_removed",
            json!({ "source_name": "Webcam" }),
        );
        let stack = SourceInputRemovedDescriptor.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.source.name"),
            Some(&Variant::String("Webcam".to_owned())),
        );
    }

    #[test]
    fn input_removed_arg_stack_omits_name_when_payload_field_absent() {
        let event = Event::new(EventSource::Obs, "obs.source.input_removed", json!({}));
        let stack = SourceInputRemovedDescriptor.build_arg_stack(&event);
        assert!(stack.get("obs.source.name").is_none());
    }
}

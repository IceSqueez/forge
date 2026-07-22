use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, TriggerConfig, VariableSchema, Variant, VariantKind,
};

use crate::payload_fields::source as fields;

pub struct SourceInputCreatedDescriptor;

impl TriggerKindDescriptor for SourceInputCreatedDescriptor {
    fn id(&self) -> &str {
        "obs.sources.input_created"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS input source created"
    }

    fn summary(&self) -> &str {
        "Fires when a new input source is added to OBS."
    }

    fn search_text(&self) -> &str {
        "obs input source created added new"
    }

    fn icon_name(&self) -> &str {
        "plus-circle"
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
        "any input created".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some("source.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "source.input_created"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(name) = event
            .payload
            .get(fields::SOURCE_NAME)
            .and_then(|v| v.as_str())
        {
            stack = stack.set(
                "obs.source.name".to_owned(),
                Variant::String(name.to_owned()),
            );
        }
        if let Some(kind) = event
            .payload
            .get(fields::SOURCE_KIND)
            .and_then(|v| v.as_str())
        {
            stack = stack.set(
                "obs.source.kind".to_owned(),
                Variant::String(kind.to_owned()),
            );
        }
        stack
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![
                DeclaredVariable {
                    name: "obs.source.name".to_owned(),
                    kind: VariantKind::String,
                    label: "Source name".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "obs.source.kind".to_owned(),
                    kind: VariantKind::String,
                    label: "Source kind".to_owned(),
                    synthesis: None,
                },
            ],
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::super::{
        SourceInputCreatedDescriptor, SourceInputRemovedDescriptor, SourceInputRenamedDescriptor,
        SourceSceneItemVisibilityChangedDescriptor,
    };
    use super::*;
    use forge_registry::TriggerKindDescriptor;
    use serde_json::json;

    const ALL_SOURCE_KINDS: [&str; 4] = [
        "source.input_created",
        "source.input_removed",
        "source.input_renamed",
        "source.visibility.changed",
    ];

    #[test]
    fn each_source_descriptor_matches_only_its_own_kind() {
        let cfg = BTreeMap::new();
        let descriptors: [(&str, &dyn TriggerKindDescriptor); 4] = [
            ("source.input_created", &SourceInputCreatedDescriptor),
            ("source.input_removed", &SourceInputRemovedDescriptor),
            ("source.input_renamed", &SourceInputRenamedDescriptor),
            (
                "source.visibility.changed",
                &SourceSceneItemVisibilityChangedDescriptor,
            ),
        ];
        for (own_kind, descriptor) in descriptors {
            for kind in ALL_SOURCE_KINDS {
                let event = Event::new(EventSource::Obs, kind, json!({}));
                assert_eq!(
                    descriptor.matches_trigger(&cfg, &event),
                    kind == own_kind,
                    "descriptor for {own_kind} given {kind}",
                );
            }
        }
    }

    #[test]
    fn source_descriptor_rejects_non_source_kind() {
        let event = Event::new(EventSource::Obs, "scene.changed", json!({}));
        assert!(!SourceInputCreatedDescriptor.matches_trigger(&BTreeMap::new(), &event));
    }

    #[test]
    fn input_created_arg_stack_extracts_name_and_kind() {
        let event = Event::new(
            EventSource::Obs,
            "source.input_created",
            json!({ "source_name": "Mic", "source_kind": "wasapi_input_capture" }),
        );
        let stack = SourceInputCreatedDescriptor.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.source.name"),
            Some(&Variant::String("Mic".to_owned())),
        );
        assert_eq!(
            stack.get("obs.source.kind"),
            Some(&Variant::String("wasapi_input_capture".to_owned())),
        );
    }

    #[test]
    fn input_created_arg_stack_omits_keys_when_payload_fields_absent() {
        let event = Event::new(EventSource::Obs, "source.input_created", json!({}));
        let stack = SourceInputCreatedDescriptor.build_arg_stack(&event);
        assert!(stack.get("obs.source.name").is_none());
        assert!(stack.get("obs.source.kind").is_none());
    }
}

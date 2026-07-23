use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, TriggerConfig, VariableSchema, Variant, VariantKind,
};

use crate::payload_fields::recording as fields;

pub struct RecordFileChangedDescriptor;

impl TriggerKindDescriptor for RecordFileChangedDescriptor {
    fn id(&self) -> &str {
        "obs.record.file_changed"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS recording file changed"
    }

    fn summary(&self) -> &str {
        "Fires when OBS switches to a new output file mid-session (e.g. automatic file split)."
    }

    fn search_text(&self) -> &str {
        "obs recording file changed split new path"
    }

    fn icon_name(&self) -> &str {
        "record"
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
        "recording file changed".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some("obs.recording.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "obs.recording.file_changed"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(p) = event
            .payload
            .get(fields::OUTPUT_PATH)
            .and_then(|v| v.as_str())
        {
            stack = stack.set(
                "obs.record.output_path".to_owned(),
                Variant::String(p.to_owned()),
            );
        }
        stack
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![DeclaredVariable {
                name: "obs.record.output_path".to_owned(),
                kind: VariantKind::String,
                label: "New recording file path".to_owned(),
                synthesis: None,
            }],
        })
    }
}

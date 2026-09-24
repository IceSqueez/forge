use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, TriggerConfig, VariableSchema, Variant, VariantKind,
};

use crate::payload_fields::source as fields;

pub struct SourceSceneItemCreatedDescriptor;

impl TriggerKindDescriptor for SourceSceneItemCreatedDescriptor {
    fn id(&self) -> &str {
        "obs.sources.scene_item_created"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS scene item added"
    }

    fn summary(&self) -> &str {
        "Fires when a source is added to a scene in OBS."
    }

    fn search_text(&self) -> &str {
        "obs scene item source added new"
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
        "any scene item added".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some("obs.source.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "obs.source.scene_item_created"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(scene) = event
            .payload
            .get(fields::SCENE_NAME)
            .and_then(|v| v.as_str())
        {
            stack = stack.set(
                "obs.scene.name".to_owned(),
                Variant::String(scene.to_owned()),
            );
        }
        if let Some(source) = event
            .payload
            .get(fields::SOURCE_NAME)
            .and_then(|v| v.as_str())
        {
            stack = stack.set(
                "obs.source.name".to_owned(),
                Variant::String(source.to_owned()),
            );
        }
        if let Some(item_id) = event.payload.get(fields::ITEM_ID).and_then(|v| v.as_i64()) {
            stack = stack.set("obs.source.item_id".to_owned(), Variant::Int(item_id));
        }
        stack
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![
                DeclaredVariable {
                    name: "obs.scene.name".to_owned(),
                    kind: VariantKind::String,
                    label: "Scene name".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "obs.source.name".to_owned(),
                    kind: VariantKind::String,
                    label: "Source name".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "obs.source.item_id".to_owned(),
                    kind: VariantKind::Int,
                    label: "Scene item ID".to_owned(),
                    synthesis: None,
                },
            ],
        })
    }
}

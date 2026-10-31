use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, TriggerConfig, UserText, VariableSchema, Variant, VariantKind,
};

use crate::vocabulary::fields::item as fields;
use crate::vocabulary::messages;
use crate::vocabulary::{event_kinds, trigger_ids, variables};

pub(crate) struct ItemRemovedDescriptor;

impl TriggerKindDescriptor for ItemRemovedDescriptor {
    fn id(&self) -> &str {
        trigger_ids::ITEM_REMOVED
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::VTube
    }

    fn label(&self) -> &str {
        "VTube Studio item removed"
    }

    fn summary(&self) -> &str {
        "Fires when an item is removed from the VTube Studio scene."
    }

    fn search_text(&self) -> &str {
        "vtube item removed unloaded prop sticker"
    }

    fn icon_name(&self) -> &str {
        "image-off"
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
        messages::CONDITION_ANY_ITEM.into()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::VTube),
            kind_prefix: Some(event_kinds::ITEM_PREFIX.to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == event_kinds::ITEM_REMOVED
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(id) = event
            .payload
            .get(fields::ITEM_INSTANCE_ID)
            .and_then(|v| v.as_str())
        {
            stack = stack.set(
                variables::item::INSTANCE_ID.to_owned(),
                Variant::String(id.to_owned()),
            );
        }
        if let Some(file) = event
            .payload
            .get(fields::ITEM_FILE_NAME)
            .and_then(|v| v.as_str())
        {
            stack = stack.set(
                variables::item::FILE_NAME.to_owned(),
                Variant::String(file.to_owned()),
            );
        }
        stack
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![
                DeclaredVariable {
                    name: variables::item::INSTANCE_ID.to_owned(),
                    kind: VariantKind::String,
                    label: "Item instance ID".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: variables::item::FILE_NAME.to_owned(),
                    kind: VariantKind::String,
                    label: "Item file name".to_owned(),
                    synthesis: None,
                },
            ],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn event(kind: &str, payload: serde_json::Value) -> Event {
        Event::new(EventSource::VTube, kind, payload)
    }

    #[test]
    fn matches_only_the_exact_removed_kind() {
        let d = ItemRemovedDescriptor;
        let cfg = TriggerConfig::new();
        assert!(d.matches_trigger(&cfg, &event("vtube.item.removed", json!({}))));
        assert!(!d.matches_trigger(&cfg, &event("vtube.item.added", json!({}))));
        assert!(!d.matches_trigger(&cfg, &event("vtube.model.loaded", json!({}))));
    }

    #[test]
    fn build_arg_stack_maps_present_payload_keys() {
        let d = ItemRemovedDescriptor;
        let stack = d.build_arg_stack(&event(
            "vtube.item.removed",
            json!({ "item_instance_id": "inst-7", "item_file_name": "hat.png" }),
        ));
        assert_eq!(
            stack.get("vtube.item.instance_id"),
            Some(&Variant::String("inst-7".to_owned()))
        );
        assert_eq!(
            stack.get("vtube.item.file_name"),
            Some(&Variant::String("hat.png".to_owned()))
        );
    }

    #[test]
    fn build_arg_stack_omits_missing_payload_keys() {
        let d = ItemRemovedDescriptor;
        let stack = d.build_arg_stack(&event(
            "vtube.item.removed",
            json!({ "item_file_name": "hat.png" }),
        ));
        assert!(stack.get("vtube.item.instance_id").is_none());
        assert_eq!(
            stack.get("vtube.item.file_name"),
            Some(&Variant::String("hat.png".to_owned()))
        );
    }
}

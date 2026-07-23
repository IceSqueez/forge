use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, TriggerConfig, VariableSchema, Variant, VariantKind,
};

use crate::payload_fields::stream as fields;

pub(crate) struct LivestreamMetadataDescriptor;

impl TriggerKindDescriptor for LivestreamMetadataDescriptor {
    fn id(&self) -> &str {
        "kick.livestream.metadata.updated"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Streams
    }

    fn label(&self) -> &str {
        "Livestream metadata"
    }

    fn summary(&self) -> &str {
        "Fires when the Kick channel stream title or category changes"
    }

    fn search_text(&self) -> &str {
        "kick livestream metadata title category edit update stream"
    }

    fn icon_name(&self) -> &str {
        "edit"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::Kick)
    }

    fn default_config(&self) -> TriggerConfig {
        TriggerConfig::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![]
    }

    fn condition_display(&self, _config: &TriggerConfig) -> String {
        "any".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Kick),
            kind_prefix: Some("kick.livestream.metadata.updated".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let stream_title = event
            .payload
            .get(fields::STREAM_TITLE)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        let category = event.payload.get(fields::CATEGORY);
        let category_id = category
            .and_then(|c| c.get(fields::CATEGORY_ID))
            .and_then(|v| v.as_u64())
            .map_or_else(String::new, |n| n.to_string());
        let category_name = category
            .and_then(|c| c.get(fields::CATEGORY_NAME))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("stream_title".to_owned(), Variant::String(stream_title))
            .set("category_id".to_owned(), Variant::String(category_id))
            .set("category_name".to_owned(), Variant::String(category_name))
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![
                DeclaredVariable {
                    name: "stream_title".to_owned(),
                    kind: VariantKind::String,
                    label: "Stream title".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "category_id".to_owned(),
                    kind: VariantKind::String,
                    label: "Category ID".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "category_name".to_owned(),
                    kind: VariantKind::String,
                    label: "Category name".to_owned(),
                    synthesis: None,
                },
            ],
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn build_arg_stack_extracts_metadata_fields_with_nested_category() {
        let event = Event::new(
            EventSource::Kick,
            "kick.channel.livestream_metadata",
            serde_json::json!({
                "stream_title": "New title",
                "category": { "id": 7, "name": "Software & Game Dev" }
            }),
        );

        let stack = LivestreamMetadataDescriptor.build_arg_stack(&event);

        assert_eq!(
            stack.get("stream_title"),
            Some(&Variant::String("New title".to_owned()))
        );
        assert_eq!(
            stack.get("category_id"),
            Some(&Variant::String("7".to_owned()))
        );
        assert_eq!(
            stack.get("category_name"),
            Some(&Variant::String("Software & Game Dev".to_owned()))
        );
    }

    #[test]
    fn build_arg_stack_leaves_category_fields_empty_when_object_absent() {
        let event = Event::new(
            EventSource::Kick,
            "kick.channel.livestream_metadata",
            serde_json::json!({
                "stream_title": "Title only"
            }),
        );

        let stack = LivestreamMetadataDescriptor.build_arg_stack(&event);

        assert_eq!(
            stack.get("stream_title"),
            Some(&Variant::String("Title only".to_owned()))
        );
        assert_eq!(
            stack.get("category_id"),
            Some(&Variant::String(String::new()))
        );
        assert_eq!(
            stack.get("category_name"),
            Some(&Variant::String(String::new()))
        );
    }
}

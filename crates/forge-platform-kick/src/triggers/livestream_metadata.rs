use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{DeclaredVariable, PlatformId, TriggerConfig, Variant, VariantKind};

use super::payload_read;
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

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .event_specific(
                    DeclaredVariable {
                        name: "stream_title".to_owned(),
                        kind: VariantKind::String,
                        label: "Stream title".to_owned(),
                        synthesis: None,
                    },
                    |event| Variant::String(payload_read::text(event, fields::STREAM_TITLE)),
                )
                .event_specific(
                    DeclaredVariable {
                        name: "category_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Category ID".to_owned(),
                        synthesis: None,
                    },
                    |event| {
                        Variant::String(payload_read::numeric_id(
                            event.payload.get(fields::CATEGORY),
                            fields::CATEGORY_ID,
                        ))
                    },
                )
                .event_specific(
                    DeclaredVariable {
                        name: "category_name".to_owned(),
                        kind: VariantKind::String,
                        label: "Category name".to_owned(),
                        synthesis: None,
                    },
                    |event| {
                        Variant::String(payload_read::nested_text(
                            event.payload.get(fields::CATEGORY),
                            fields::CATEGORY_NAME,
                        ))
                    },
                ),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::Actorless
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

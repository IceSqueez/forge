use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{DeclaredVariable, PlatformId, TriggerConfig, Variant, VariantKind};

use super::payload_read;
use crate::vocabulary::fields::stream as fields;
use crate::vocabulary::{event_kinds, trigger_ids, variables};

pub(crate) struct LivestreamMetadataDescriptor;

impl TriggerKindDescriptor for LivestreamMetadataDescriptor {
    fn id(&self) -> &str {
        trigger_ids::LIVESTREAM_METADATA_UPDATED
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

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Kick),
            kind_prefix: Some(event_kinds::LIVESTREAM_METADATA_UPDATED.to_owned()),
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
                        name: variables::livestream::STREAM_TITLE.to_owned(),
                        kind: VariantKind::String,
                        label: "Stream title".to_owned(),
                        synthesis: None,
                    },
                    |event| Variant::String(payload_read::text(event, fields::STREAM_TITLE)),
                )
                .event_specific(
                    DeclaredVariable {
                        name: variables::livestream::CATEGORY_ID.to_owned(),
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
                        name: variables::livestream::CATEGORY_NAME.to_owned(),
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
mod tests {
    use super::*;

    use serde_json::json;

    #[test]
    fn the_title_and_the_nested_category_are_read_from_the_payload() {
        for (payload, title, category_id, category_name) in [
            (
                json!({
                    "stream_title": "New title",
                    "category": { "id": 7, "name": "Software & Game Dev" }
                }),
                "New title",
                "7",
                "Software & Game Dev",
            ),
            (
                json!({ "stream_title": "Title only" }),
                "Title only",
                "",
                "",
            ),
            (json!({ "category": { "id": 7 } }), "", "7", ""),
        ] {
            let stack = LivestreamMetadataDescriptor.build_arg_stack(&Event::new(
                EventSource::Kick,
                "kick.livestream.metadata.updated",
                payload.clone(),
            ));
            for (name, value) in [
                ("stream_title", title),
                ("category_id", category_id),
                ("category_name", category_name),
            ] {
                assert_eq!(
                    stack.get(name),
                    Some(&Variant::String(value.to_owned())),
                    "'{name}' for {payload}"
                );
            }
        }
    }
}

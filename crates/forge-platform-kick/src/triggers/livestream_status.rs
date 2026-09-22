use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{DeclaredVariable, PlatformId, TriggerConfig, Variant, VariantKind};

use super::payload_read;
use crate::payload_fields::stream as fields;

pub(crate) struct LivestreamStatusDescriptor;

impl TriggerKindDescriptor for LivestreamStatusDescriptor {
    fn id(&self) -> &str {
        "kick.livestream.status.updated"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Streams
    }

    fn label(&self) -> &str {
        "Livestream status"
    }

    fn summary(&self) -> &str {
        "Fires when the Kick channel livestream status changes (live or offline)"
    }

    fn search_text(&self) -> &str {
        "kick livestream status live offline stream channel"
    }

    fn icon_name(&self) -> &str {
        "radio"
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
            kind_prefix: Some("kick.livestream.status.updated".to_owned()),
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
                        name: "is_live".to_owned(),
                        kind: VariantKind::Bool,
                        label: "Is live".to_owned(),
                        synthesis: None,
                    },
                    |event| Variant::Bool(payload_read::flag(event, fields::IS_LIVE)),
                )
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
    fn build_arg_stack_extracts_status_fields_with_nested_category() {
        let event = Event::new(
            EventSource::Kick,
            "kick.channel.livestream_status",
            serde_json::json!({
                "is_live": true,
                "stream_title": "Late night coding",
                "category": { "id": 42, "name": "Just Chatting" }
            }),
        );

        let stack = LivestreamStatusDescriptor.build_arg_stack(&event);

        assert_eq!(stack.get("is_live"), Some(&Variant::Bool(true)));
        assert_eq!(
            stack.get("stream_title"),
            Some(&Variant::String("Late night coding".to_owned()))
        );
        assert_eq!(
            stack.get("category_id"),
            Some(&Variant::String("42".to_owned()))
        );
        assert_eq!(
            stack.get("category_name"),
            Some(&Variant::String("Just Chatting".to_owned()))
        );
    }

    #[test]
    fn build_arg_stack_leaves_category_fields_empty_when_object_absent() {
        let event = Event::new(
            EventSource::Kick,
            "kick.channel.livestream_status",
            serde_json::json!({
                "is_live": false,
                "stream_title": "Offline"
            }),
        );

        let stack = LivestreamStatusDescriptor.build_arg_stack(&event);

        assert_eq!(stack.get("is_live"), Some(&Variant::Bool(false)));
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

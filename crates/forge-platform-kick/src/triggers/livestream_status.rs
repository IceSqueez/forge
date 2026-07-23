use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, TriggerConfig, VariableSchema, Variant, VariantKind,
};

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

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let is_live = event
            .payload
            .get(fields::IS_LIVE)
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

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
            .set("is_live".to_owned(), Variant::Bool(is_live))
            .set("stream_title".to_owned(), Variant::String(stream_title))
            .set("category_id".to_owned(), Variant::String(category_id))
            .set("category_name".to_owned(), Variant::String(category_name))
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![
                DeclaredVariable {
                    name: "is_live".to_owned(),
                    kind: VariantKind::Bool,
                    label: "Is live".to_owned(),
                    synthesis: None,
                },
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

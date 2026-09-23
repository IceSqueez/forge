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

    use serde_json::json;

    #[test]
    fn the_status_and_the_nested_category_are_read_from_the_payload() {
        for (payload, live, title, category_id, category_name) in [
            (
                json!({
                    "is_live": true,
                    "stream_title": "Late night coding",
                    "category": { "id": 42, "name": "Just Chatting" }
                }),
                true,
                "Late night coding",
                "42",
                "Just Chatting",
            ),
            (
                json!({ "is_live": false, "stream_title": "Offline" }),
                false,
                "Offline",
                "",
                "",
            ),
            (
                json!({ "is_live": true, "category": { "name": "Software & Game Dev" } }),
                true,
                "",
                "",
                "Software & Game Dev",
            ),
        ] {
            let stack = LivestreamStatusDescriptor.build_arg_stack(&Event::new(
                EventSource::Kick,
                "kick.livestream.status.updated",
                payload.clone(),
            ));
            assert_eq!(
                stack.get("is_live"),
                Some(&Variant::Bool(live)),
                "{payload}"
            );
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

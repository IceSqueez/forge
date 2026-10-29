use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{DeclaredVariable, PlatformId, TriggerConfig, Variant, VariantKind};

use super::payload_read;
use crate::vocabulary::fields::stream as fields;
use crate::vocabulary::{event_kinds, trigger_ids, variables};

pub(crate) struct ChannelBroadcastTitleChangedDescriptor;

impl TriggerKindDescriptor for ChannelBroadcastTitleChangedDescriptor {
    fn id(&self) -> &str {
        trigger_ids::STREAM_TITLE_CHANGED
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Streams
    }

    fn label(&self) -> &str {
        "Live broadcast title changed"
    }

    fn summary(&self) -> &str {
        "Fires when the title of an active YouTube live broadcast is edited"
    }

    fn search_text(&self) -> &str {
        "youtube live stream title changed renamed broadcast edit"
    }

    fn icon_name(&self) -> &str {
        "edit"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::YouTube)
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
            source: Some(EventSource::YouTube),
            kind_prefix: Some(event_kinds::STREAM_TITLE_CHANGED.to_owned()),
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
                        name: variables::stream::TITLE_OLD.to_owned(),
                        kind: VariantKind::String,
                        label: "Previous broadcast title".to_owned(),
                        synthesis: None,
                    },
                    |event| {
                        Variant::String(payload_read::nested_text(
                            event,
                            fields::TITLE,
                            fields::OLD,
                        ))
                    },
                )
                .event_specific(
                    DeclaredVariable {
                        name: variables::stream::TITLE_NEW.to_owned(),
                        kind: VariantKind::String,
                        label: "New broadcast title".to_owned(),
                        synthesis: None,
                    },
                    |event| {
                        Variant::String(payload_read::nested_text(
                            event,
                            fields::TITLE,
                            fields::NEW,
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

    fn title_changed_event(old: &str, new: &str) -> Event {
        Event::new(
            EventSource::YouTube,
            "youtube.stream.title_changed",
            serde_json::json!({
                "title": { "old": old, "new": new },
            }),
        )
    }

    #[test]
    fn a_title_change_keeps_the_previous_title_apart_from_the_one_that_replaced_it() {
        let stack = ChannelBroadcastTitleChangedDescriptor
            .build_arg_stack(&title_changed_event("Morning Coding", "Afternoon Coding"));
        assert_eq!(
            stack.get("stream.title_old"),
            Some(&Variant::String("Morning Coding".to_owned()))
        );
        assert_eq!(
            stack.get("stream.title_new"),
            Some(&Variant::String("Afternoon Coding".to_owned()))
        );
    }
}

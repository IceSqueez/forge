use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, TriggerConfig, VariableSchema, Variant, VariantKind,
};

pub(crate) struct ChannelBroadcastTitleChangedDescriptor;

impl TriggerKindDescriptor for ChannelBroadcastTitleChangedDescriptor {
    fn id(&self) -> &str {
        "youtube.stream.title_changed"
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
            kind_prefix: Some("youtube.stream.title_changed".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let title_old = event
            .payload
            .get("stream.title_old")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let title_new = event
            .payload
            .get("stream.title_new")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("stream.title_old".to_owned(), Variant::String(title_old))
            .set("stream.title_new".to_owned(), Variant::String(title_new))
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![
                DeclaredVariable {
                    name: "stream.title_old".to_owned(),
                    kind: VariantKind::String,
                    label: "Previous broadcast title".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "stream.title_new".to_owned(),
                    kind: VariantKind::String,
                    label: "New broadcast title".to_owned(),
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

    fn title_changed_event(old: &str, new: &str) -> Event {
        Event::new(
            EventSource::YouTube,
            "youtube.stream.title_changed",
            serde_json::json!({
                "stream.title_old": old,
                "stream.title_new": new,
            }),
        )
    }

    #[test]
    fn build_arg_stack_maps_old_and_new_title() {
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

    #[test]
    fn build_arg_stack_defaults_missing_payload_fields_to_empty() {
        let event = Event::new(
            EventSource::YouTube,
            "youtube.stream.title_changed",
            serde_json::json!({}),
        );
        let stack = ChannelBroadcastTitleChangedDescriptor.build_arg_stack(&event);
        assert_eq!(
            stack.get("stream.title_old"),
            Some(&Variant::String(String::new()))
        );
        assert_eq!(
            stack.get("stream.title_new"),
            Some(&Variant::String(String::new()))
        );
    }

    #[test]
    fn event_filter_targets_own_kind_on_youtube_source() {
        // Contract: the filter must route the exact kind this descriptor handles,
        // from the YouTube source. A copy-paste drift pointing kind_prefix at a
        // sibling trigger would silently never match this descriptor's events.
        let filter = ChannelBroadcastTitleChangedDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::YouTube));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some(ChannelBroadcastTitleChangedDescriptor.id())
        );
    }
}

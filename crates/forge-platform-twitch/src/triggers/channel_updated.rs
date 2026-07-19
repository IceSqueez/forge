use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, TriggerConfig, VariableSchema, Variant, VariantKind,
};

pub(crate) struct ChannelUpdatedDescriptor;

impl TriggerKindDescriptor for ChannelUpdatedDescriptor {
    fn id(&self) -> &str {
        "twitch.channel.update"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Streams
    }

    fn label(&self) -> &str {
        "Channel updated"
    }

    fn summary(&self) -> &str {
        "Fires when the broadcaster updates their channel title, category, or language"
    }

    fn search_text(&self) -> &str {
        "twitch channel update title category game language"
    }

    fn icon_name(&self) -> &str {
        "broadcast"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::Twitch)
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
            source: Some(EventSource::Twitch),
            kind_prefix: Some("channel.update".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let channel = event.payload.get("channel");

        let title = channel
            .and_then(|c| c.get("title"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let category_id = channel
            .and_then(|c| c.get("category_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let category_name = channel
            .and_then(|c| c.get("category_name"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let language = channel
            .and_then(|c| c.get("language"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("channel.title".to_owned(), Variant::String(title))
            .set(
                "channel.category_id".to_owned(),
                Variant::String(category_id),
            )
            .set(
                "channel.category_name".to_owned(),
                Variant::String(category_name),
            )
            .set("channel.language".to_owned(), Variant::String(language))
    }
    fn output_schema(&self) -> Option<VariableSchema> {
        Some({
            VariableSchema {
                variables: vec![
                    DeclaredVariable {
                        name: "channel.title".to_owned(),
                        kind: VariantKind::String,
                        label: "Channel title".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "channel.category_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Category ID".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "channel.category_name".to_owned(),
                        kind: VariantKind::String,
                        label: "Category name".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "channel.language".to_owned(),
                        kind: VariantKind::String,
                        label: "Broadcast language".to_owned(),
                        synthesis: None,
                    },
                ],
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channel_update_event() -> Event {
        let payload = serde_json::json!({
            "channel": {
                "title": "New title",
                "language": "en",
                "category_id": "509658",
                "category_name": "Just Chatting",
            },
        });
        Event::new(EventSource::Twitch, "channel.update", payload)
    }

    #[test]
    fn event_filter_targets_channel_update_topic_from_twitch() {
        let filter = ChannelUpdatedDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(filter.kind_prefix.as_deref(), Some("channel.update"));
    }

    #[test]
    fn build_arg_stack_maps_channel_fields_from_nested_payload() {
        let stack = ChannelUpdatedDescriptor.build_arg_stack(&channel_update_event());
        assert_eq!(
            stack.get("channel.title"),
            Some(&Variant::String("New title".to_owned()))
        );
        assert_eq!(
            stack.get("channel.category_id"),
            Some(&Variant::String("509658".to_owned()))
        );
        assert_eq!(
            stack.get("channel.category_name"),
            Some(&Variant::String("Just Chatting".to_owned()))
        );
        assert_eq!(
            stack.get("channel.language"),
            Some(&Variant::String("en".to_owned()))
        );
    }

    #[test]
    fn build_arg_stack_defaults_missing_fields_to_empty_strings() {
        let event = Event::new(EventSource::Twitch, "channel.update", serde_json::json!({}));
        let stack = ChannelUpdatedDescriptor.build_arg_stack(&event);
        assert_eq!(
            stack.get("channel.title"),
            Some(&Variant::String(String::new()))
        );
        assert_eq!(
            stack.get("channel.language"),
            Some(&Variant::String(String::new()))
        );
    }
}

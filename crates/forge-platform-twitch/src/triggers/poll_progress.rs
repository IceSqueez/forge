use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, TriggerConfig, VariableSchema, Variant, VariantKind,
};

pub(crate) struct PollProgressDescriptor;

impl TriggerKindDescriptor for PollProgressDescriptor {
    fn id(&self) -> &str {
        "twitch.poll.progress"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Polls
    }

    fn label(&self) -> &str {
        "Poll progress"
    }

    fn summary(&self) -> &str {
        "Fires when vote counts update on an active poll"
    }

    fn search_text(&self) -> &str {
        "twitch poll progress update vote"
    }

    fn icon_name(&self) -> &str {
        "chart-bar"
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
            kind_prefix: Some("channel.poll.progress".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let poll = event.payload.get("poll");

        let poll_id = poll
            .and_then(|v| v.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let title = poll
            .and_then(|v| v.get("title"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("poll.id".to_owned(), Variant::String(poll_id))
            .set("poll.title".to_owned(), Variant::String(title))
    }
    fn output_schema(&self) -> Option<VariableSchema> {
        Some({
            VariableSchema {
                variables: vec![
                    DeclaredVariable {
                        name: "poll.id".to_owned(),
                        kind: VariantKind::String,
                        label: "Poll ID".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "poll.title".to_owned(),
                        kind: VariantKind::String,
                        label: "Poll title".to_owned(),
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

    fn poll_progress_event() -> Event {
        let payload = serde_json::json!({
            "poll": {
                "id": "poll-2",
                "title": "Next game?",
            },
        });
        Event::new(EventSource::Twitch, "channel.poll.progress", payload)
    }

    #[test]
    fn event_filter_targets_poll_progress_topic_from_twitch() {
        let filter = PollProgressDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(filter.kind_prefix.as_deref(), Some("channel.poll.progress"));
    }

    #[test]
    fn build_arg_stack_maps_poll_id_and_title() {
        let stack = PollProgressDescriptor.build_arg_stack(&poll_progress_event());
        assert_eq!(
            stack.get("poll.id"),
            Some(&Variant::String("poll-2".to_owned()))
        );
        assert_eq!(
            stack.get("poll.title"),
            Some(&Variant::String("Next game?".to_owned()))
        );
    }
}

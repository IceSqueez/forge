use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, TriggerConfig, VariableSchema, Variant, VariantKind,
};

use crate::payload_fields::poll as fields;

pub(crate) struct PollEndedDescriptor;

impl TriggerKindDescriptor for PollEndedDescriptor {
    fn id(&self) -> &str {
        "twitch.poll.ended"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Polls
    }

    fn label(&self) -> &str {
        "Poll ended"
    }

    fn summary(&self) -> &str {
        "Fires when a poll ends (completed, archived, or terminated)"
    }

    fn search_text(&self) -> &str {
        "twitch poll ended completed archived terminated"
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
            kind_prefix: Some("channel.poll.end".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let poll = event.payload.get(fields::POLL);

        let poll_id = poll
            .and_then(|v| v.get(fields::POLL_ID))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let title = poll
            .and_then(|v| v.get(fields::POLL_TITLE))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let status = poll
            .and_then(|v| v.get(fields::STATUS))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let ended_at = poll
            .and_then(|v| v.get(fields::ENDED_AT))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("poll.id".to_owned(), Variant::String(poll_id))
            .set("poll.title".to_owned(), Variant::String(title))
            .set("poll.status".to_owned(), Variant::String(status))
            .set("poll.ended_at".to_owned(), Variant::String(ended_at))
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
                    DeclaredVariable {
                        name: "poll.status".to_owned(),
                        kind: VariantKind::String,
                        label: "Poll status".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "poll.ended_at".to_owned(),
                        kind: VariantKind::String,
                        label: "Ended at".to_owned(),
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

    fn poll_end_event() -> Event {
        let payload = serde_json::json!({
            "poll": {
                "id": "poll-3",
                "title": "Map vote",
                "status": "completed",
                "ended_at": "2026-06-13T18:10:00Z",
            },
        });
        Event::new(EventSource::Twitch, "channel.poll.end", payload)
    }

    #[test]
    fn event_filter_targets_poll_end_topic_from_twitch() {
        let filter = PollEndedDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(filter.kind_prefix.as_deref(), Some("channel.poll.end"));
    }

    #[test]
    fn build_arg_stack_maps_status_and_ended_at_alongside_id_and_title() {
        let stack = PollEndedDescriptor.build_arg_stack(&poll_end_event());
        assert_eq!(
            stack.get("poll.id"),
            Some(&Variant::String("poll-3".to_owned()))
        );
        assert_eq!(
            stack.get("poll.title"),
            Some(&Variant::String("Map vote".to_owned()))
        );
        assert_eq!(
            stack.get("poll.status"),
            Some(&Variant::String("completed".to_owned()))
        );
        assert_eq!(
            stack.get("poll.ended_at"),
            Some(&Variant::String("2026-06-13T18:10:00Z".to_owned()))
        );
    }
}

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, TriggerConfig, VariableSchema, Variant, VariantKind,
};

use crate::payload_fields::poll as fields;

pub(crate) struct PollStartedDescriptor;

impl TriggerKindDescriptor for PollStartedDescriptor {
    fn id(&self) -> &str {
        "twitch.poll.started"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Polls
    }

    fn label(&self) -> &str {
        "Poll started"
    }

    fn summary(&self) -> &str {
        "Fires when a poll begins on the broadcaster's channel"
    }

    fn search_text(&self) -> &str {
        "twitch poll started begin vote"
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
            kind_prefix: Some("twitch.channel.poll.begin".to_owned()),
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
        let started_at = poll
            .and_then(|v| v.get(fields::STARTED_AT))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let ends_at = poll
            .and_then(|v| v.get(fields::ENDS_AT))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let choices = build_choices_variant(event);

        ArgStack::new()
            .set("poll.id".to_owned(), Variant::String(poll_id))
            .set("poll.title".to_owned(), Variant::String(title))
            .set("poll.started_at".to_owned(), Variant::String(started_at))
            .set("poll.ends_at".to_owned(), Variant::String(ends_at))
            .set("poll.choices".to_owned(), choices)
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
                        name: "poll.started_at".to_owned(),
                        kind: VariantKind::String,
                        label: "Started at".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "poll.ends_at".to_owned(),
                        kind: VariantKind::String,
                        label: "Ends at".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "poll.choices".to_owned(),
                        kind: VariantKind::Array,
                        label: "Poll choices".to_owned(),
                        synthesis: None,
                    },
                ],
            }
        })
    }
}

pub(crate) fn build_choices_variant(event: &Event) -> Variant {
    let choices = event
        .payload
        .get(fields::CHOICES)
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    Variant::Array(
        choices
            .iter()
            .map(|choice| {
                let mut obj = std::collections::BTreeMap::new();
                obj.insert(
                    "id".to_owned(),
                    Variant::String(
                        choice
                            .get(fields::CHOICE_ID)
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_owned(),
                    ),
                );
                obj.insert(
                    "title".to_owned(),
                    Variant::String(
                        choice
                            .get(fields::CHOICE_TITLE)
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_owned(),
                    ),
                );
                obj.insert(
                    "votes".to_owned(),
                    Variant::Int(
                        choice
                            .get(fields::CHOICE_VOTES)
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0),
                    ),
                );
                Variant::Object(obj)
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn poll_begin_event() -> Event {
        let payload = serde_json::json!({
            "poll": {
                "id": "poll-1",
                "title": "Best emote?",
                "started_at": "2026-06-13T18:00:00Z",
                "ends_at": "2026-06-13T18:05:00Z",
            },
        });
        Event::new(EventSource::Twitch, "twitch.channel.poll.begin", payload)
    }

    #[test]
    fn event_filter_targets_poll_begin_topic_from_twitch() {
        let filter = PollStartedDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("twitch.channel.poll.begin")
        );
    }

    #[test]
    fn build_arg_stack_maps_poll_id_title_and_timing_fields() {
        let stack = PollStartedDescriptor.build_arg_stack(&poll_begin_event());
        assert_eq!(
            stack.get("poll.id"),
            Some(&Variant::String("poll-1".to_owned()))
        );
        assert_eq!(
            stack.get("poll.title"),
            Some(&Variant::String("Best emote?".to_owned()))
        );
        assert_eq!(
            stack.get("poll.started_at"),
            Some(&Variant::String("2026-06-13T18:00:00Z".to_owned()))
        );
        assert_eq!(
            stack.get("poll.ends_at"),
            Some(&Variant::String("2026-06-13T18:05:00Z".to_owned()))
        );
    }
}

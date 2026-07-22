use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, TriggerConfig, VariableSchema, Variant, VariantKind,
};

use crate::payload_fields::guest_star as guest_star_fields;

pub(crate) struct GuestStarSessionEndedDescriptor;

impl TriggerKindDescriptor for GuestStarSessionEndedDescriptor {
    fn id(&self) -> &str {
        "twitch.guest_star.session_ended"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Streams
    }

    fn label(&self) -> &str {
        "Guest Star session ended"
    }

    fn summary(&self) -> &str {
        "Fires when a running Guest Star session ends"
    }

    fn search_text(&self) -> &str {
        "twitch guest star session ended stopped host"
    }

    fn icon_name(&self) -> &str {
        "star"
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
            kind_prefix: Some("channel.guest_star_session.end".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let session = event.payload.get(guest_star_fields::SESSION);

        // guest_star.session_id is the chaining var for guest-star sub-actions (%guest_star.session_id%).
        let session_id = session
            .and_then(|s| s.get(guest_star_fields::SESSION_ID))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let ended_at = session
            .and_then(|s| s.get(guest_star_fields::SESSION_ENDED_AT))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set(
                "guest_star.session_id".to_owned(),
                Variant::String(session_id),
            )
            .set("guest_star.ended_at".to_owned(), Variant::String(ended_at))
    }
    fn output_schema(&self) -> Option<VariableSchema> {
        Some({
            VariableSchema {
                variables: vec![
                    DeclaredVariable {
                        name: "guest_star.session_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Guest Star session ID".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "guest_star.ended_at".to_owned(),
                        kind: VariantKind::String,
                        label: "Session ended at".to_owned(),
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

    fn end_event() -> Event {
        Event::new(
            EventSource::Twitch,
            "channel.guest_star_session.end",
            serde_json::json!({
                "session": { "id": "sess-42", "ended_at": "2026-06-13T21:00:00Z" },
            }),
        )
    }

    #[test]
    fn event_filter_targets_session_end_kind_from_twitch() {
        let filter = GuestStarSessionEndedDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("channel.guest_star_session.end")
        );
    }

    #[test]
    fn build_arg_stack_exposes_session_id_chaining_var_and_ended_at() {
        let stack = GuestStarSessionEndedDescriptor.build_arg_stack(&end_event());
        assert_eq!(
            stack.get("guest_star.session_id"),
            Some(&Variant::String("sess-42".to_owned()))
        );
        assert_eq!(
            stack.get("guest_star.ended_at"),
            Some(&Variant::String("2026-06-13T21:00:00Z".to_owned()))
        );
    }
}

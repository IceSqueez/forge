use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

pub(crate) struct GuestStarGuestUpdatedDescriptor;

impl TriggerKindDescriptor for GuestStarGuestUpdatedDescriptor {
    fn id(&self) -> &str {
        "twitch.guest_star.guest_updated"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Streams
    }

    fn label(&self) -> &str {
        "Guest Star guest updated"
    }

    fn summary(&self) -> &str {
        "Fires when a Guest Star guest changes state in the session"
    }

    fn search_text(&self) -> &str {
        "twitch guest star guest update state slot invited accepted ready backstage live removed"
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
        // state_filter="" fires on every state transition.
        // Named states from Twitch Guest Star BETA docs cover the catalog's
        // conceptual split (guest_joined=>"live", guest_left=>"removed",
        // slot_stripped=>"removed", etc.) as a single configurable filter.
        vec![FormField::Text {
            key: "state_filter",
            label: "Guest state (empty = any)",
            placeholder: "e.g. live",
        }]
    }

    fn condition_display(&self, config: &TriggerConfig) -> String {
        let state = config
            .get("state_filter")
            .and_then(|v| {
                if let Variant::String(s) = v {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .unwrap_or("");
        if state.is_empty() {
            "any state".to_owned()
        } else {
            format!("state = {}", state)
        }
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Twitch),
            kind_prefix: Some("channel.guest_star_guest.update".to_owned()),
        }
    }

    fn matches_trigger(&self, config: &TriggerConfig, event: &Event) -> bool {
        let filter = config
            .get("state_filter")
            .and_then(|v| {
                if let Variant::String(s) = v {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .unwrap_or("");

        if filter.is_empty() {
            return true;
        }

        let event_state = event
            .payload
            .get("guest_star")
            .and_then(|gs| gs.get("state"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        event_state == filter
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let guest_star = event.payload.get("guest_star");
        let guest = event.payload.get("guest");

        let session_id = guest_star
            .and_then(|gs| gs.get("session_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let slot_id = guest_star
            .and_then(|gs| gs.get("slot_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let state = guest_star
            .and_then(|gs| gs.get("state"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let guest_login = guest
            .and_then(|g| g.get("login"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let guest_id = guest
            .and_then(|g| g.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set(
                "guest_star.session_id".to_owned(),
                Variant::String(session_id),
            )
            .set("guest_star.slot_id".to_owned(), Variant::String(slot_id))
            .set("guest_star.state".to_owned(), Variant::String(state))
            .set("guest.login".to_owned(), Variant::String(guest_login))
            .set("guest.id".to_owned(), Variant::String(guest_id))
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
                        name: "guest_star.slot_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Guest Star slot ID".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "guest_star.state".to_owned(),
                        kind: VariantKind::String,
                        label: "Guest state".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "guest.login".to_owned(),
                        kind: VariantKind::String,
                        label: "Guest login".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                    DeclaredVariable {
                        name: "guest.id".to_owned(),
                        kind: VariantKind::String,
                        label: "Guest ID".to_owned(),
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

    fn update_event(state: &str) -> Event {
        Event::new(
            EventSource::Twitch,
            "channel.guest_star_guest.update",
            serde_json::json!({
                "guest_star": {
                    "session_id": "sess-7",
                    "slot_id": "3",
                    "state": state,
                },
                "guest": {
                    "id": "guest-42",
                    "login": "guest_login",
                    "display_name": "GuestName",
                },
            }),
        )
    }

    fn config_with_filter(filter: &str) -> TriggerConfig {
        let mut cfg = TriggerConfig::new();
        cfg.insert(
            "state_filter".to_owned(),
            Variant::String(filter.to_owned()),
        );
        cfg
    }

    #[test]
    fn event_filter_targets_guest_update_kind_from_twitch() {
        let filter = GuestStarGuestUpdatedDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("channel.guest_star_guest.update")
        );
    }

    #[test]
    fn state_filter_fires_only_on_exact_state_match_empty_meaning_any() {
        let event = update_event("live");
        for (filter, expected) in [("", true), ("live", true), ("removed", false)] {
            let cfg = config_with_filter(filter);
            assert_eq!(
                GuestStarGuestUpdatedDescriptor.matches_trigger(&cfg, &event),
                expected,
                "filter {filter:?} against event state \"live\""
            );
        }
    }

    #[test]
    fn missing_state_filter_config_defaults_to_any_and_fires() {
        let event = update_event("removed");
        let cfg = TriggerConfig::new();
        assert!(GuestStarGuestUpdatedDescriptor.matches_trigger(&cfg, &event));
    }

    #[test]
    fn build_arg_stack_exposes_guest_star_and_guest_chaining_vars() {
        let stack = GuestStarGuestUpdatedDescriptor.build_arg_stack(&update_event("live"));
        assert_eq!(
            stack.get("guest_star.session_id"),
            Some(&Variant::String("sess-7".to_owned()))
        );
        assert_eq!(
            stack.get("guest_star.slot_id"),
            Some(&Variant::String("3".to_owned()))
        );
        assert_eq!(
            stack.get("guest_star.state"),
            Some(&Variant::String("live".to_owned()))
        );
        assert_eq!(
            stack.get("guest.login"),
            Some(&Variant::String("guest_login".to_owned()))
        );
        assert_eq!(
            stack.get("guest.id"),
            Some(&Variant::String("guest-42".to_owned()))
        );
    }
}

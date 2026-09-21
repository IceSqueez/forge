use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{
    ActorRole, ActorSlot, CanonicalVariable, DeclaredVariable, PlatformId, SynthesisHint,
    TriggerConfig, Variant, VariantKind,
};

use super::payload_read::{self, twitch_actor};
use crate::payload_fields::guest_star as guest_star_fields;

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
        "Fires when a Guest Star guest or slot changes state in the session"
    }

    fn search_text(&self) -> &str {
        "twitch guest star guest update state slot invited accepted ready backstage live removed host video audio volume"
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
        // Empty state_filter fires on every state transition.
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
            kind_prefix: Some("twitch.channel.guest_star_guest.update".to_owned()),
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
            .get(guest_star_fields::STATE)
            .and_then(|v| v.as_str())
            .unwrap_or("");

        event_state == filter
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(twitch_actor(ActorRole::Principal), guest_identity)
                .event_specific(
                    DeclaredVariable {
                        name: "guest_star.session_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Guest Star session ID".to_owned(),
                        synthesis: None,
                    },
                    |event| {
                        Variant::String(payload_read::text(
                            event,
                            guest_star_fields::SESSION_ID_FIELD,
                        ))
                    },
                )
                .event_specific(
                    DeclaredVariable {
                        name: "guest_star.slot_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Guest Star slot ID".to_owned(),
                        synthesis: None,
                    },
                    |event| {
                        Variant::String(payload_read::text(event, guest_star_fields::SLOT_ID_FIELD))
                    },
                )
                .event_specific(
                    DeclaredVariable {
                        name: "guest_star.state".to_owned(),
                        kind: VariantKind::String,
                        label: "Guest state".to_owned(),
                        synthesis: None,
                    },
                    |event| Variant::String(payload_read::text(event, guest_star_fields::STATE)),
                )
                .event_specific(
                    DeclaredVariable {
                        name: "host.video_enabled".to_owned(),
                        kind: VariantKind::Bool,
                        label: "Host video enabled".to_owned(),
                        synthesis: None,
                    },
                    |event| {
                        Variant::Bool(payload_read::nested_flag(
                            event,
                            guest_star_fields::HOST,
                            guest_star_fields::HOST_VIDEO_ENABLED,
                        ))
                    },
                )
                .event_specific(
                    DeclaredVariable {
                        name: "host.audio_enabled".to_owned(),
                        kind: VariantKind::Bool,
                        label: "Host audio enabled".to_owned(),
                        synthesis: None,
                    },
                    |event| {
                        Variant::Bool(payload_read::nested_flag(
                            event,
                            guest_star_fields::HOST,
                            guest_star_fields::HOST_AUDIO_ENABLED,
                        ))
                    },
                )
                .event_specific(
                    DeclaredVariable {
                        name: "host.volume".to_owned(),
                        kind: VariantKind::Int,
                        label: "Host volume".to_owned(),
                        synthesis: Some(SynthesisHint::BoundedInt { min: 0, max: 100 }),
                    },
                    |event| {
                        Variant::Int(payload_read::nested_number(
                            event,
                            guest_star_fields::HOST,
                            guest_star_fields::HOST_VOLUME,
                        ))
                    },
                )
                .legacy(
                    DeclaredVariable {
                        name: "guest.login".to_owned(),
                        kind: VariantKind::String,
                        label: "Guest login".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Login),
                    |event| {
                        Variant::String(payload_read::nested_text(
                            event,
                            guest_star_fields::GUEST,
                            guest_star_fields::GUEST_LOGIN,
                        ))
                    },
                )
                .legacy(
                    DeclaredVariable {
                        name: "guest.id".to_owned(),
                        kind: VariantKind::String,
                        label: "Guest ID".to_owned(),
                        synthesis: None,
                    },
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Id),
                    |event| {
                        Variant::String(payload_read::nested_text(
                            event,
                            guest_star_fields::GUEST,
                            guest_star_fields::GUEST_ID,
                        ))
                    },
                ),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::principal()
    }
}

fn guest_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(
        event.payload.get(guest_star_fields::GUEST),
        guest_star_fields::GUEST_ID,
        guest_star_fields::GUEST_LOGIN,
        guest_star_fields::GUEST_DISPLAY_NAME,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn update_event(state: &str) -> Event {
        Event::new(
            EventSource::Twitch,
            "twitch.channel.guest_star_guest.update",
            serde_json::json!({
                "session_id": "sess-7",
                "slot_id": "3",
                "state": state,
                "guest": {
                    "id": "guest-42",
                    "login": "guest_login",
                    "display_name": "GuestName",
                },
                "host": {
                    "video_enabled": true,
                    "audio_enabled": false,
                    "volume": 80,
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
            Some("twitch.channel.guest_star_guest.update")
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

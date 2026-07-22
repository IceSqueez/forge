use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

pub(crate) struct GuestStarSlotUpdatedDescriptor;

impl TriggerKindDescriptor for GuestStarSlotUpdatedDescriptor {
    fn id(&self) -> &str {
        "twitch.guest_star.slot_updated"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Streams
    }

    fn label(&self) -> &str {
        "Guest Star slot updated"
    }

    fn summary(&self) -> &str {
        "Fires when a Guest Star slot changes (host video/audio toggle, volume)"
    }

    fn search_text(&self) -> &str {
        "twitch guest star slot update host video audio volume"
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
        "any slot".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Twitch),
            kind_prefix: Some("channel.guest_star_slot.update".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let slot = event.payload.get("slot");

        let session_id = event
            .payload
            .get("session_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let slot_id = slot
            .and_then(|s| s.get("slot_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let host_video_enabled = slot
            .and_then(|s| s.get("host_video_enabled"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let host_audio_enabled = slot
            .and_then(|s| s.get("host_audio_enabled"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let volume = slot
            .and_then(|s| s.get("volume"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        ArgStack::new()
            .set("session.id".to_owned(), Variant::String(session_id))
            .set("slot.id".to_owned(), Variant::String(slot_id))
            .set(
                "slot.host_video_enabled".to_owned(),
                Variant::Bool(host_video_enabled),
            )
            .set(
                "slot.host_audio_enabled".to_owned(),
                Variant::Bool(host_audio_enabled),
            )
            .set("slot.volume".to_owned(), Variant::Int(volume))
    }
    fn output_schema(&self) -> Option<VariableSchema> {
        Some({
            VariableSchema {
                variables: vec![
                    DeclaredVariable {
                        name: "session.id".to_owned(),
                        kind: VariantKind::String,
                        label: "Guest Star session ID".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "slot.id".to_owned(),
                        kind: VariantKind::String,
                        label: "Slot ID".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "slot.host_video_enabled".to_owned(),
                        kind: VariantKind::Bool,
                        label: "Host video enabled".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "slot.host_audio_enabled".to_owned(),
                        kind: VariantKind::Bool,
                        label: "Host audio enabled".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "slot.volume".to_owned(),
                        kind: VariantKind::Int,
                        label: "Slot volume".to_owned(),
                        synthesis: Some(SynthesisHint::BoundedInt { min: 0, max: 100 }),
                    },
                ],
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot_event() -> Event {
        Event::new(
            EventSource::Twitch,
            "channel.guest_star_slot.update",
            serde_json::json!({
                "session_id": "sess-77",
                "slot": {
                    "slot_id": "2",
                    "host_video_enabled": true,
                    "host_audio_enabled": false,
                    "volume": 80,
                },
            }),
        )
    }

    #[test]
    fn event_filter_targets_slot_update_kind_from_twitch() {
        let filter = GuestStarSlotUpdatedDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("channel.guest_star_slot.update")
        );
    }

    #[test]
    fn build_arg_stack_reads_session_from_root_and_slot_fields_from_nested_object() {
        let stack = GuestStarSlotUpdatedDescriptor.build_arg_stack(&slot_event());
        assert_eq!(
            stack.get("session.id"),
            Some(&Variant::String("sess-77".to_owned()))
        );
        assert_eq!(stack.get("slot.id"), Some(&Variant::String("2".to_owned())));
        assert_eq!(
            stack.get("slot.host_video_enabled"),
            Some(&Variant::Bool(true))
        );
        assert_eq!(
            stack.get("slot.host_audio_enabled"),
            Some(&Variant::Bool(false))
        );
    }

    #[test]
    fn build_arg_stack_marshals_integer_volume_as_int_variant() {
        let stack = GuestStarSlotUpdatedDescriptor.build_arg_stack(&slot_event());
        assert_eq!(stack.get("slot.volume"), Some(&Variant::Int(80)));
    }

    #[test]
    fn build_arg_stack_degrades_fractional_volume_to_zero_without_panicking() {
        let event = Event::new(
            EventSource::Twitch,
            "channel.guest_star_slot.update",
            serde_json::json!({
                "session_id": "sess-9",
                "slot": { "slot_id": "1", "volume": 42.5 },
            }),
        );
        let stack = GuestStarSlotUpdatedDescriptor.build_arg_stack(&event);
        assert_eq!(stack.get("slot.volume"), Some(&Variant::Int(0)));
    }

    #[test]
    fn build_arg_stack_degrades_non_numeric_volume_to_zero_without_panicking() {
        let event = Event::new(
            EventSource::Twitch,
            "channel.guest_star_slot.update",
            serde_json::json!({
                "session_id": "sess-9",
                "slot": { "slot_id": "1", "volume": "loud" },
            }),
        );
        let stack = GuestStarSlotUpdatedDescriptor.build_arg_stack(&event);
        assert_eq!(stack.get("slot.volume"), Some(&Variant::Int(0)));
    }

    #[test]
    fn build_arg_stack_on_empty_payload_yields_safe_defaults() {
        let event = Event::new(
            EventSource::Twitch,
            "channel.guest_star_slot.update",
            serde_json::json!({}),
        );
        let stack = GuestStarSlotUpdatedDescriptor.build_arg_stack(&event);
        assert_eq!(
            stack.get("session.id"),
            Some(&Variant::String(String::new()))
        );
        assert_eq!(stack.get("slot.id"), Some(&Variant::String(String::new())));
        assert_eq!(
            stack.get("slot.host_video_enabled"),
            Some(&Variant::Bool(false))
        );
        assert_eq!(
            stack.get("slot.host_audio_enabled"),
            Some(&Variant::Bool(false))
        );
        assert_eq!(stack.get("slot.volume"), Some(&Variant::Int(0)));
    }
}

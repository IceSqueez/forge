use super::audio_source;
use super::families;
use forge_events::EventSource;
use forge_registry::{EventTriggerSpec, TriggerCondition, TriggerMatcher};

use crate::vocabulary::{event_kinds, trigger_ids};

pub(crate) const AUDIO_SOURCE_MUTE_CHANGED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::AUDIO_SOURCE_MUTE_CHANGED,
    source: EventSource::Obs,
    label: "OBS audio source mute changed",
    summary: "Fires when an OBS input source is muted or unmuted.",
    search_text: "obs audio mute unmute source input microphone",
    icon: "microphone-off",
    prefix: event_kinds::AUDIO_PREFIX,
    matcher: TriggerMatcher::KindWhere(
        event_kinds::AUDIO_SOURCE_MUTE_CHANGED,
        audio_source::source_name_matches,
    ),
    condition: TriggerCondition::FromConfig(audio_source::condition_display),
    config_fields: audio_source::config_fields,
    variables: families::source_mute_variables,
};

#[cfg(test)]
mod tests {
    use super::super::audio_source_balance_changed::AUDIO_SOURCE_BALANCE_CHANGED;
    use super::super::audio_source_mute_changed::AUDIO_SOURCE_MUTE_CHANGED;
    use super::super::audio_source_sync_offset_changed::AUDIO_SOURCE_SYNC_OFFSET_CHANGED;
    use super::super::audio_source_volume_changed::AUDIO_SOURCE_VOLUME_CHANGED;
    use forge_events::Event;
    use forge_events::EventSource;
    use forge_registry::TriggerKindDescriptor;
    use forge_types::Variant;
    use serde_json::json;
    use std::collections::BTreeMap;

    const ALL_AUDIO_KINDS: [&str; 4] = [
        "obs.audio.source_mute_changed",
        "obs.audio.source_volume_changed",
        "obs.audio.source_balance_changed",
        "obs.audio.source_sync_offset_changed",
    ];

    fn audio_event(kind: &str, source: &str) -> Event {
        Event::new(EventSource::Obs, kind, json!({ "source_name": source }))
    }

    #[test]
    fn each_audio_descriptor_matches_only_its_own_kind() {
        let cfg = BTreeMap::new();
        let descriptors: [(&str, &dyn TriggerKindDescriptor); 4] = [
            ("obs.audio.source_mute_changed", &AUDIO_SOURCE_MUTE_CHANGED),
            (
                "obs.audio.source_volume_changed",
                &AUDIO_SOURCE_VOLUME_CHANGED,
            ),
            (
                "obs.audio.source_balance_changed",
                &AUDIO_SOURCE_BALANCE_CHANGED,
            ),
            (
                "obs.audio.source_sync_offset_changed",
                &AUDIO_SOURCE_SYNC_OFFSET_CHANGED,
            ),
        ];
        for (own_kind, descriptor) in descriptors {
            for kind in ALL_AUDIO_KINDS {
                assert_eq!(
                    descriptor.matches_trigger(&cfg, &audio_event(kind, "Mic")),
                    kind == own_kind,
                    "descriptor for {own_kind} given {kind}",
                );
            }
        }
    }

    #[test]
    fn audio_descriptor_rejects_non_audio_kind() {
        let event = Event::new(EventSource::Obs, "obs.scene.changed", json!({}));
        assert!(!AUDIO_SOURCE_MUTE_CHANGED.matches_trigger(&BTreeMap::new(), &event));
    }

    #[test]
    fn empty_filter_matches_any_source() {
        let cfg = BTreeMap::new();
        assert!(AUDIO_SOURCE_MUTE_CHANGED.matches_trigger(
            &cfg,
            &audio_event("obs.audio.source_mute_changed", "Desktop")
        ));
    }

    #[test]
    fn empty_string_filter_matches_any_source() {
        let mut cfg = BTreeMap::new();
        cfg.insert("source_name".to_owned(), Variant::String(String::new()));
        assert!(
            AUDIO_SOURCE_MUTE_CHANGED
                .matches_trigger(&cfg, &audio_event("obs.audio.source_mute_changed", "Mic"))
        );
    }

    #[test]
    fn configured_filter_matches_when_source_name_equals() {
        let mut cfg = BTreeMap::new();
        cfg.insert("source_name".to_owned(), Variant::String("Mic".to_owned()));
        assert!(
            AUDIO_SOURCE_MUTE_CHANGED
                .matches_trigger(&cfg, &audio_event("obs.audio.source_mute_changed", "Mic"))
        );
    }

    #[test]
    fn configured_filter_rejects_when_source_name_differs() {
        let mut cfg = BTreeMap::new();
        cfg.insert("source_name".to_owned(), Variant::String("Mic".to_owned()));
        assert!(!AUDIO_SOURCE_MUTE_CHANGED.matches_trigger(
            &cfg,
            &audio_event("obs.audio.source_mute_changed", "Desktop")
        ));
    }

    #[test]
    fn configured_filter_rejects_when_event_has_no_source_name() {
        let mut cfg = BTreeMap::new();
        cfg.insert("source_name".to_owned(), Variant::String("Mic".to_owned()));
        let event = Event::new(EventSource::Obs, "obs.audio.source_mute_changed", json!({}));
        assert!(!AUDIO_SOURCE_MUTE_CHANGED.matches_trigger(&cfg, &event));
    }

    #[test]
    fn mute_arg_stack_extracts_source_name_and_muted_flag() {
        let event = Event::new(
            EventSource::Obs,
            "obs.audio.source_mute_changed",
            json!({ "source_name": "Mic", "is_muted": true }),
        );
        let stack = AUDIO_SOURCE_MUTE_CHANGED.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.source.name"),
            Some(&Variant::String("Mic".to_owned())),
        );
        assert_eq!(stack.get("obs.source.is_muted"), Some(&Variant::Bool(true)));
    }

    #[test]
    fn mute_arg_stack_omits_keys_when_payload_fields_absent() {
        let event = Event::new(EventSource::Obs, "obs.audio.source_mute_changed", json!({}));
        let stack = AUDIO_SOURCE_MUTE_CHANGED.build_arg_stack(&event);
        assert!(stack.get("obs.source.name").is_none());
        assert!(stack.get("obs.source.is_muted").is_none());
    }
}

use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, TriggerConfig, VariableSchema, Variant, VariantKind,
};

pub struct AudioSourceMuteChangedDescriptor;

impl TriggerKindDescriptor for AudioSourceMuteChangedDescriptor {
    fn id(&self) -> &str {
        "obs.audio.source_mute_changed"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS audio source mute changed"
    }

    fn summary(&self) -> &str {
        "Fires when an OBS input source is muted or unmuted."
    }

    fn search_text(&self) -> &str {
        "obs audio mute unmute source input microphone"
    }

    fn icon_name(&self) -> &str {
        "microphone-off"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::Universal
    }

    fn default_config(&self) -> TriggerConfig {
        BTreeMap::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Optional {
            key: "source_name",
            label: "Source name (leave empty to match any)",
            inner: Box::new(FormField::DynamicSelect {
                key: "source_name",
                label: "Source name",
                options_key: "obs.audio_input_names",
            }),
        }]
    }

    fn condition_display(&self, config: &TriggerConfig) -> String {
        match config.get("source_name") {
            Some(Variant::String(s)) if !s.is_empty() => format!("source = {s}"),
            _ => "any source".to_owned(),
        }
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some("audio.".to_owned()),
        }
    }

    fn matches_trigger(&self, config: &TriggerConfig, event: &Event) -> bool {
        if event.kind != "audio.source_mute_changed" {
            return false;
        }
        source_name_matches(config, event)
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        build_mute_arg_stack(event)
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![
                DeclaredVariable {
                    name: "obs.source.name".to_owned(),
                    kind: VariantKind::String,
                    label: "Source name".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "obs.source.is_muted".to_owned(),
                    kind: VariantKind::Bool,
                    label: "Source muted".to_owned(),
                    synthesis: None,
                },
            ],
        })
    }
}

pub(crate) fn build_mute_arg_stack(event: &Event) -> ArgStack {
    let mut stack = ArgStack::new();
    if let Some(name) = event.payload.get("source_name").and_then(|v| v.as_str()) {
        stack = stack.set(
            "obs.source.name".to_owned(),
            Variant::String(name.to_owned()),
        );
    }
    if let Some(muted) = event.payload.get("is_muted").and_then(|v| v.as_bool()) {
        stack = stack.set("obs.source.is_muted".to_owned(), Variant::Bool(muted));
    }
    stack
}

pub(crate) fn source_name_matches(config: &TriggerConfig, event: &Event) -> bool {
    match config.get("source_name") {
        Some(Variant::String(s)) if !s.is_empty() => {
            event.payload.get("source_name").and_then(|v| v.as_str()) == Some(s.as_str())
        }
        _ => true,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    // The four audio-source descriptors share the `TriggerKindDescriptor` shape and the
    // `source_name_matches` filter helper. The 1:1 kind discrimination of all four and the
    // load-bearing filter logic are tested together here; each sibling file tests only its
    // own typed arg-stack extraction (not re-testing the shared filter).
    use super::super::{
        AudioSourceBalanceChangedDescriptor, AudioSourceMuteChangedDescriptor,
        AudioSourceSyncOffsetChangedDescriptor, AudioSourceVolumeChangedDescriptor,
    };
    use super::*;
    use forge_registry::TriggerKindDescriptor;
    use serde_json::json;

    const ALL_AUDIO_KINDS: [&str; 4] = [
        "audio.source_mute_changed",
        "audio.source_volume_changed",
        "audio.source_balance_changed",
        "audio.source_sync_offset_changed",
    ];

    fn audio_event(kind: &str, source: &str) -> Event {
        Event::new(EventSource::Obs, kind, json!({ "source_name": source }))
    }

    /// Each audio descriptor must fire on exactly its own kind and reject the other three.
    /// A descriptor matching a sibling kind would mis-fire user actions on the wrong event.
    #[test]
    fn each_audio_descriptor_matches_only_its_own_kind() {
        let cfg = BTreeMap::new();
        let descriptors: [(&str, &dyn TriggerKindDescriptor); 4] = [
            (
                "audio.source_mute_changed",
                &AudioSourceMuteChangedDescriptor,
            ),
            (
                "audio.source_volume_changed",
                &AudioSourceVolumeChangedDescriptor,
            ),
            (
                "audio.source_balance_changed",
                &AudioSourceBalanceChangedDescriptor,
            ),
            (
                "audio.source_sync_offset_changed",
                &AudioSourceSyncOffsetChangedDescriptor,
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

    /// A non-audio kind sharing the `audio.`-adjacent event source must never match.
    #[test]
    fn audio_descriptor_rejects_non_audio_kind() {
        let event = Event::new(EventSource::Obs, "scene.changed", json!({}));
        assert!(!AudioSourceMuteChangedDescriptor.matches_trigger(&BTreeMap::new(), &event));
    }

    /// Empty filter (no `source_name` configured) matches any source - the default.
    #[test]
    fn empty_filter_matches_any_source() {
        let cfg = BTreeMap::new();
        assert!(
            AudioSourceMuteChangedDescriptor
                .matches_trigger(&cfg, &audio_event("audio.source_mute_changed", "Desktop"))
        );
    }

    /// An empty-string filter value is treated as "any source", not as a literal match
    /// against an empty source name.
    #[test]
    fn empty_string_filter_matches_any_source() {
        let mut cfg = BTreeMap::new();
        cfg.insert("source_name".to_owned(), Variant::String(String::new()));
        assert!(
            AudioSourceMuteChangedDescriptor
                .matches_trigger(&cfg, &audio_event("audio.source_mute_changed", "Mic"))
        );
    }

    /// A configured filter matches when the event's source name equals it.
    #[test]
    fn configured_filter_matches_when_source_name_equals() {
        let mut cfg = BTreeMap::new();
        cfg.insert("source_name".to_owned(), Variant::String("Mic".to_owned()));
        assert!(
            AudioSourceMuteChangedDescriptor
                .matches_trigger(&cfg, &audio_event("audio.source_mute_changed", "Mic"))
        );
    }

    /// A configured filter rejects an event whose source name differs.
    #[test]
    fn configured_filter_rejects_when_source_name_differs() {
        let mut cfg = BTreeMap::new();
        cfg.insert("source_name".to_owned(), Variant::String("Mic".to_owned()));
        assert!(
            !AudioSourceMuteChangedDescriptor
                .matches_trigger(&cfg, &audio_event("audio.source_mute_changed", "Desktop"))
        );
    }

    /// A configured filter rejects an event that carries no source name at all (can't
    /// confirm the source, so the specific filter must not fire).
    #[test]
    fn configured_filter_rejects_when_event_has_no_source_name() {
        let mut cfg = BTreeMap::new();
        cfg.insert("source_name".to_owned(), Variant::String("Mic".to_owned()));
        let event = Event::new(EventSource::Obs, "audio.source_mute_changed", json!({}));
        assert!(!AudioSourceMuteChangedDescriptor.matches_trigger(&cfg, &event));
    }

    #[test]
    fn mute_arg_stack_extracts_source_name_and_muted_flag() {
        let event = Event::new(
            EventSource::Obs,
            "audio.source_mute_changed",
            json!({ "source_name": "Mic", "is_muted": true }),
        );
        let stack = build_mute_arg_stack(&event);
        assert_eq!(
            stack.get("obs.source.name"),
            Some(&Variant::String("Mic".to_owned())),
        );
        assert_eq!(stack.get("obs.source.is_muted"), Some(&Variant::Bool(true)));
    }

    #[test]
    fn mute_arg_stack_omits_keys_when_payload_fields_absent() {
        let event = Event::new(EventSource::Obs, "audio.source_mute_changed", json!({}));
        let stack = build_mute_arg_stack(&event);
        assert!(stack.get("obs.source.name").is_none());
        assert!(stack.get("obs.source.is_muted").is_none());
    }
}

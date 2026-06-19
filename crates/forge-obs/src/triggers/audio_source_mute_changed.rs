use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, TriggerConfig, Variant};

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

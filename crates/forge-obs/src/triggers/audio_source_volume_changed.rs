use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, TriggerConfig, Variant};

use super::audio_source_mute_changed::source_name_matches;

pub struct AudioSourceVolumeChangedDescriptor;

impl TriggerKindDescriptor for AudioSourceVolumeChangedDescriptor {
    fn id(&self) -> &str {
        "obs.audio.source_volume_changed"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS audio source volume changed"
    }

    fn summary(&self) -> &str {
        "Fires when the volume level of an OBS input source changes."
    }

    fn search_text(&self) -> &str {
        "obs audio volume db decibel source input level"
    }

    fn icon_name(&self) -> &str {
        "volume"
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
        if event.kind != "audio.source_volume_changed" {
            return false;
        }
        source_name_matches(config, event)
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(name) = event.payload.get("source_name").and_then(|v| v.as_str()) {
            stack = stack.set(
                "obs.source.name".to_owned(),
                Variant::String(name.to_owned()),
            );
        }
        if let Some(db) = event.payload.get("volume_db").and_then(|v| v.as_f64()) {
            stack = stack.set("obs.source.volume_db".to_owned(), Variant::Float(db));
        }
        if let Some(mul) = event
            .payload
            .get("volume_multiplier")
            .and_then(|v| v.as_f64())
        {
            stack = stack.set(
                "obs.source.volume_multiplier".to_owned(),
                Variant::Float(mul),
            );
        }
        stack
    }
}

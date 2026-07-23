use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, TriggerConfig, VariableSchema, Variant, VariantKind,
};

use super::audio_source_mute_changed::source_name_matches;
use crate::payload_fields::audio as fields;

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
            kind_prefix: Some("obs.audio.".to_owned()),
        }
    }

    fn matches_trigger(&self, config: &TriggerConfig, event: &Event) -> bool {
        if event.kind != "obs.audio.source_volume_changed" {
            return false;
        }
        source_name_matches(config, event)
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(name) = event
            .payload
            .get(fields::SOURCE_NAME)
            .and_then(|v| v.as_str())
        {
            stack = stack.set(
                "obs.source.name".to_owned(),
                Variant::String(name.to_owned()),
            );
        }
        if let Some(db) = event
            .payload
            .get(fields::VOLUME_DB)
            .and_then(|v| v.as_f64())
        {
            stack = stack.set("obs.source.volume_db".to_owned(), Variant::Float(db));
        }
        if let Some(mul) = event
            .payload
            .get(fields::VOLUME_MULTIPLIER)
            .and_then(|v| v.as_f64())
        {
            stack = stack.set(
                "obs.source.volume_multiplier".to_owned(),
                Variant::Float(mul),
            );
        }
        stack
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
                    name: "obs.source.volume_db".to_owned(),
                    kind: VariantKind::Float,
                    label: "Volume (dB)".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "obs.source.volume_multiplier".to_owned(),
                    kind: VariantKind::Float,
                    label: "Volume multiplier".to_owned(),
                    synthesis: None,
                },
            ],
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_registry::TriggerKindDescriptor;
    use serde_json::json;

    #[test]
    fn volume_arg_stack_extracts_name_db_and_multiplier_as_floats() {
        let event = Event::new(
            EventSource::Obs,
            "obs.audio.source_volume_changed",
            json!({ "source_name": "Mic", "volume_db": -12.5, "volume_multiplier": 0.25 }),
        );
        let stack = AudioSourceVolumeChangedDescriptor.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.source.name"),
            Some(&Variant::String("Mic".to_owned())),
        );
        assert_eq!(
            stack.get("obs.source.volume_db"),
            Some(&Variant::Float(-12.5)),
        );
        assert_eq!(
            stack.get("obs.source.volume_multiplier"),
            Some(&Variant::Float(0.25)),
        );
    }

    #[test]
    fn volume_arg_stack_omits_keys_when_payload_fields_absent() {
        let event = Event::new(
            EventSource::Obs,
            "obs.audio.source_volume_changed",
            json!({}),
        );
        let stack = AudioSourceVolumeChangedDescriptor.build_arg_stack(&event);
        assert!(stack.get("obs.source.name").is_none());
        assert!(stack.get("obs.source.volume_db").is_none());
        assert!(stack.get("obs.source.volume_multiplier").is_none());
    }
}

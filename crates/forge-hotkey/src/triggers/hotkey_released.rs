use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, TriggerConfig, VariableSchema, Variant, VariantKind,
};

use crate::payload_fields as fields;

pub struct HotkeyReleasedDescriptor;

impl TriggerKindDescriptor for HotkeyReleasedDescriptor {
    fn id(&self) -> &str {
        "hotkey.global.released"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Hotkey
    }

    fn label(&self) -> &str {
        "Hotkey Released"
    }

    fn summary(&self) -> &str {
        "Fires when a held global hotkey combo is released."
    }

    fn search_text(&self) -> &str {
        "hotkey global shortcut combo keyboard released hold push to talk"
    }

    fn icon_name(&self) -> &str {
        "keyboard"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::Universal
    }

    fn default_config(&self) -> TriggerConfig {
        BTreeMap::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Text {
            key: "combo",
            label: "Hotkey combo",
            placeholder: "e.g. Ctrl+Shift+1",
        }]
    }

    fn condition_display(&self, config: &TriggerConfig) -> String {
        config
            .get("combo")
            .and_then(|v| {
                if let Variant::String(s) = v {
                    Some(s.clone())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "any hotkey".to_owned())
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Hotkey),
            kind_prefix: Some("hotkey.".to_owned()),
        }
    }

    fn matches_trigger(&self, config: &TriggerConfig, event: &Event) -> bool {
        if event.kind != "hotkey.global.released" {
            return false;
        }
        if event.source != EventSource::Hotkey {
            return false;
        }
        if let Some(Variant::String(combo)) = config.get("combo") {
            let event_combo = event
                .payload
                .get(fields::COMBO)
                .and_then(|v| v.as_str())
                .unwrap_or("");
            return combo == event_combo;
        }
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(combo) = event.payload.get(fields::COMBO).and_then(|v| v.as_str()) {
            stack = stack.set("hotkey.combo".to_owned(), Variant::String(combo.to_owned()));
        }
        if let Some(id) = event.payload.get(fields::ID).and_then(|v| v.as_u64()) {
            stack = stack.set("hotkey.id".to_owned(), Variant::Int(id as i64));
        }
        if let Some(ts) = event
            .payload
            .get(fields::TIMESTAMP_US)
            .and_then(|v| v.as_u64())
        {
            stack = stack.set("hotkey.timestamp_us".to_owned(), Variant::Int(ts as i64));
        }
        if let Some(hold_ms) = event.payload.get(fields::HOLD_MS).and_then(|v| v.as_u64()) {
            stack = stack.set("hotkey.hold_ms".to_owned(), Variant::Int(hold_ms as i64));
        }
        stack
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![
                DeclaredVariable {
                    name: "hotkey.combo".to_owned(),
                    kind: VariantKind::String,
                    label: "Hotkey combo".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "hotkey.id".to_owned(),
                    kind: VariantKind::Int,
                    label: "Hotkey ID".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "hotkey.timestamp_us".to_owned(),
                    kind: VariantKind::Int,
                    label: "Timestamp (microseconds)".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "hotkey.hold_ms".to_owned(),
                    kind: VariantKind::Int,
                    label: "Held for (milliseconds)".to_owned(),
                    synthesis: None,
                },
            ],
        })
    }
}


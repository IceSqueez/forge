use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

use super::chat_arg_stack::{base_chat_args, base_chat_schema};
use crate::payload_fields::chat as fields;

pub(crate) struct ChatCheerMessageDescriptor;

impl TriggerKindDescriptor for ChatCheerMessageDescriptor {
    fn id(&self) -> &str {
        "twitch.chat.cheer_message"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Bits
    }

    fn label(&self) -> &str {
        "Cheer message"
    }

    fn summary(&self) -> &str {
        "Fires when a chat message contains a bits cheer"
    }

    fn search_text(&self) -> &str {
        "twitch cheer bits chat message min max"
    }

    fn icon_name(&self) -> &str {
        "diamond"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::Twitch)
    }

    fn default_config(&self) -> TriggerConfig {
        let mut cfg = TriggerConfig::new();
        cfg.insert("min_bits".to_owned(), Variant::Int(1));
        cfg.insert("max_bits".to_owned(), Variant::Int(-1));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Integer {
                key: "min_bits",
                label: "Minimum bits",
                min: 1,
                max: i64::MAX,
            },
            FormField::Integer {
                key: "max_bits",
                label: "Maximum bits (-1 = unlimited)",
                min: -1,
                max: i64::MAX,
            },
        ]
    }

    fn condition_display(&self, config: &TriggerConfig) -> String {
        let min = config
            .get("min_bits")
            .and_then(|v| {
                if let Variant::Int(n) = v {
                    Some(*n)
                } else {
                    None
                }
            })
            .unwrap_or(1);
        let max = config
            .get("max_bits")
            .and_then(|v| {
                if let Variant::Int(n) = v {
                    Some(*n)
                } else {
                    None
                }
            })
            .unwrap_or(-1);
        if max < 0 {
            format!(">= {} bits", min)
        } else {
            format!("{}-{} bits", min, max)
        }
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Twitch),
            kind_prefix: Some("twitch.channel.chat.message".to_owned()),
        }
    }

    fn matches_trigger(&self, config: &TriggerConfig, event: &Event) -> bool {
        let bits = match event
            .payload
            .get(fields::CHEER)
            .and_then(|c| c.get(fields::CHEER_BITS))
            .and_then(|v| v.as_i64())
        {
            Some(b) => b,
            None => return false,
        };

        let min = config
            .get("min_bits")
            .and_then(|v| {
                if let Variant::Int(n) = v {
                    Some(*n)
                } else {
                    None
                }
            })
            .unwrap_or(1);
        let max = config
            .get("max_bits")
            .and_then(|v| {
                if let Variant::Int(n) = v {
                    Some(*n)
                } else {
                    None
                }
            })
            .unwrap_or(-1);

        if bits < min {
            return false;
        }

        if max >= 0 && bits > max {
            return false;
        }

        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let bits = event
            .payload
            .get(fields::CHEER)
            .and_then(|c| c.get(fields::CHEER_BITS))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        base_chat_args(event).set("cheer.bits".to_owned(), Variant::Int(bits))
    }
    fn output_schema(&self) -> Option<VariableSchema> {
        let mut schema = base_chat_schema();
        schema.variables.push(DeclaredVariable {
            name: "cheer.bits".to_owned(),
            kind: VariantKind::Int,
            label: "Bits cheered".to_owned(),
            synthesis: Some(SynthesisHint::BoundedInt { min: 1, max: 10000 }),
        });
        Some(schema)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cheer_config(min_bits: i64, max_bits: i64) -> TriggerConfig {
        let mut cfg = TriggerConfig::new();
        cfg.insert("min_bits".to_owned(), Variant::Int(min_bits));
        cfg.insert("max_bits".to_owned(), Variant::Int(max_bits));
        cfg
    }

    fn chat_event(cheer_bits: Option<i64>) -> Event {
        let mut payload = serde_json::json!({
            "channel": "streamer",
            "user": { "login": "viewer", "id": "123", "roles": [] },
            "message": "cheer100 hi",
            "badges": [],
            "color": "#FF0000"
        });
        if let Some(bits) = cheer_bits {
            payload["cheer"] = serde_json::json!({ "bits": bits });
        }
        Event::new(EventSource::Twitch, "twitch.channel.chat.message", payload)
    }

    #[test]
    fn matches_trigger_filters_on_bits_range() {
        let cases = [
            ("bits within open range", Some(100), (1, -1), true),
            ("no cheer object", None, (1, -1), false),
            ("bits one below min", Some(99), (100, -1), false),
            ("bits at min boundary", Some(100), (100, -1), true),
            ("bits at max boundary", Some(500), (1, 500), true),
            ("bits one above max", Some(501), (1, 500), false),
            ("max -1 lifts upper bound", Some(1_000_000), (1, -1), true),
        ];
        for (name, cheer, (min, max), expected) in cases {
            let cfg = cheer_config(min, max);
            assert_eq!(
                ChatCheerMessageDescriptor.matches_trigger(&cfg, &chat_event(cheer)),
                expected,
                "case: {name}"
            );
        }
    }

    #[test]
    fn build_arg_stack_adds_cheer_args_to_base_chat_args() {
        let stack = ChatCheerMessageDescriptor.build_arg_stack(&chat_event(Some(250)));
        assert_eq!(stack.get("cheer.bits"), Some(&Variant::Int(250)));
        assert_eq!(
            stack.get("message_text"),
            Some(&Variant::String("cheer100 hi".to_owned()))
        );
    }
}

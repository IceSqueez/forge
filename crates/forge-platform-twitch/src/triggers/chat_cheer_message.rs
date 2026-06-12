use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

use super::chat_arg_stack::base_chat_args;

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
        "twitch cheer bits chat message anonymous min max"
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
        cfg.insert("anonymous_allowed".to_owned(), Variant::Bool(true));
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
            FormField::Toggle {
                key: "anonymous_allowed",
                label: "Allow anonymous cheers",
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
            kind_prefix: Some("chat.message".to_owned()),
        }
    }

    fn matches_trigger(&self, config: &TriggerConfig, event: &Event) -> bool {
        let bits = match event
            .payload
            .get("cheer")
            .and_then(|c| c.get("bits"))
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
        let anonymous_allowed = config
            .get("anonymous_allowed")
            .and_then(|v| {
                if let Variant::Bool(b) = v {
                    Some(*b)
                } else {
                    None
                }
            })
            .unwrap_or(true);

        let is_anonymous = event
            .payload
            .get("cheer")
            .and_then(|c| c.get("is_anonymous"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if !anonymous_allowed && is_anonymous {
            return false;
        }

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
            .get("cheer")
            .and_then(|c| c.get("bits"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let is_anonymous = event
            .payload
            .get("cheer")
            .and_then(|c| c.get("is_anonymous"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        base_chat_args(event)
            .set("cheer.bits".to_owned(), Variant::Int(bits))
            .set("cheer.is_anonymous".to_owned(), Variant::Bool(is_anonymous))
    }
}

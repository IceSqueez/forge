use crate::vocabulary::trigger_ids;
use crate::vocabulary::{event_kinds, messages};
use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, CanonicalCount, CanonicalVariable, EventFilter, FormField,
    KindPlatformContract, TriggerCategory, TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{PlatformId, TriggerConfig, UserText, Variant};

use super::chat_arg_stack::base_chat_variables;
use crate::vocabulary::fields::chat as fields;

const NO_MAXIMUM: i64 = 0;

fn ordered_bounds(min: i64, max: i64) -> (i64, i64) {
    (min.min(max), min.max(max))
}

pub(crate) struct ChatCheerMessageDescriptor;

impl TriggerKindDescriptor for ChatCheerMessageDescriptor {
    fn id(&self) -> &str {
        trigger_ids::CHAT_CHEER_MESSAGE
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
        cfg.insert("max_bits".to_owned(), Variant::Int(NO_MAXIMUM));
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
                label: "Maximum bits (0 = no maximum)",
                min: 0,
                max: i64::MAX,
            },
        ]
    }

    fn condition_display(&self, config: &TriggerConfig) -> UserText {
        let int_of = |key: &str| match config.get(key) {
            Some(Variant::Int(n)) => Some(*n),
            _ => None,
        };
        let min = int_of("min_bits").unwrap_or(1);
        match int_of("max_bits") {
            Some(max) if max > NO_MAXIMUM => {
                let (low, high) = ordered_bounds(min, max);
                messages::condition_bits_between(low, high).into()
            }
            _ => messages::condition_bits_at_least(min).into(),
        }
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Twitch),
            kind_prefix: Some(event_kinds::CHANNEL_CHAT_MESSAGE.to_owned()),
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
            .unwrap_or(NO_MAXIMUM);

        if max <= NO_MAXIMUM {
            return bits >= min;
        }
        let (low, high) = ordered_bounds(min, max);
        (low..=high).contains(&bits)
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            base_chat_variables()
                .count(CanonicalCount::BitsAmount, cheered_bits)
                .legacy(
                    "cheer.bits",
                    "Bits cheered",
                    CanonicalVariable::Count(CanonicalCount::BitsAmount),
                ),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::principal()
    }
}

fn cheered_bits(event: &Event) -> i64 {
    event
        .payload
        .get(fields::CHEER)
        .and_then(|cheer| cheer.get(fields::CHEER_BITS))
        .and_then(|value| value.as_i64())
        .unwrap_or_default()
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
            ("max 0 lifts upper bound", Some(1_000_000), (1, 0), true),
            ("max 0 still honours min", Some(99), (100, 0), false),
            ("swapped bounds accept inside", Some(300), (500, 100), true),
            (
                "swapped bounds accept the low end",
                Some(100),
                (500, 100),
                true,
            ),
            (
                "swapped bounds accept the high end",
                Some(500),
                (500, 100),
                true,
            ),
            ("swapped bounds reject below", Some(99), (500, 100), false),
            ("swapped bounds reject above", Some(501), (500, 100), false),
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
    fn a_cheer_message_publishes_its_bits_as_the_canonical_bits_amount() {
        let stack = ChatCheerMessageDescriptor.build_arg_stack(&chat_event(Some(250)));
        assert_eq!(stack.get("bits_amount"), Some(&Variant::Int(250)));
    }
}

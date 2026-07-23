use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

use crate::payload_fields::support as fields;

pub(crate) struct SupportCheerDescriptor;

impl TriggerKindDescriptor for SupportCheerDescriptor {
    fn id(&self) -> &str {
        "twitch.support.cheer"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Bits
    }

    fn label(&self) -> &str {
        "Cheer"
    }

    fn summary(&self) -> &str {
        "Fires on a bits cheer event"
    }

    fn search_text(&self) -> &str {
        "twitch cheer bits donation anonymous"
    }

    fn icon_name(&self) -> &str {
        "diamond"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::Twitch)
    }

    fn default_config(&self) -> TriggerConfig {
        let mut cfg = TriggerConfig::new();
        cfg.insert("min_bits".to_owned(), Variant::Int(0));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Integer {
            key: "min_bits",
            label: "Minimum bits",
            min: 0,
            max: i64::MAX,
        }]
    }

    fn condition_display(&self, config: &TriggerConfig) -> String {
        let min_bits = config
            .get("min_bits")
            .and_then(|v| {
                if let Variant::Int(n) = v {
                    Some(*n)
                } else {
                    None
                }
            })
            .unwrap_or(0);
        format!(">= {} bits", min_bits)
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Twitch),
            kind_prefix: Some("twitch.channel.cheer".to_owned()),
        }
    }

    fn matches_trigger(&self, config: &TriggerConfig, event: &Event) -> bool {
        let min_bits = config
            .get("min_bits")
            .and_then(|v| {
                if let Variant::Int(n) = v {
                    Some(*n)
                } else {
                    None
                }
            })
            .unwrap_or(0);

        let bits = event
            .payload
            .get(fields::BITS)
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        bits >= min_bits
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let bits = event
            .payload
            .get(fields::BITS)
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let message = event
            .payload
            .get(fields::MESSAGE)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let is_anonymous = event
            .payload
            .get(fields::IS_ANONYMOUS)
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let user_login = event
            .payload
            .get(fields::USER)
            .and_then(|u| u.get(fields::USER_LOGIN))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let user_id = event
            .payload
            .get(fields::USER)
            .and_then(|u| u.get(fields::USER_ID))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("bits_amount".to_owned(), Variant::Int(bits))
            .set("cheer_message".to_owned(), Variant::String(message))
            .set("cheer_is_anonymous".to_owned(), Variant::Bool(is_anonymous))
            .set("user_login".to_owned(), Variant::String(user_login))
            .set("user_id".to_owned(), Variant::String(user_id))
    }
    fn output_schema(&self) -> Option<VariableSchema> {
        Some({
            VariableSchema {
                variables: vec![
                    DeclaredVariable {
                        name: "bits_amount".to_owned(),
                        kind: VariantKind::Int,
                        label: "Bits amount".to_owned(),
                        synthesis: Some(SynthesisHint::BoundedInt { min: 1, max: 10000 }),
                    },
                    DeclaredVariable {
                        name: "cheer_message".to_owned(),
                        kind: VariantKind::String,
                        label: "Cheer message".to_owned(),
                        synthesis: Some(SynthesisHint::Message),
                    },
                    DeclaredVariable {
                        name: "cheer_is_anonymous".to_owned(),
                        kind: VariantKind::Bool,
                        label: "Anonymous cheer".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "user_login".to_owned(),
                        kind: VariantKind::String,
                        label: "User login".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                    DeclaredVariable {
                        name: "user_id".to_owned(),
                        kind: VariantKind::String,
                        label: "User ID".to_owned(),
                        synthesis: None,
                    },
                ],
            }
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn make_config(min_bits: i64) -> TriggerConfig {
        let mut config = TriggerConfig::new();
        config.insert("min_bits".to_owned(), Variant::Int(min_bits));
        config
    }

    fn cheer_event(bits: i64) -> Event {
        Event::new(
            EventSource::Twitch,
            "channel.cheer",
            serde_json::json!({
                "bits": bits,
                "message": "PogChamp PogChamp PogChamp",
                "is_anonymous": false,
                "user": { "id": "555", "login": "cheerer", "display_name": "Cheerer" }
            }),
        )
    }

    #[test]
    fn condition_display_shows_bits_threshold() {
        let mut cfg = TriggerConfig::new();
        cfg.insert("min_bits".to_owned(), Variant::Int(100));
        assert_eq!(
            SupportCheerDescriptor.condition_display(&cfg),
            ">= 100 bits"
        );
    }

    #[test]
    fn matches_when_bits_meet_threshold() {
        let cfg = make_config(100);
        assert!(SupportCheerDescriptor.matches_trigger(&cfg, &cheer_event(100)));
        assert!(SupportCheerDescriptor.matches_trigger(&cfg, &cheer_event(500)));
    }

    #[test]
    fn does_not_match_below_threshold() {
        let cfg = make_config(100);
        assert!(!SupportCheerDescriptor.matches_trigger(&cfg, &cheer_event(50)));
    }

    #[test]
    fn min_bits_zero_always_matches() {
        let cfg = make_config(0);
        assert!(SupportCheerDescriptor.matches_trigger(&cfg, &cheer_event(1)));
    }

    #[test]
    fn build_arg_stack_extracts_cheer_fields() {
        let stack = SupportCheerDescriptor.build_arg_stack(&cheer_event(200));
        assert_eq!(stack.get("bits_amount"), Some(&Variant::Int(200)));
        assert_eq!(stack.get("cheer_is_anonymous"), Some(&Variant::Bool(false)));
        assert_eq!(
            stack.get("user_login"),
            Some(&Variant::String("cheerer".to_owned()))
        );
    }
}

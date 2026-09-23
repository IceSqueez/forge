use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{
    ActorRole, CanonicalCount, CanonicalVariable, DeclaredVariable, PlatformId, SynthesisHint,
    TriggerConfig, Variant, VariantKind,
};

use super::payload_read::{self, twitch_actor};
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

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(twitch_actor(ActorRole::Principal), cheerer_identity)
                .message_text(cheer_message)
                .count(CanonicalCount::BitsAmount, |event| {
                    payload_read::number(event, fields::BITS)
                })
                .event_specific(
                    DeclaredVariable {
                        name: "cheer_is_anonymous".to_owned(),
                        kind: VariantKind::Bool,
                        label: "Anonymous cheer".to_owned(),
                        synthesis: None,
                    },
                    |event| Variant::Bool(payload_read::flag(event, fields::IS_ANONYMOUS)),
                )
                .legacy(
                    DeclaredVariable {
                        name: "cheer_message".to_owned(),
                        kind: VariantKind::String,
                        label: "Cheer message".to_owned(),
                        synthesis: Some(SynthesisHint::Message),
                    },
                    CanonicalVariable::MessageText,
                    |event| Variant::String(cheer_message(event)),
                ),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::principal()
    }
}

fn cheerer_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(
        event.payload.get(fields::USER),
        fields::USER_ID,
        fields::USER_LOGIN,
        fields::USER_DISPLAY_NAME,
    )
}

fn cheer_message(event: &Event) -> String {
    payload_read::text(event, fields::MESSAGE)
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
    fn a_cheer_publishes_the_canonical_actor_block_the_bits_and_the_cheer_message() {
        let stack = SupportCheerDescriptor.build_arg_stack(&cheer_event(200));
        for (name, value) in [
            ("user_id", "555"),
            ("user_name", "Cheerer"),
            ("user_login", "cheerer"),
            ("user_platform", "twitch"),
            ("message_text", "PogChamp PogChamp PogChamp"),
        ] {
            assert_eq!(
                stack.get(name),
                Some(&Variant::String(value.to_owned())),
                "'{name}'"
            );
        }
        assert_eq!(stack.get("bits_amount"), Some(&Variant::Int(200)));
        assert_eq!(stack.get("cheer_is_anonymous"), Some(&Variant::Bool(false)));
    }

    #[test]
    fn the_legacy_cheer_message_still_carries_the_line_message_text_now_carries() {
        let stack = SupportCheerDescriptor.build_arg_stack(&cheer_event(200));
        assert_eq!(
            stack.get("cheer_message"),
            Some(&Variant::String("PogChamp PogChamp PogChamp".to_owned()))
        );
    }
}

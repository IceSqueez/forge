use super::spec::TwitchTriggerSpec;
use crate::vocabulary::trigger_ids;
use crate::vocabulary::{event_kinds, messages};
use forge_events::Event;
use forge_registry::{
    ActorDeclaration, ActorIdentity, ActorRole, CanonicalCount, CanonicalVariable, FormField,
    SubActionConfigExt, TriggerCategory, TriggerVariables,
};
use forge_types::{DeclaredVariable, TriggerConfig, UserText, Variant, VariantKind};

use super::payload_read::{self, twitch_actor};
use crate::vocabulary::fields::support as fields;

fn default_config() -> TriggerConfig {
    let mut cfg = TriggerConfig::new();
    cfg.insert("min_bits".to_owned(), Variant::Int(0));
    cfg
}

fn config_fields() -> Vec<FormField> {
    vec![FormField::Integer {
        key: "min_bits",
        label: "Minimum bits",
        min: 0,
        max: i64::MAX,
    }]
}

fn condition_display(config: &TriggerConfig) -> UserText {
    let min_bits = match config.get("min_bits") {
        Some(Variant::Int(n)) => *n,
        _ => 0,
    };
    messages::condition_bits_at_least(min_bits).into()
}

fn matches(config: &TriggerConfig, event: &Event) -> bool {
    let min_bits = config.int("min_bits").unwrap_or(0);

    let bits = event
        .payload
        .get(fields::BITS)
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    bits >= min_bits
}

fn variables() -> Option<TriggerVariables> {
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
                    synthesis: None,
                },
                |event| Variant::Bool(payload_read::flag(event, fields::IS_ANONYMOUS)),
            )
            .legacy("cheer_message", CanonicalVariable::MessageText),
    )
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::SUPPORT_CHEER,
    category: TriggerCategory::Bits,
    label: "Cheer",
    summary: "Fires on a bits cheer event",
    search_text: "twitch cheer bits donation anonymous",
    icon: "diamond",
    kind: event_kinds::CHANNEL_CHEER,
    actors: ActorDeclaration::principal(),
    variables,
    matches,
    default_config,
    config_fields,
    condition_display,
    ..TwitchTriggerSpec::BASE
};

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
mod tests {
    use super::*;
    use forge_events::EventSource;
    use forge_registry::TriggerKindDescriptor;

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
            SPEC.condition_display(&cfg),
            messages::condition_bits_at_least(100).into()
        );
    }

    #[test]
    fn matches_when_bits_meet_threshold() {
        let cfg = make_config(100);
        assert!(SPEC.matches_trigger(&cfg, &cheer_event(100)));
        assert!(SPEC.matches_trigger(&cfg, &cheer_event(500)));
    }

    #[test]
    fn does_not_match_below_threshold() {
        let cfg = make_config(100);
        assert!(!SPEC.matches_trigger(&cfg, &cheer_event(50)));
    }

    #[test]
    fn min_bits_zero_always_matches() {
        let cfg = make_config(0);
        assert!(SPEC.matches_trigger(&cfg, &cheer_event(1)));
    }

    #[test]
    fn a_cheer_publishes_the_canonical_actor_block_the_bits_and_the_cheer_message() {
        let stack = SPEC.build_arg_stack(&cheer_event(200));
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
}

use super::spec::TwitchTriggerSpec;
use crate::vocabulary::trigger_ids;
use crate::vocabulary::{event_kinds, messages};
use forge_events::Event;
use forge_registry::ChatEventKind;
use forge_registry::{
    ActorDeclaration, ActorIdentity, ActorRole, ActorSlot, CanonicalVariable, FormField,
    SubActionConfigExt, TriggerCategory, TriggerVariables,
};
use forge_types::{DeclaredVariable, SynthesisHint, TriggerConfig, UserText, Variant, VariantKind};

use super::payload_read::{self, twitch_actor};
use crate::vocabulary::fields::charity as charity_fields;

fn default_config() -> TriggerConfig {
    let mut cfg = TriggerConfig::new();
    cfg.insert("min_amount_cents".to_owned(), Variant::Int(0));
    cfg
}

fn config_fields() -> Vec<FormField> {
    vec![FormField::Integer {
        key: "min_amount_cents",
        label: "Minimum donation (cents)",
        min: 0,
        max: i64::MAX,
    }]
}

fn condition_display(config: &TriggerConfig) -> UserText {
    let min = config.int("min_amount_cents").unwrap_or(0);
    if min == 0 {
        messages::CONDITION_ANY_AMOUNT.into()
    } else {
        messages::condition_amount_at_least(min).into()
    }
}

fn matches(config: &TriggerConfig, event: &Event) -> bool {
    let min_amount_cents = config.int("min_amount_cents").unwrap_or(0);

    let amount_cents = event
        .payload
        .get(charity_fields::CHARITY)
        .and_then(|c| c.get(charity_fields::AMOUNT_CENTS))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    amount_cents >= min_amount_cents
}

fn variables() -> Option<TriggerVariables> {
    Some(
        TriggerVariables::new()
            .actor(twitch_actor(ActorRole::Principal), donor_identity)
            .event_specific(
                DeclaredVariable {
                    name: "charity.id".to_owned(),
                    kind: VariantKind::String,
                    synthesis: None,
                },
                |event| {
                    Variant::String(payload_read::nested_text(
                        event,
                        charity_fields::CHARITY,
                        charity_fields::CHARITY_ID,
                    ))
                },
            )
            .event_specific(
                DeclaredVariable {
                    name: "charity.name".to_owned(),
                    kind: VariantKind::String,
                    synthesis: None,
                },
                |event| {
                    Variant::String(payload_read::nested_text(
                        event,
                        charity_fields::CHARITY,
                        charity_fields::CHARITY_NAME,
                    ))
                },
            )
            .event_specific(
                DeclaredVariable {
                    name: "charity.amount_cents".to_owned(),
                    kind: VariantKind::Int,
                    synthesis: Some(SynthesisHint::BoundedInt {
                        min: 0,
                        max: super::MAX_SYNTHESIZED_COUNT,
                    }),
                },
                |event| {
                    Variant::Int(payload_read::nested_number(
                        event,
                        charity_fields::CHARITY,
                        charity_fields::AMOUNT_CENTS,
                    ))
                },
            )
            .event_specific(
                DeclaredVariable {
                    name: "charity.currency_code".to_owned(),
                    kind: VariantKind::String,
                    synthesis: None,
                },
                |event| {
                    Variant::String(payload_read::nested_text(
                        event,
                        charity_fields::CHARITY,
                        charity_fields::CURRENCY_CODE,
                    ))
                },
            )
            .event_specific(
                DeclaredVariable {
                    name: "charity.decimal_places".to_owned(),
                    kind: VariantKind::Int,
                    synthesis: Some(SynthesisHint::BoundedInt { min: 0, max: 3 }),
                },
                |event| {
                    Variant::Int(payload_read::nested_number(
                        event,
                        charity_fields::CHARITY,
                        charity_fields::DECIMAL_PLACES,
                    ))
                },
            )
            .legacy(
                "charity.user.login",
                CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Login),
            )
            .legacy(
                "charity.user.display_name",
                CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Name),
            ),
    )
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::SUPPORT_CHARITY_DONATION,
    category: TriggerCategory::Charity,
    label: "Charity donation",
    summary: "Fires when a viewer donates to the active charity campaign",
    search_text: "twitch charity donate donation campaign",
    icon: "heart",
    kind: event_kinds::CHANNEL_CHARITY_CAMPAIGN_DONATE,
    actors: ActorDeclaration::principal(),
    variables,
    matches,
    default_config,
    config_fields,
    condition_display,
    chat_event: Some(ChatEventKind::Donation),
    ..TwitchTriggerSpec::BASE
};

fn donor_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(
        event.payload.get(charity_fields::USER),
        charity_fields::USER_ID,
        charity_fields::USER_LOGIN,
        charity_fields::USER_DISPLAY_NAME,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::EventSource;
    use forge_registry::TriggerKindDescriptor;

    fn donation_event(amount_cents: i64) -> Event {
        Event::new(
            EventSource::Twitch,
            "channel.charity_campaign.donate",
            serde_json::json!({
                "charity": {
                    "id": "camp-1",
                    "name": "Helping Hands",
                    "amount_cents": amount_cents,
                    "currency_code": "USD",
                },
                "user": { "login": "giver", "display_name": "Giver" },
            }),
        )
    }

    fn config_min(min_amount_cents: i64) -> TriggerConfig {
        let mut cfg = TriggerConfig::new();
        cfg.insert(
            "min_amount_cents".to_owned(),
            Variant::Int(min_amount_cents),
        );
        cfg
    }

    #[test]
    fn min_amount_cents_filter_compares_at_and_around_boundary() {
        for (min, expected) in [
            (config_min(0), true),
            (config_min(500), true),
            (config_min(501), false),
            (TriggerConfig::new(), true),
        ] {
            assert_eq!(SPEC.matches_trigger(&min, &donation_event(500)), expected,);
        }
    }

    #[test]
    fn build_arg_stack_maps_amount_as_int_and_string_fields() {
        let stack = SPEC.build_arg_stack(&donation_event(2500));
        assert_eq!(stack.get("charity.amount_cents"), Some(&Variant::Int(2500)));
        assert_eq!(
            stack.get("charity.id"),
            Some(&Variant::String("camp-1".to_owned()))
        );
        assert_eq!(
            stack.get("charity.name"),
            Some(&Variant::String("Helping Hands".to_owned()))
        );
        assert_eq!(
            stack.get("charity.currency_code"),
            Some(&Variant::String("USD".to_owned()))
        );
        assert_eq!(
            stack.get("charity.user.login"),
            Some(&Variant::String("giver".to_owned()))
        );
        assert_eq!(
            stack.get("charity.user.display_name"),
            Some(&Variant::String("Giver".to_owned()))
        );
    }

    #[test]
    fn the_donation_stack_carries_the_amount_precision_the_currency_uses() {
        for (decimal_places, currency) in [(0, "JPY"), (2, "USD"), (3, "KWD")] {
            let event = Event::new(
                EventSource::Twitch,
                "channel.charity_campaign.donate",
                serde_json::json!({
                    "charity": { "currency_code": currency, "decimal_places": decimal_places },
                    "user": { "login": "giver", "display_name": "Giver" },
                }),
            );

            let stack = SPEC.build_arg_stack(&event);

            assert_eq!(
                stack.get("charity.decimal_places"),
                Some(&Variant::Int(decimal_places)),
                "{currency}"
            );
        }
    }

    #[test]
    fn a_stored_donation_event_without_a_precision_reads_as_zero_places() {
        let stack = SPEC.build_arg_stack(&donation_event(2500));

        assert_eq!(stack.get("charity.decimal_places"), Some(&Variant::Int(0)));
    }
}

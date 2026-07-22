use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

use crate::payload_fields::charity as charity_fields;

pub(crate) struct CharityDonationDescriptor;

impl TriggerKindDescriptor for CharityDonationDescriptor {
    fn id(&self) -> &str {
        "twitch.support.charity_donation"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Charity
    }

    fn label(&self) -> &str {
        "Charity donation"
    }

    fn summary(&self) -> &str {
        "Fires when a viewer donates to the active charity campaign"
    }

    fn search_text(&self) -> &str {
        "twitch charity donate donation campaign"
    }

    fn icon_name(&self) -> &str {
        "heart"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::Twitch)
    }

    fn default_config(&self) -> TriggerConfig {
        let mut cfg = TriggerConfig::new();
        cfg.insert("min_amount_cents".to_owned(), Variant::Int(0));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Integer {
            key: "min_amount_cents",
            label: "Minimum donation (cents)",
            min: 0,
            max: i64::MAX,
        }]
    }

    fn condition_display(&self, config: &TriggerConfig) -> String {
        let min = config
            .get("min_amount_cents")
            .and_then(|v| {
                if let Variant::Int(n) = v {
                    Some(*n)
                } else {
                    None
                }
            })
            .unwrap_or(0);
        if min == 0 {
            "any amount".to_owned()
        } else {
            format!("amount >= {min} cents")
        }
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Twitch),
            kind_prefix: Some("channel.charity_campaign.donate".to_owned()),
        }
    }

    fn matches_trigger(&self, config: &TriggerConfig, event: &Event) -> bool {
        let min_amount_cents = config
            .get("min_amount_cents")
            .and_then(|v| {
                if let Variant::Int(n) = v {
                    Some(*n)
                } else {
                    None
                }
            })
            .unwrap_or(0);

        let amount_cents = event
            .payload
            .get(charity_fields::CHARITY)
            .and_then(|c| c.get(charity_fields::AMOUNT_CENTS))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        amount_cents >= min_amount_cents
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let charity = event.payload.get(charity_fields::CHARITY);
        let user = event.payload.get(charity_fields::USER);

        let charity_id = charity
            .and_then(|c| c.get(charity_fields::CHARITY_ID))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let charity_name = charity
            .and_then(|c| c.get(charity_fields::CHARITY_NAME))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let amount_cents = charity
            .and_then(|c| c.get(charity_fields::AMOUNT_CENTS))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let currency_code = charity
            .and_then(|c| c.get(charity_fields::CURRENCY_CODE))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let user_login = user
            .and_then(|u| u.get(charity_fields::USER_LOGIN))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let user_display_name = user
            .and_then(|u| u.get(charity_fields::USER_DISPLAY_NAME))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();

        ArgStack::new()
            .set("charity.id".to_owned(), Variant::String(charity_id))
            .set("charity.name".to_owned(), Variant::String(charity_name))
            .set(
                "charity.amount_cents".to_owned(),
                Variant::Int(amount_cents),
            )
            .set(
                "charity.currency_code".to_owned(),
                Variant::String(currency_code),
            )
            .set("charity.user.login".to_owned(), Variant::String(user_login))
            .set(
                "charity.user.display_name".to_owned(),
                Variant::String(user_display_name),
            )
    }
    fn output_schema(&self) -> Option<VariableSchema> {
        Some({
            VariableSchema {
                variables: vec![
                    DeclaredVariable {
                        name: "charity.id".to_owned(),
                        kind: VariantKind::String,
                        label: "Charity campaign ID".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "charity.name".to_owned(),
                        kind: VariantKind::String,
                        label: "Charity name".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "charity.amount_cents".to_owned(),
                        kind: VariantKind::Int,
                        label: "Donation amount (cents)".to_owned(),
                        synthesis: Some(SynthesisHint::BoundedInt {
                            min: 0,
                            max: 1000000,
                        }),
                    },
                    DeclaredVariable {
                        name: "charity.currency_code".to_owned(),
                        kind: VariantKind::String,
                        label: "Currency code".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "charity.user.login".to_owned(),
                        kind: VariantKind::String,
                        label: "Donor login".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                    DeclaredVariable {
                        name: "charity.user.display_name".to_owned(),
                        kind: VariantKind::String,
                        label: "Donor display name".to_owned(),
                        synthesis: Some(SynthesisHint::DisplayName),
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
    fn event_filter_targets_donate_topic_on_twitch_source() {
        let filter = CharityDonationDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("channel.charity_campaign.donate")
        );
    }

    #[test]
    fn min_amount_cents_filter_compares_at_and_around_boundary() {
        for (min, expected) in [
            (config_min(0), true),        // floor accepts any amount
            (config_min(500), true),      // boundary: equal passes (>=)
            (config_min(501), false),     // one over boundary rejects
            (TriggerConfig::new(), true), // missing config falls back to 0
        ] {
            assert_eq!(
                CharityDonationDescriptor.matches_trigger(&min, &donation_event(500)),
                expected,
            );
        }
    }

    #[test]
    fn build_arg_stack_maps_amount_as_int_and_string_fields() {
        let stack = CharityDonationDescriptor.build_arg_stack(&donation_event(2500));
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
}

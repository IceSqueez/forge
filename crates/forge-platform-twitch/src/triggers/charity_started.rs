use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

use crate::vocabulary::fields::charity as charity_fields;

pub(crate) struct CharityStartedDescriptor;

impl TriggerKindDescriptor for CharityStartedDescriptor {
    fn id(&self) -> &str {
        trigger_ids::SUPPORT_CHARITY_STARTED
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Charity
    }

    fn label(&self) -> &str {
        "Charity campaign started"
    }

    fn summary(&self) -> &str {
        "Fires when the broadcaster starts a charity campaign"
    }

    fn search_text(&self) -> &str {
        "twitch charity campaign start begin"
    }

    fn icon_name(&self) -> &str {
        "heart"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::Twitch)
    }

    fn default_config(&self) -> TriggerConfig {
        TriggerConfig::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![]
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Twitch),
            kind_prefix: Some(event_kinds::CHANNEL_CHARITY_CAMPAIGN_START.to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::Actorless
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        build_charity_lifecycle_arg_stack(event)
    }
    fn output_schema(&self) -> Option<VariableSchema> {
        Some(build_charity_lifecycle_schema())
    }
}

pub(super) fn build_charity_lifecycle_arg_stack(event: &Event) -> ArgStack {
    let charity = event.payload.get(charity_fields::CHARITY);

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
    let current_amount_cents = charity
        .and_then(|c| c.get(charity_fields::CURRENT_AMOUNT_CENTS))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let target_amount_cents = charity
        .and_then(|c| c.get(charity_fields::TARGET_AMOUNT_CENTS))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let currency_code = charity
        .and_then(|c| c.get(charity_fields::CURRENCY_CODE))
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_owned();

    let decimal_places = charity
        .and_then(|c| c.get(charity_fields::DECIMAL_PLACES))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    ArgStack::new()
        .set("charity.id".to_owned(), Variant::String(charity_id))
        .set("charity.name".to_owned(), Variant::String(charity_name))
        .set(
            "charity.current_amount_cents".to_owned(),
            Variant::Int(current_amount_cents),
        )
        .set(
            "charity.target_amount_cents".to_owned(),
            Variant::Int(target_amount_cents),
        )
        .set(
            "charity.currency_code".to_owned(),
            Variant::String(currency_code),
        )
        .set(
            "charity.decimal_places".to_owned(),
            Variant::Int(decimal_places),
        )
}
pub(super) fn build_charity_lifecycle_schema() -> VariableSchema {
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
                name: "charity.current_amount_cents".to_owned(),
                kind: VariantKind::Int,
                label: "Current amount (cents)".to_owned(),
                synthesis: Some(SynthesisHint::BoundedInt {
                    min: 0,
                    max: 1000000,
                }),
            },
            DeclaredVariable {
                name: "charity.target_amount_cents".to_owned(),
                kind: VariantKind::Int,
                label: "Target amount (cents)".to_owned(),
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
                name: "charity.decimal_places".to_owned(),
                kind: VariantKind::Int,
                label: "Amount decimal places".to_owned(),
                synthesis: Some(SynthesisHint::BoundedInt { min: 0, max: 3 }),
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lifecycle_event() -> Event {
        Event::new(
            EventSource::Twitch,
            "channel.charity_campaign.start",
            serde_json::json!({
                "charity": {
                    "id": "camp-9",
                    "name": "Rivers Fund",
                    "current_amount_cents": 12_000,
                    "target_amount_cents": 50_000,
                    "currency_code": "EUR",
                },
            }),
        )
    }

    #[test]
    fn lifecycle_arg_stack_maps_current_and_target_as_int_and_strings() {
        let stack = build_charity_lifecycle_arg_stack(&lifecycle_event());
        assert_eq!(
            stack.get("charity.current_amount_cents"),
            Some(&Variant::Int(12_000))
        );
        assert_eq!(
            stack.get("charity.target_amount_cents"),
            Some(&Variant::Int(50_000))
        );
        assert_eq!(
            stack.get("charity.id"),
            Some(&Variant::String("camp-9".to_owned()))
        );
        assert_eq!(
            stack.get("charity.name"),
            Some(&Variant::String("Rivers Fund".to_owned()))
        );
        assert_eq!(
            stack.get("charity.currency_code"),
            Some(&Variant::String("EUR".to_owned()))
        );
    }

    #[test]
    fn lifecycle_arg_stack_defaults_missing_amounts_to_zero_int() {
        let event = Event::new(
            EventSource::Twitch,
            "channel.charity_campaign.start",
            serde_json::json!({ "charity": { "id": "camp-x" } }),
        );
        let stack = build_charity_lifecycle_arg_stack(&event);
        assert_eq!(
            stack.get("charity.current_amount_cents"),
            Some(&Variant::Int(0))
        );
        assert_eq!(
            stack.get("charity.target_amount_cents"),
            Some(&Variant::Int(0))
        );
    }

    #[test]
    fn lifecycle_arg_stack_carries_the_amount_precision_the_currency_uses() {
        for (decimal_places, currency) in [(0, "JPY"), (2, "EUR"), (3, "KWD")] {
            let event = Event::new(
                EventSource::Twitch,
                "channel.charity_campaign.start",
                serde_json::json!({
                    "charity": { "currency_code": currency, "decimal_places": decimal_places },
                }),
            );

            let stack = build_charity_lifecycle_arg_stack(&event);

            assert_eq!(
                stack.get("charity.decimal_places"),
                Some(&Variant::Int(decimal_places)),
                "{currency}"
            );
        }
    }

    #[test]
    fn a_stored_lifecycle_event_without_a_precision_reads_as_zero_places() {
        let stack = build_charity_lifecycle_arg_stack(&lifecycle_event());

        assert_eq!(stack.get("charity.decimal_places"), Some(&Variant::Int(0)));
    }
}

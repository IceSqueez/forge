use super::families;
use super::spec::TwitchTriggerSpec;
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_registry::{ActorDeclaration, TriggerCategory};

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::SUPPORT_CHARITY_STARTED,
    category: TriggerCategory::Charity,
    label: "Charity campaign started",
    summary: "Fires when the broadcaster starts a charity campaign",
    search_text: "twitch charity campaign start begin",
    icon: "heart",
    kind: event_kinds::CHANNEL_CHARITY_CAMPAIGN_START,
    actors: ActorDeclaration::Actorless,
    variables: families::charity_variables,
    ..TwitchTriggerSpec::BASE
};

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::{Event, EventSource};
    use forge_registry::TriggerKindDescriptor;
    use forge_types::Variant;

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
        let stack = SPEC.build_arg_stack(&lifecycle_event());
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
        let stack = SPEC.build_arg_stack(&event);
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

            let stack = SPEC.build_arg_stack(&event);

            assert_eq!(
                stack.get("charity.decimal_places"),
                Some(&Variant::Int(decimal_places)),
                "{currency}"
            );
        }
    }

    #[test]
    fn a_stored_lifecycle_event_without_a_precision_reads_as_zero_places() {
        let stack = SPEC.build_arg_stack(&lifecycle_event());

        assert_eq!(stack.get("charity.decimal_places"), Some(&Variant::Int(0)));
    }
}

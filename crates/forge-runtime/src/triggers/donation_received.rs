use forge_events::{DONATION_RECEIVED_KIND, DonationReceived, Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, EventFilter, FormField, KindPlatformContract, LoginSlot,
    SourcedActor, TriggerCategory, TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{ActorRole, DeclaredVariable, TriggerConfig, Variant, VariantKind};

pub const DONATION_PROVIDER_OPTIONS_KEY: &str = "donations.provider_ids";

const PROVIDER_KEY: &str = "provider";

pub struct DonationReceivedDescriptor;

impl TriggerKindDescriptor for DonationReceivedDescriptor {
    fn id(&self) -> &str {
        DONATION_RECEIVED_KIND
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Donations
    }

    fn label(&self) -> &str {
        "Donation received"
    }

    fn summary(&self) -> &str {
        "Fires when a donation arrives through a connected donation service"
    }

    fn search_text(&self) -> &str {
        "donation donate tip money amount currency support donor"
    }

    fn icon_name(&self) -> &str {
        "currency-dollar"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::Universal
    }

    fn default_config(&self) -> TriggerConfig {
        TriggerConfig::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Optional {
            key: PROVIDER_KEY,
            label: "Donation service (leave empty to match any)",
            inner: Box::new(FormField::DynamicSelect {
                key: PROVIDER_KEY,
                label: "Donation service",
                options_key: DONATION_PROVIDER_OPTIONS_KEY,
            }),
        }]
    }

    fn condition_display(&self, config: &TriggerConfig) -> String {
        configured_provider(config).map_or_else(
            || "any donation service".to_owned(),
            |provider| format!("service = {provider}"),
        )
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Donation),
            kind_prefix: Some(DONATION_RECEIVED_KIND.to_owned()),
        }
    }

    fn matches_trigger(&self, config: &TriggerConfig, event: &Event) -> bool {
        let Some(donation) = DonationReceived::from_event(event) else {
            return false;
        };
        configured_provider(config).is_none_or(|provider| provider == donation.provider.as_str())
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .sourced_actor(ActorRole::Principal, LoginSlot::PlatformHasNone, donor)
                .message_text(|event| {
                    DonationReceived::from_event(event)
                        .and_then(|donation| donation.message)
                        .unwrap_or_default()
                })
                .money(|event| {
                    DonationReceived::from_event(event).map(|donation| donation.amount())
                })
                .event_specific(
                    declared("donation_provider", VariantKind::String, "Donation service"),
                    |event| {
                        Variant::String(
                            DonationReceived::from_event(event)
                                .map(|donation| donation.provider.as_str().to_owned())
                                .unwrap_or_default(),
                        )
                    },
                )
                .event_specific(
                    declared("donation_id", VariantKind::String, "Donation ID"),
                    |event| {
                        Variant::String(
                            DonationReceived::from_event(event)
                                .map(|donation| donation.donation_id)
                                .unwrap_or_default(),
                        )
                    },
                )
                .event_specific(
                    declared("donor_visibility", VariantKind::String, "Donor visibility"),
                    |event| {
                        Variant::String(
                            DonationReceived::from_event(event)
                                .map(|donation| donation.donor_visibility.as_str().to_owned())
                                .unwrap_or_default(),
                        )
                    },
                )
                .event_specific(
                    declared("donation_is_test", VariantKind::Bool, "Test donation"),
                    |event| {
                        Variant::Bool(
                            DonationReceived::from_event(event)
                                .is_some_and(|donation| donation.test),
                        )
                    },
                )
                .event_specific(
                    declared("donated_at", VariantKind::Datetime, "Donation time"),
                    |event| {
                        Variant::Datetime(
                            DonationReceived::from_event(event)
                                .map_or(event.timestamp, |donation| donation.occurred_at),
                        )
                    },
                ),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::principal()
    }
}

fn configured_provider(config: &TriggerConfig) -> Option<&str> {
    config
        .get(PROVIDER_KEY)
        .and_then(Variant::as_str)
        .map(str::trim)
        .filter(|provider| !provider.is_empty())
}

fn donor(event: &Event) -> SourcedActor {
    let donation = DonationReceived::from_event(event);
    SourcedActor {
        identity: ActorIdentity {
            id: String::new(),
            display_name: donation
                .as_ref()
                .map(|donation| donation.donor_name.clone())
                .unwrap_or_default(),
            login: None,
        },
        source: donation
            .map(|donation| donation.provider.as_str().to_owned())
            .unwrap_or_default(),
    }
}

fn declared(name: &str, kind: VariantKind, label: &str) -> DeclaredVariable {
    DeclaredVariable {
        name: name.to_owned(),
        kind,
        label: label.to_owned(),
        synthesis: None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::BTreeMap;

    use forge_registry::{SynthesisSample, declared_variables, synthesize_args};
    use forge_types::{CurrencyCode, Donation, DonationOrigin, Donor, IntegrationId, MoneyAmount};
    use time::OffsetDateTime;

    use super::*;

    const PLACEHOLDER: &str = "Anonymous donor";

    fn occurred_at() -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(1_790_000_000).unwrap()
    }

    fn donation_event(provider: &'static str, origin: DonationOrigin) -> Event {
        let donation = Donation {
            provider: IntegrationId::from_static(provider),
            donation_id: "d-42".to_owned(),
            donor: Donor::Named("Alice".to_owned()),
            message: Some("keep it up".to_owned()),
            amount: MoneyAmount::from_micros(12_500_000, CurrencyCode::parse("UAH").unwrap()),
            occurred_at: occurred_at(),
            origin,
        };
        DonationReceived::announce(&donation, PLACEHOLDER)
            .unwrap()
            .into_event()
            .unwrap()
    }

    fn provider_config(provider: Option<&str>) -> TriggerConfig {
        let mut config = TriggerConfig::new();
        if let Some(provider) = provider {
            config.insert(
                PROVIDER_KEY.to_owned(),
                Variant::String(provider.to_owned()),
            );
        }
        config
    }

    fn strings(pairs: &[(&str, &str)]) -> BTreeMap<String, Variant> {
        pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), Variant::String((*value).to_owned())))
            .collect()
    }

    #[test]
    fn provider_filter_matches_any_service_when_blank_and_only_the_named_one_otherwise() {
        let event = donation_event("donatello", DonationOrigin::Live);
        for (configured, matches) in [
            (None, true),
            (Some(""), true),
            (Some("   "), true),
            (Some("donatello"), true),
            (Some(" donatello "), true),
            (Some("monobank"), false),
        ] {
            assert_eq!(
                DonationReceivedDescriptor.matches_trigger(&provider_config(configured), &event),
                matches,
                "configured {configured:?}"
            );
        }
    }

    #[test]
    fn a_non_donation_event_never_matches_even_without_a_provider_filter() {
        let event = Event::new(
            EventSource::Donation,
            "donation.refunded",
            donation_event("donatello", DonationOrigin::Live).payload,
        );
        assert!(!DonationReceivedDescriptor.matches_trigger(&provider_config(None), &event));
    }

    #[test]
    fn a_donation_publishes_every_declared_variable_from_its_payload() {
        let event = donation_event("donatello", DonationOrigin::Test);
        let stack = DonationReceivedDescriptor
            .variables()
            .unwrap()
            .arg_stack(&event)
            .snapshot();

        let mut expected = strings(&[
            ("user_id", ""),
            ("user_name", "Alice"),
            ("user_platform", "donatello"),
            ("message_text", "keep it up"),
            ("currency", "UAH"),
            ("amount_formatted", "12.50 UAH"),
            ("donation_provider", "donatello"),
            ("donation_id", "d-42"),
            ("donor_visibility", "named"),
        ]);
        expected.insert("amount_micros".to_owned(), Variant::Int(12_500_000));
        expected.insert("amount".to_owned(), Variant::Float(12.5));
        expected.insert("donation_is_test".to_owned(), Variant::Bool(true));
        expected.insert("donated_at".to_owned(), Variant::Datetime(occurred_at()));
        assert_eq!(stack, expected);
    }

    #[test]
    fn a_corrupt_donation_payload_publishes_empty_values_and_the_event_time() {
        let event = Event::new(
            EventSource::Donation,
            DONATION_RECEIVED_KIND,
            serde_json::json!({ "provider": "donatello" }),
        );
        let stack = DonationReceivedDescriptor
            .variables()
            .unwrap()
            .arg_stack(&event)
            .snapshot();

        let mut expected = strings(&[
            ("user_id", ""),
            ("user_name", ""),
            ("user_platform", ""),
            ("message_text", ""),
            ("currency", ""),
            ("amount_formatted", ""),
            ("donation_provider", ""),
            ("donation_id", ""),
            ("donor_visibility", ""),
        ]);
        expected.insert("amount_micros".to_owned(), Variant::Int(0));
        expected.insert("amount".to_owned(), Variant::Float(0.0));
        expected.insert("donation_is_test".to_owned(), Variant::Bool(false));
        expected.insert("donated_at".to_owned(), Variant::Datetime(event.timestamp));
        assert_eq!(stack, expected);
    }

    #[test]
    fn test_run_synthesizes_one_coherent_money_sample() {
        let variables = declared_variables(&DonationReceivedDescriptor).unwrap();
        let args = synthesize_args(
            &variables,
            &SynthesisSample::stable(DonationReceivedDescriptor.platform_contract()),
        );

        let money: Vec<Option<&Variant>> =
            ["amount_micros", "amount", "currency", "amount_formatted"]
                .into_iter()
                .map(|name| args.get(name))
                .collect();
        assert_eq!(
            money,
            [
                Some(&Variant::Int(505_000_000)),
                Some(&Variant::Float(505.0)),
                Some(&Variant::String("UAH".to_owned())),
                Some(&Variant::String("505 UAH".to_owned())),
            ]
        );
    }
}

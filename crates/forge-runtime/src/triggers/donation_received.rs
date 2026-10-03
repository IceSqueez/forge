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

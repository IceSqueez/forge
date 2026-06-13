use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct CharityDonationDescriptor;

impl TriggerKindDescriptor for CharityDonationDescriptor {
    fn id(&self) -> &str {
        "twitch.support.charity_donation"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Bits
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
            .get("charity")
            .and_then(|c| c.get("amount_cents"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        amount_cents >= min_amount_cents
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let charity = event.payload.get("charity");
        let user = event.payload.get("user");

        let charity_id = charity
            .and_then(|c| c.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let charity_name = charity
            .and_then(|c| c.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let amount_cents = charity
            .and_then(|c| c.get("amount_cents"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let currency_code = charity
            .and_then(|c| c.get("currency_code"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let user_login = user
            .and_then(|u| u.get("login"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let user_display_name = user
            .and_then(|u| u.get("display_name"))
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
}

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct CharityStartedDescriptor;

impl TriggerKindDescriptor for CharityStartedDescriptor {
    fn id(&self) -> &str {
        "twitch.support.charity_started"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Bits
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

    fn condition_display(&self, _config: &TriggerConfig) -> String {
        "any".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Twitch),
            kind_prefix: Some("channel.charity_campaign.start".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        build_charity_lifecycle_arg_stack(event)
    }
}

pub(super) fn build_charity_lifecycle_arg_stack(event: &Event) -> ArgStack {
    let charity = event.payload.get("charity");

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
    let current_amount_cents = charity
        .and_then(|c| c.get("current_amount_cents"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let target_amount_cents = charity
        .and_then(|c| c.get("target_amount_cents"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let currency_code = charity
        .and_then(|c| c.get("currency_code"))
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_owned();

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
}

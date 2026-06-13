use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct AutomodTermsUpdatedDescriptor;

impl TriggerKindDescriptor for AutomodTermsUpdatedDescriptor {
    fn id(&self) -> &str {
        "twitch.automod.terms_updated"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Moderation
    }

    fn label(&self) -> &str {
        "AutoMod terms updated"
    }

    fn summary(&self) -> &str {
        "Fires when a moderator adds or removes permitted or blocked AutoMod terms"
    }

    fn search_text(&self) -> &str {
        "twitch automod terms blocked permitted moderator added removed updated"
    }

    fn icon_name(&self) -> &str {
        "shield"
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
            kind_prefix: Some("channel.automod.terms.update".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let moderator = event.payload.get("moderator");

        let moderator_login = moderator
            .and_then(|m| m.get("login"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let action = event
            .payload
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set(
                "moderator_login".to_owned(),
                Variant::String(moderator_login),
            )
            .set("automod.action".to_owned(), Variant::String(action))
    }
}

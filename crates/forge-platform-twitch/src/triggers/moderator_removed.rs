use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct ModeratorRemovedDescriptor;

impl TriggerKindDescriptor for ModeratorRemovedDescriptor {
    fn id(&self) -> &str {
        "twitch.channel.moderator_removed"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Moderation
    }

    fn label(&self) -> &str {
        "Moderator removed"
    }

    fn summary(&self) -> &str {
        "Fires when moderator status is revoked from a viewer"
    }

    fn search_text(&self) -> &str {
        "twitch moderator mod removed unmodded demoted moderation"
    }

    fn icon_name(&self) -> &str {
        "shield-x"
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
            kind_prefix: Some("channel.moderator.remove".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let user = event.payload.get("user");

        let user_login = user
            .and_then(|u| u.get("login"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let user_id = user
            .and_then(|u| u.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let user_name = user
            .and_then(|u| u.get("display_name"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("user_login".to_owned(), Variant::String(user_login))
            .set("user_id".to_owned(), Variant::String(user_id))
            .set("user_name".to_owned(), Variant::String(user_name))
    }
}

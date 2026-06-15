use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct UnbanRequestResolvedDescriptor;

impl TriggerKindDescriptor for UnbanRequestResolvedDescriptor {
    fn id(&self) -> &str {
        "twitch.channel.unban_request_resolved"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Moderation
    }

    fn label(&self) -> &str {
        "Unban request resolved"
    }

    fn summary(&self) -> &str {
        "Fires when a moderator approves, denies, or cancels an unban request"
    }

    fn search_text(&self) -> &str {
        "twitch unban request resolved approved denied canceled moderator moderation"
    }

    fn icon_name(&self) -> &str {
        "shield-check"
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
            kind_prefix: Some("channel.unban_request.resolve".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let request_id = event
            .payload
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        let user = event.payload.get("user");
        let user_login = user
            .and_then(|u| u.get("login"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        let resolution = event
            .payload
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        let moderator = event.payload.get("moderator");
        let moderator_login = moderator
            .and_then(|m| m.get("login"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        let resolution_text = event
            .payload
            .get("resolution_text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("unban.request_id".to_owned(), Variant::String(request_id))
            .set("unban.target.login".to_owned(), Variant::String(user_login))
            .set("unban.resolution".to_owned(), Variant::String(resolution))
            .set(
                "unban.moderator.login".to_owned(),
                Variant::String(moderator_login),
            )
            .set(
                "unban.resolution_text".to_owned(),
                Variant::String(resolution_text),
            )
    }
}

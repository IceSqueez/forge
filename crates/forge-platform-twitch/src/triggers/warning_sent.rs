use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct WarningSentDescriptor;

impl TriggerKindDescriptor for WarningSentDescriptor {
    fn id(&self) -> &str {
        "twitch.channel.warning_sent"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Moderation
    }

    fn label(&self) -> &str {
        "Warning sent"
    }

    fn summary(&self) -> &str {
        "Fires when a moderator issues a warning to a user"
    }

    fn search_text(&self) -> &str {
        "twitch warning sent moderator reason chat rules moderation"
    }

    fn icon_name(&self) -> &str {
        "bell-alert"
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
            kind_prefix: Some("channel.warning.send".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let user = event.payload.get("user");
        let moderator = event.payload.get("moderator");

        let user_login = user
            .and_then(|v| v.get("login"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let user_id = user
            .and_then(|v| v.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let user_name = user
            .and_then(|v| v.get("display_name"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        let moderator_login = moderator
            .and_then(|v| v.get("login"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        let reason = event
            .payload
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        let chat_rules_cited = event
            .payload
            .get("chat_rules_cited")
            .and_then(|v| v.as_array())
            .map(|arr| {
                Variant::Array(
                    arr.iter()
                        .filter_map(|s| s.as_str())
                        .map(|s| Variant::String(s.to_owned()))
                        .collect(),
                )
            })
            .unwrap_or_else(|| Variant::Array(vec![]));

        ArgStack::new()
            .set(
                "warning.target.login".to_owned(),
                Variant::String(user_login),
            )
            .set("warning.target.id".to_owned(), Variant::String(user_id))
            .set(
                "warning.target.display_name".to_owned(),
                Variant::String(user_name),
            )
            .set(
                "warning.moderator.login".to_owned(),
                Variant::String(moderator_login),
            )
            .set("warning.reason".to_owned(), Variant::String(reason))
            .set("warning.chat_rules_cited".to_owned(), chat_rules_cited)
    }
}

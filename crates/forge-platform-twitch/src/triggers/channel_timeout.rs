use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct ChannelTimeoutDescriptor;

impl TriggerKindDescriptor for ChannelTimeoutDescriptor {
    fn id(&self) -> &str {
        "twitch.channel.timeout"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Moderation
    }

    fn label(&self) -> &str {
        "User timed out"
    }

    fn summary(&self) -> &str {
        "Fires when a viewer is timed out from the channel"
    }

    fn search_text(&self) -> &str {
        "twitch timeout timed out moderation ban temporary"
    }

    fn icon_name(&self) -> &str {
        "clock"
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
            // channel.ban is the shared topic; is_permanent == false distinguishes timeouts.
            kind_prefix: Some("channel.ban".to_owned()),
        }
    }

    // channel.ban fires for both bans and timeouts; only fire for timeouts here.
    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        !event
            .payload
            .get("is_permanent")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let user = event.payload.get("user");
        let moderator = event.payload.get("moderator");

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
        let moderator_login = moderator
            .and_then(|m| m.get("login"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let reason = event
            .payload
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let banned_at = event
            .payload
            .get("banned_at")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let ends_at = event
            .payload
            .get("ends_at")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("user_login".to_owned(), Variant::String(user_login))
            .set("user_id".to_owned(), Variant::String(user_id))
            .set("user_name".to_owned(), Variant::String(user_name))
            .set(
                "moderator_login".to_owned(),
                Variant::String(moderator_login),
            )
            .set("reason".to_owned(), Variant::String(reason))
            .set("banned_at".to_owned(), Variant::String(banned_at))
            .set("ends_at".to_owned(), Variant::String(ends_at))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timeout_event() -> Event {
        let payload = serde_json::json!({
            "user": { "id": "777", "login": "viewer_one", "display_name": "ViewerOne" },
            "moderator": { "login": "mod_jane", "display_name": "ModJane" },
            "reason": "cooldown",
            "banned_at": "2026-06-13T10:00:00Z",
            "ends_at": "2026-06-13T10:10:00Z",
            "is_permanent": false,
        });
        Event::new(EventSource::Twitch, "channel.ban", payload)
    }

    // Timeout shares the channel.ban topic with the ban descriptor (the
    // is_permanent split is verified in channel_ban's centerpiece test).
    #[test]
    fn event_filter_targets_shared_channel_ban_topic_from_twitch() {
        let filter = ChannelTimeoutDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(filter.kind_prefix.as_deref(), Some("channel.ban"));
    }

    #[test]
    fn build_arg_stack_exposes_ends_at_alongside_ban_vars() {
        let stack = ChannelTimeoutDescriptor.build_arg_stack(&timeout_event());
        assert_eq!(
            stack.get("user_login"),
            Some(&Variant::String("viewer_one".to_owned()))
        );
        assert_eq!(
            stack.get("moderator_login"),
            Some(&Variant::String("mod_jane".to_owned()))
        );
        assert_eq!(
            stack.get("reason"),
            Some(&Variant::String("cooldown".to_owned()))
        );
        assert_eq!(
            stack.get("banned_at"),
            Some(&Variant::String("2026-06-13T10:00:00Z".to_owned()))
        );
        // ends_at is the timeout-only var that distinguishes it from a permanent ban.
        assert_eq!(
            stack.get("ends_at"),
            Some(&Variant::String("2026-06-13T10:10:00Z".to_owned()))
        );
    }
}

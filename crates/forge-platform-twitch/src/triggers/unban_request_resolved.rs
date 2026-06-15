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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_filter_targets_unban_request_resolve_topic_from_twitch() {
        let filter = UnbanRequestResolvedDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("channel.unban_request.resolve")
        );
    }

    #[test]
    fn build_arg_stack_maps_all_resolve_fields_from_publisher_payload() {
        // Payload shape mirrors `publish_unban_request_resolve_event`: the
        // `status` field surfaces as `unban.resolution`, and moderator data
        // lives under a nested `moderator` object.
        let payload = serde_json::json!({
            "id": "req-42",
            "user": { "login": "banned_viewer" },
            "status": "approved",
            "moderator": { "login": "mod_alice" },
            "resolution_text": "appeal accepted",
        });
        let event = Event::new(
            EventSource::Twitch,
            "channel.unban_request.resolve",
            payload,
        );
        let stack = UnbanRequestResolvedDescriptor.build_arg_stack(&event);

        for (key, expected) in [
            ("unban.request_id", "req-42"),
            ("unban.target.login", "banned_viewer"),
            ("unban.resolution", "approved"),
            ("unban.moderator.login", "mod_alice"),
            ("unban.resolution_text", "appeal accepted"),
        ] {
            assert_eq!(
                stack.get(key),
                Some(&Variant::String(expected.to_owned())),
                "wrong value for {key}"
            );
        }
    }

    #[test]
    fn build_arg_stack_uses_empty_strings_when_resolve_payload_is_empty() {
        let event = Event::new(
            EventSource::Twitch,
            "channel.unban_request.resolve",
            serde_json::json!({}),
        );
        let stack = UnbanRequestResolvedDescriptor.build_arg_stack(&event);

        for key in [
            "unban.request_id",
            "unban.target.login",
            "unban.resolution",
            "unban.moderator.login",
            "unban.resolution_text",
        ] {
            assert_eq!(
                stack.get(key),
                Some(&Variant::String(String::new())),
                "expected empty string for {key}"
            );
        }
    }
}

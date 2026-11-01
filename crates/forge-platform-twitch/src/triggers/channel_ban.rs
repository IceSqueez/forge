use super::spec::TwitchTriggerSpec;
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_events::Event;
use forge_registry::{ActorDeclaration, ActorRole, TriggerCategory, TriggerVariables};
use forge_types::{DeclaredVariable, TriggerConfig, Variant, VariantKind};

use super::payload_read::{self, twitch_actor};
use crate::vocabulary::fields::moderation as fields;

fn matches(_config: &TriggerConfig, event: &Event) -> bool {
    event
        .payload
        .get(fields::IS_PERMANENT)
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

fn variables() -> Option<TriggerVariables> {
    Some(
        TriggerVariables::new()
            .actor(
                twitch_actor(ActorRole::Principal),
                payload_read::moderated_user_identity,
            )
            .actor(
                twitch_actor(ActorRole::Moderator),
                payload_read::acting_moderator_identity,
            )
            .event_specific(
                DeclaredVariable {
                    name: "reason".to_owned(),
                    kind: VariantKind::String,
                    label: "Ban reason".to_owned(),
                    synthesis: None,
                },
                |event| Variant::String(payload_read::text(event, fields::REASON)),
            )
            .event_specific_optional(
                DeclaredVariable {
                    name: "banned_at".to_owned(),
                    kind: VariantKind::Datetime,
                    label: "Banned at".to_owned(),
                    synthesis: None,
                },
                |event| payload_read::timestamp(event, fields::BANNED_AT),
            ),
    )
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::CHANNEL_BAN,
    category: TriggerCategory::Moderation,
    label: "User banned (permanent)",
    summary: "Fires when a viewer is permanently banned from the channel",
    search_text: "twitch ban banned permanent moderation",
    icon: "ban",
    kind: event_kinds::CHANNEL_BAN,
    actors: ActorDeclaration::Actors(&[ActorRole::Moderator]),
    variables,
    matches,
    ..TwitchTriggerSpec::BASE
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timestamp::tests::instant;
    use crate::triggers::channel_timeout::SPEC as CHANNEL_TIMEOUT;
    use forge_events::EventSource;
    use forge_registry::TriggerKindDescriptor;

    fn ban_event(is_permanent: bool) -> Event {
        let payload = serde_json::json!({
            "user": { "id": "777", "login": "viewer_one", "display_name": "ViewerOne" },
            "moderator": { "login": "mod_jane", "display_name": "ModJane" },
            "reason": "spamming",
            "banned_at": "2026-06-13T10:00:00Z",
            "ends_at": "2026-06-13T10:10:00Z",
            "is_permanent": is_permanent,
        });
        Event::new(EventSource::Twitch, "twitch.channel.ban", payload)
    }

    #[test]
    fn is_permanent_routes_ban_and_timeout_to_opposite_descriptors() {
        let cfg = TriggerConfig::new();

        let permanent = ban_event(true);
        assert!(
            SPEC.matches_trigger(&cfg, &permanent),
            "permanent ban must fire the ban descriptor"
        );
        assert!(
            !CHANNEL_TIMEOUT.matches_trigger(&cfg, &permanent),
            "permanent ban must NOT fire the timeout descriptor"
        );

        let timeout = ban_event(false);
        assert!(
            !SPEC.matches_trigger(&cfg, &timeout),
            "timeout must NOT fire the ban descriptor"
        );
        assert!(
            CHANNEL_TIMEOUT.matches_trigger(&cfg, &timeout),
            "timeout must fire the timeout descriptor"
        );
    }

    #[test]
    fn missing_is_permanent_defaults_to_timeout_not_ban() {
        let cfg = TriggerConfig::new();
        let payload = serde_json::json!({
            "user": { "id": "1", "login": "x", "display_name": "X" },
            "moderator": { "login": "m", "display_name": "M" },
        });
        let event = Event::new(EventSource::Twitch, "twitch.channel.ban", payload);
        assert!(!SPEC.matches_trigger(&cfg, &event));
        assert!(CHANNEL_TIMEOUT.matches_trigger(&cfg, &event));
    }

    #[test]
    fn event_filter_targets_channel_ban_topic_from_twitch() {
        let filter = SPEC.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(filter.kind_prefix.as_deref(), Some("twitch.channel.ban"));
    }

    #[test]
    fn build_arg_stack_exposes_user_moderator_reason_and_banned_at() {
        let stack = SPEC.build_arg_stack(&ban_event(true));
        assert_eq!(
            stack.get("user_login"),
            Some(&Variant::String("viewer_one".to_owned()))
        );
        assert_eq!(
            stack.get("user_id"),
            Some(&Variant::String("777".to_owned()))
        );
        assert_eq!(
            stack.get("user_name"),
            Some(&Variant::String("ViewerOne".to_owned()))
        );
        assert_eq!(
            stack.get("moderator_login"),
            Some(&Variant::String("mod_jane".to_owned()))
        );
        assert_eq!(
            stack.get("reason"),
            Some(&Variant::String("spamming".to_owned()))
        );
        assert_eq!(
            stack.get("banned_at"),
            Some(&instant("2026-06-13T10:00:00Z"))
        );
        assert_eq!(stack.get("ends_at"), None);
    }

    #[test]
    fn a_permanent_ban_with_a_null_end_still_publishes_when_it_happened() {
        let mut event = ban_event(true);
        event.payload["ends_at"] = serde_json::Value::Null;

        let stack = SPEC.build_arg_stack(&event);

        assert_eq!(
            stack.get("banned_at"),
            Some(&instant("2026-06-13T10:00:00Z"))
        );
    }
}

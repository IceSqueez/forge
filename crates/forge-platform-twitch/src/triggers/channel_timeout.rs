use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorRole, EventFilter, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{DeclaredVariable, PlatformId, TriggerConfig, Variant, VariantKind};

use super::payload_read::{self, twitch_actor};
use crate::vocabulary::fields::moderation as fields;

pub(crate) struct ChannelTimeoutDescriptor;

impl TriggerKindDescriptor for ChannelTimeoutDescriptor {
    fn id(&self) -> &str {
        trigger_ids::CHANNEL_TIMEOUT
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

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Twitch),
            kind_prefix: Some(event_kinds::CHANNEL_BAN.to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        !event
            .payload
            .get(fields::IS_PERMANENT)
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    fn variables(&self) -> Option<TriggerVariables> {
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
                        label: "Timeout reason".to_owned(),
                        synthesis: None,
                    },
                    |event| Variant::String(payload_read::text(event, fields::REASON)),
                )
                .event_specific_optional(
                    DeclaredVariable {
                        name: "banned_at".to_owned(),
                        kind: VariantKind::Datetime,
                        label: "Timed out at".to_owned(),
                        synthesis: None,
                    },
                    |event| payload_read::timestamp(event, fields::BANNED_AT),
                )
                .event_specific_optional(
                    DeclaredVariable {
                        name: "ends_at".to_owned(),
                        kind: VariantKind::Datetime,
                        label: "Timeout ends at".to_owned(),
                        synthesis: None,
                    },
                    |event| payload_read::timestamp(event, fields::ENDS_AT),
                ),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::Actors(&[ActorRole::Moderator])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timestamp::tests::instant;

    fn timeout_event() -> Event {
        let payload = serde_json::json!({
            "user": { "id": "777", "login": "viewer_one", "display_name": "ViewerOne" },
            "moderator": { "login": "mod_jane", "display_name": "ModJane" },
            "reason": "cooldown",
            "banned_at": "2026-06-13T10:00:00Z",
            "ends_at": "2026-06-13T10:10:00Z",
            "is_permanent": false,
        });
        Event::new(EventSource::Twitch, "twitch.channel.ban", payload)
    }

    #[test]
    fn event_filter_targets_shared_channel_ban_topic_from_twitch() {
        let filter = ChannelTimeoutDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(filter.kind_prefix.as_deref(), Some("twitch.channel.ban"));
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
            Some(&instant("2026-06-13T10:00:00Z"))
        );
        assert_eq!(stack.get("ends_at"), Some(&instant("2026-06-13T10:10:00Z")));
    }

    #[test]
    fn a_timeout_without_an_end_moment_omits_ends_at_and_keeps_banned_at() {
        for ends_at in [serde_json::Value::Null, serde_json::json!("")] {
            let mut event = timeout_event();
            event.payload["ends_at"] = ends_at.clone();

            let stack = ChannelTimeoutDescriptor.build_arg_stack(&event);

            assert_eq!(stack.get("ends_at"), None, "{ends_at}");
            assert_eq!(
                stack.get("banned_at"),
                Some(&instant("2026-06-13T10:00:00Z")),
                "{ends_at}"
            );
        }
    }
}

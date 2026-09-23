use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{ActorRole, DeclaredVariable, PlatformId, TriggerConfig, Variant, VariantKind};

use super::payload_read::{self, twitch_actor};
use crate::payload_fields::moderation as fields;

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
            kind_prefix: Some("twitch.channel.ban".to_owned()),
        }
    }

    // channel.ban fires for both bans and timeouts; only fire for timeouts (is_permanent == false).
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
                .actor(twitch_actor(ActorRole::Principal), moderated_user_identity)
                .actor(
                    twitch_actor(ActorRole::Moderator),
                    acting_moderator_identity,
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
                .event_specific(
                    DeclaredVariable {
                        name: "banned_at".to_owned(),
                        kind: VariantKind::String,
                        label: "Timed out at".to_owned(),
                        synthesis: None,
                    },
                    |event| Variant::String(payload_read::text(event, fields::BANNED_AT)),
                )
                .event_specific(
                    DeclaredVariable {
                        name: "ends_at".to_owned(),
                        kind: VariantKind::String,
                        label: "Timeout ends at".to_owned(),
                        synthesis: None,
                    },
                    |event| Variant::String(payload_read::text(event, fields::ENDS_AT)),
                ),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::Actors(&[ActorRole::Moderator])
    }
}

fn moderated_user_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(
        event.payload.get(fields::USER),
        fields::USER_ID,
        fields::USER_LOGIN,
        fields::USER_DISPLAY_NAME,
    )
}

fn acting_moderator_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(
        event.payload.get(fields::MODERATOR),
        fields::MODERATOR_ID,
        fields::MODERATOR_LOGIN,
        fields::MODERATOR_DISPLAY_NAME,
    )
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
            Some(&Variant::String("2026-06-13T10:00:00Z".to_owned()))
        );
        assert_eq!(
            stack.get("ends_at"),
            Some(&Variant::String("2026-06-13T10:10:00Z".to_owned()))
        );
    }
}

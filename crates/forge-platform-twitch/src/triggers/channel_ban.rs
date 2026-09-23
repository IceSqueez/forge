use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{ActorRole, DeclaredVariable, PlatformId, TriggerConfig, Variant, VariantKind};

use super::payload_read::{self, twitch_actor};
use crate::payload_fields::moderation as fields;

pub(crate) struct ChannelBanDescriptor;

impl TriggerKindDescriptor for ChannelBanDescriptor {
    fn id(&self) -> &str {
        "twitch.channel.ban"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Moderation
    }

    fn label(&self) -> &str {
        "User banned (permanent)"
    }

    fn summary(&self) -> &str {
        "Fires when a viewer is permanently banned from the channel"
    }

    fn search_text(&self) -> &str {
        "twitch ban banned permanent moderation"
    }

    fn icon_name(&self) -> &str {
        "ban"
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

    // channel.ban fires for both bans and timeouts; only fire for permanent bans here.
    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event
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
                        label: "Ban reason".to_owned(),
                        synthesis: None,
                    },
                    |event| Variant::String(payload_read::text(event, fields::REASON)),
                )
                .event_specific(
                    DeclaredVariable {
                        name: "banned_at".to_owned(),
                        kind: VariantKind::String,
                        label: "Banned at".to_owned(),
                        synthesis: None,
                    },
                    |event| Variant::String(payload_read::text(event, fields::BANNED_AT)),
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
    use crate::triggers::channel_timeout::ChannelTimeoutDescriptor;

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
            ChannelBanDescriptor.matches_trigger(&cfg, &permanent),
            "permanent ban must fire the ban descriptor"
        );
        assert!(
            !ChannelTimeoutDescriptor.matches_trigger(&cfg, &permanent),
            "permanent ban must NOT fire the timeout descriptor"
        );

        let timeout = ban_event(false);
        assert!(
            !ChannelBanDescriptor.matches_trigger(&cfg, &timeout),
            "timeout must NOT fire the ban descriptor"
        );
        assert!(
            ChannelTimeoutDescriptor.matches_trigger(&cfg, &timeout),
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
        assert!(!ChannelBanDescriptor.matches_trigger(&cfg, &event));
        assert!(ChannelTimeoutDescriptor.matches_trigger(&cfg, &event));
    }

    #[test]
    fn event_filter_targets_channel_ban_topic_from_twitch() {
        let filter = ChannelBanDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(filter.kind_prefix.as_deref(), Some("twitch.channel.ban"));
    }

    #[test]
    fn build_arg_stack_exposes_user_moderator_reason_and_banned_at() {
        let stack = ChannelBanDescriptor.build_arg_stack(&ban_event(true));
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
            Some(&Variant::String("2026-06-13T10:00:00Z".to_owned()))
        );
        assert_eq!(stack.get("ends_at"), None);
    }
}

use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{ActorRole, PlatformId, TriggerConfig};

use super::payload_read::{self, twitch_actor};
use crate::payload_fields::moderation as fields;

pub(crate) struct ChannelUnbanDescriptor;

impl TriggerKindDescriptor for ChannelUnbanDescriptor {
    fn id(&self) -> &str {
        "twitch.channel.unban"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Moderation
    }

    fn label(&self) -> &str {
        "User unbanned"
    }

    fn summary(&self) -> &str {
        "Fires when a viewer's ban or timeout is lifted"
    }

    fn search_text(&self) -> &str {
        "twitch unban unbanned moderation pardon"
    }

    fn icon_name(&self) -> &str {
        "check"
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
            kind_prefix: Some("twitch.channel.unban".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(twitch_actor(ActorRole::Principal), moderated_user_identity)
                .actor(
                    twitch_actor(ActorRole::Moderator),
                    acting_moderator_identity,
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
    use forge_types::Variant;

    fn unban_event() -> Event {
        let payload = serde_json::json!({
            "user": { "id": "777", "login": "viewer_one", "display_name": "ViewerOne" },
            "moderator": { "login": "mod_jane", "display_name": "ModJane" },
        });
        Event::new(EventSource::Twitch, "twitch.channel.unban", payload)
    }

    #[test]
    fn event_filter_targets_channel_unban_topic_from_twitch() {
        let filter = ChannelUnbanDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(filter.kind_prefix.as_deref(), Some("twitch.channel.unban"));
    }

    #[test]
    fn build_arg_stack_exposes_user_and_moderator_without_ban_specific_vars() {
        let stack = ChannelUnbanDescriptor.build_arg_stack(&unban_event());
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
        assert_eq!(stack.get("reason"), None);
        assert_eq!(stack.get("banned_at"), None);
        assert_eq!(stack.get("ends_at"), None);
    }
}

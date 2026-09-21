use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{ActorRole, DeclaredVariable, PlatformId, TriggerConfig, Variant, VariantKind};

use super::payload_read::{self, twitch_actor};
use crate::payload_fields::follow as fields;

pub(crate) struct ChannelFollowDescriptor;

impl TriggerKindDescriptor for ChannelFollowDescriptor {
    fn id(&self) -> &str {
        "twitch.channel.follow"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Users
    }

    fn label(&self) -> &str {
        "Follow"
    }

    fn summary(&self) -> &str {
        "Fires when a viewer follows your channel"
    }

    fn search_text(&self) -> &str {
        "twitch follow follower new"
    }

    fn icon_name(&self) -> &str {
        "heart"
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
            kind_prefix: Some("twitch.channel.follow".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(twitch_actor(ActorRole::Principal), follower_identity)
                .event_specific(
                    DeclaredVariable {
                        name: "followed_at".to_owned(),
                        kind: VariantKind::String,
                        label: "Followed at".to_owned(),
                        synthesis: None,
                    },
                    |event| Variant::String(payload_read::text(event, fields::FOLLOWED_AT)),
                ),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::principal()
    }
}

fn follower_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(
        event.payload.get(fields::USER),
        fields::USER_ID,
        fields::USER_LOGIN,
        fields::USER_DISPLAY_NAME,
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn follow_event() -> Event {
        Event::new(
            EventSource::Twitch,
            "channel.follow",
            serde_json::json!({
                "followed_at": "2026-06-13T10:00:00Z",
                "user": { "id": "42", "login": "new_follower", "display_name": "NewFollower" }
            }),
        )
    }

    #[test]
    fn event_filter_gates_on_follow_kind_prefix() {
        let filter = ChannelFollowDescriptor.event_filter();
        assert_eq!(filter.kind_prefix.as_deref(), Some("twitch.channel.follow"));
        assert_eq!(filter.source, Some(EventSource::Twitch));
    }

    #[test]
    fn build_arg_stack_maps_user_and_followed_at_from_nested_payload() {
        let stack = ChannelFollowDescriptor.build_arg_stack(&follow_event());
        assert_eq!(
            stack.get("user_login"),
            Some(&Variant::String("new_follower".to_owned()))
        );
        assert_eq!(
            stack.get("user_id"),
            Some(&Variant::String("42".to_owned()))
        );
        assert_eq!(
            stack.get("user_name"),
            Some(&Variant::String("NewFollower".to_owned()))
        );
        assert_eq!(
            stack.get("followed_at"),
            Some(&Variant::String("2026-06-13T10:00:00Z".to_owned()))
        );
    }
}

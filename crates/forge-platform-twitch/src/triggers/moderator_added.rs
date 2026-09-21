use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{ActorRole, PlatformId, TriggerConfig};

use super::payload_read::{self, twitch_actor};
use crate::payload_fields::moderator as moderator_fields;

pub(crate) struct ModeratorAddedDescriptor;

impl TriggerKindDescriptor for ModeratorAddedDescriptor {
    fn id(&self) -> &str {
        "twitch.channel.moderator_added"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Moderation
    }

    fn label(&self) -> &str {
        "Moderator added"
    }

    fn summary(&self) -> &str {
        "Fires when a viewer is granted moderator status in the channel"
    }

    fn search_text(&self) -> &str {
        "twitch moderator mod added promoted moderation"
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
            kind_prefix: Some("twitch.channel.moderator.add".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(twitch_actor(ActorRole::Principal), affected_user_identity),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::principal()
    }
}

fn affected_user_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(
        event.payload.get(moderator_fields::USER),
        moderator_fields::USER_ID,
        moderator_fields::USER_LOGIN,
        moderator_fields::USER_DISPLAY_NAME,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_types::Variant;

    fn moderator_add_event() -> Event {
        let payload = serde_json::json!({
            "user": { "id": "777", "login": "viewer_one", "display_name": "ViewerOne" },
        });
        Event::new(EventSource::Twitch, "twitch.channel.moderator.add", payload)
    }

    #[test]
    fn event_filter_targets_moderator_add_topic_from_twitch() {
        let filter = ModeratorAddedDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("twitch.channel.moderator.add")
        );
    }

    #[test]
    fn build_arg_stack_maps_user_fields_from_nested_payload() {
        let stack = ModeratorAddedDescriptor.build_arg_stack(&moderator_add_event());
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
    }
}

use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{ActorRole, PlatformId, TriggerConfig};

use super::payload_read::{self, twitch_actor};
use crate::payload_fields::moderator as moderator_fields;

pub(crate) struct ModeratorRemovedDescriptor;

impl TriggerKindDescriptor for ModeratorRemovedDescriptor {
    fn id(&self) -> &str {
        "twitch.channel.moderator_removed"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Moderation
    }

    fn label(&self) -> &str {
        "Moderator removed"
    }

    fn summary(&self) -> &str {
        "Fires when moderator status is revoked from a viewer"
    }

    fn search_text(&self) -> &str {
        "twitch moderator mod removed unmodded demoted moderation"
    }

    fn icon_name(&self) -> &str {
        "shield-x"
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
            kind_prefix: Some("twitch.channel.moderator.remove".to_owned()),
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

    fn moderator_remove_event() -> Event {
        let payload = serde_json::json!({
            "user": { "id": "888", "login": "ex_mod", "display_name": "ExMod" },
        });
        Event::new(
            EventSource::Twitch,
            "twitch.channel.moderator.remove",
            payload,
        )
    }

    #[test]
    fn event_filter_targets_moderator_remove_topic_from_twitch() {
        let filter = ModeratorRemovedDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("twitch.channel.moderator.remove")
        );
    }

    #[test]
    fn build_arg_stack_maps_user_fields_from_nested_payload() {
        let stack = ModeratorRemovedDescriptor.build_arg_stack(&moderator_remove_event());
        assert_eq!(
            stack.get("user_login"),
            Some(&Variant::String("ex_mod".to_owned()))
        );
        assert_eq!(
            stack.get("user_id"),
            Some(&Variant::String("888".to_owned()))
        );
        assert_eq!(
            stack.get("user_name"),
            Some(&Variant::String("ExMod".to_owned()))
        );
    }
}

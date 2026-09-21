use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{ActorRole, PlatformId, TriggerConfig};

use super::payload_read::{self, twitch_actor};
use crate::payload_fields::warning as warning_fields;

pub(crate) struct WarningAcknowledgedDescriptor;

impl TriggerKindDescriptor for WarningAcknowledgedDescriptor {
    fn id(&self) -> &str {
        "twitch.channel.warning_acknowledged"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Moderation
    }

    fn label(&self) -> &str {
        "Warning acknowledged"
    }

    fn summary(&self) -> &str {
        "Fires when a user acknowledges a warning issued by a moderator"
    }

    fn search_text(&self) -> &str {
        "twitch warning acknowledged user moderation"
    }

    fn icon_name(&self) -> &str {
        "bell-check"
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
            kind_prefix: Some("twitch.channel.warning.acknowledge".to_owned()),
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
        event.payload.get(warning_fields::USER),
        warning_fields::USER_ID,
        warning_fields::USER_LOGIN,
        warning_fields::USER_DISPLAY_NAME,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_types::Variant;

    fn warning_acknowledged_event() -> Event {
        let payload = serde_json::json!({
            "user": { "id": "654", "login": "warned_user", "display_name": "WarnedUser" },
        });
        Event::new(
            EventSource::Twitch,
            "twitch.channel.warning.acknowledge",
            payload,
        )
    }

    #[test]
    fn event_filter_targets_warning_acknowledge_topic_from_twitch() {
        let filter = WarningAcknowledgedDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("twitch.channel.warning.acknowledge")
        );
    }

    #[test]
    fn build_arg_stack_maps_user_fields_from_nested_payload() {
        let stack = WarningAcknowledgedDescriptor.build_arg_stack(&warning_acknowledged_event());
        assert_eq!(
            stack.get("user_login"),
            Some(&Variant::String("warned_user".to_owned()))
        );
        assert_eq!(
            stack.get("user_id"),
            Some(&Variant::String("654".to_owned()))
        );
        assert_eq!(
            stack.get("user_name"),
            Some(&Variant::String("WarnedUser".to_owned()))
        );
    }
}

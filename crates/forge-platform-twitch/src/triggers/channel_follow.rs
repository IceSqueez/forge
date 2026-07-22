use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

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
            kind_prefix: Some("channel.follow".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let user_login = event
            .payload
            .get(fields::USER)
            .and_then(|u| u.get(fields::USER_LOGIN))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let user_id = event
            .payload
            .get(fields::USER)
            .and_then(|u| u.get(fields::USER_ID))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let user_name = event
            .payload
            .get(fields::USER)
            .and_then(|u| u.get(fields::USER_DISPLAY_NAME))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let followed_at = event
            .payload
            .get(fields::FOLLOWED_AT)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("user_login".to_owned(), Variant::String(user_login))
            .set("user_id".to_owned(), Variant::String(user_id))
            .set("user_name".to_owned(), Variant::String(user_name))
            .set("followed_at".to_owned(), Variant::String(followed_at))
    }
    fn output_schema(&self) -> Option<VariableSchema> {
        Some({
            VariableSchema {
                variables: vec![
                    DeclaredVariable {
                        name: "user_login".to_owned(),
                        kind: VariantKind::String,
                        label: "Follower login".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                    DeclaredVariable {
                        name: "user_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Follower ID".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "user_name".to_owned(),
                        kind: VariantKind::String,
                        label: "Follower display name".to_owned(),
                        synthesis: Some(SynthesisHint::DisplayName),
                    },
                    DeclaredVariable {
                        name: "followed_at".to_owned(),
                        kind: VariantKind::String,
                        label: "Followed at".to_owned(),
                        synthesis: None,
                    },
                ],
            }
        })
    }
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
        assert_eq!(filter.kind_prefix.as_deref(), Some("channel.follow"));
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

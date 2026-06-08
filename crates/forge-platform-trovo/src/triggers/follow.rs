use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

use super::chat::str_field;

pub(crate) struct FollowDescriptor;

impl TriggerKindDescriptor for FollowDescriptor {
    fn id(&self) -> &str {
        "trovo.follow"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Users
    }

    fn label(&self) -> &str {
        "Follow"
    }

    fn summary(&self) -> &str {
        "Fires when a viewer follows the Trovo channel"
    }

    fn search_text(&self) -> &str {
        "trovo follow new follower viewer channel"
    }

    fn icon_name(&self) -> &str {
        "user-plus"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::Trovo)
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
            source: Some(EventSource::Trovo),
            kind_prefix: Some("trovo.follow".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let nick_name = str_field(&event.payload, "nick_name");
        let user_name = str_field(&event.payload, "user_name");
        let sender_id = str_field(&event.payload, "sender_id");

        ArgStack::new()
            .set("nick_name".to_owned(), Variant::String(nick_name))
            .set("user_name".to_owned(), Variant::String(user_name))
            .set("sender_id".to_owned(), Variant::String(sender_id))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_events::EventSource;
    use forge_types::Variant;

    fn follow_event() -> Event {
        Event::new(
            EventSource::Trovo,
            "trovo.follow",
            serde_json::json!({
                "content": "",
                "nick_name": "NewFollower",
                "user_name": "follower_login",
                "sender_id": "uid_follow"
            }),
        )
    }

    #[test]
    fn build_arg_stack_extracts_user_fields() {
        let stack = FollowDescriptor.build_arg_stack(&follow_event());
        assert_eq!(
            stack.get("nick_name"),
            Some(&Variant::String("NewFollower".to_owned()))
        );
        assert_eq!(
            stack.get("user_name"),
            Some(&Variant::String("follower_login".to_owned()))
        );
        assert_eq!(
            stack.get("sender_id"),
            Some(&Variant::String("uid_follow".to_owned()))
        );
        assert_eq!(stack.get("content"), None, "follow arg stack omits content");
    }
}
